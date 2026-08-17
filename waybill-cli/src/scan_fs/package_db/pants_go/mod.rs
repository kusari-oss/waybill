//! Milestone 226: Pants Go reader — BUILD-walker enrichment + `[golang]` toolchain pin.
//!
//! Two entry points:
//! - [`read`] emits ONE design-tier `pkg:generic/go@<version>`
//!   component when `pants.toml` `[golang] expected_version` is set.
//!   Called from `read_all` alongside m225's `pants_shell::read`.
//! - [`enrich`] runs AFTER m191's `reconcile_design_source_tiers`
//!   (before m148 canonicalization) and injects
//!   `waybill:pants-target` annotations onto every existing
//!   `pkg:golang/*` component the Go reader emitted from `go.sum`
//!   entries. **Zero fabrication** (FR-012 / Principle IX): the
//!   enrichment NEVER pushes new components into the vector.
//!
//! See `specs/226-pants-go-reader/` for spec + plan + contracts.

pub mod build_dsl;
pub mod config;
pub mod enrichment;
pub mod ownership_index;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::json;
use waybill_common::resolution::{LifecycleScope, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::exclude_path::ExclusionSet;
use super::pants_common;
use super::PackageDbEntry;

/// The four built-in Pants Go backend target types we recognize.
///
/// All four variants prefix with `Go` because they mirror the
/// upstream Pants target-function names (`go_mod`, `go_binary`, etc.)
/// — that upstream naming is authoritative; clippy's shared-prefix
/// suggestion (`Mod`, `Binary`) would obscure the mapping.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoTargetKind {
    /// `go_mod(name="mod")` — implicit owner of every go.sum entry in the dir.
    GoMod,
    /// `go_third_party_package(name="X", import_path="example.com/foo")` — one dep.
    GoThirdPartyPackage,
    /// `go_binary(name="X", main="./cmd/foo")` — a buildable Go binary.
    GoBinary,
    /// `go_package(name="X")` — a Go package source directory.
    GoPackage,
}

impl GoTargetKind {
    /// Function-call name (matches the DSL target-function identifier).
    /// Currently used only by unit tests + WARN diagnostics.
    #[allow(dead_code)]
    pub(crate) fn as_dsl_name(self) -> &'static str {
        match self {
            Self::GoMod => "go_mod",
            Self::GoThirdPartyPackage => "go_third_party_package",
            Self::GoBinary => "go_binary",
            Self::GoPackage => "go_package",
        }
    }
}

/// One parsed target declaration extracted from a BUILD file.
#[derive(Debug, Clone)]
pub(crate) struct GoTargetDeclaration {
    pub(crate) kind: GoTargetKind,
    /// `name=` kwarg value. `None` when omitted (Pants defaults:
    /// `"mod"` for go_mod, dir basename for go_package; other kinds
    /// require an explicit name).
    pub(crate) name: Option<String>,
    /// `import_path=` kwarg. Only populated for `go_third_party_package`.
    pub(crate) import_path: Option<String>,
    /// `main=` kwarg. Path relative to BUILD dir. Only populated for
    /// `go_binary`.
    pub(crate) main: Option<String>,
    /// 1-based line number of the target's opening `(` for diagnostics.
    /// Surfaced only through `GoTargetParseError` variants.
    #[allow(dead_code)]
    pub(crate) start_line: u32,
}

/// Canonical Pants target address newtype. `<dir>:<name>` for
/// subdirectory BUILDs; bare `<name>` for the root BUILD.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TargetAddress(pub(crate) String);

impl std::fmt::Display for TargetAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The enrichment-pass lookup structure, built once per scan from
/// all parsed BUILD-file Go target declarations.
#[derive(Debug, Default)]
pub(crate) struct GoOwnershipIndex {
    /// `go_mod` BUILD directory → target address. Longest-prefix
    /// match against a component's `source_path` wins per R3.
    /// `BTreeMap` gives sorted-key iteration for determinism.
    pub(crate) go_mod_roots: BTreeMap<PathBuf, TargetAddress>,
    /// import_path → list of `go_third_party_package` addresses.
    /// Multiple targets can claim the same import path (rare).
    pub(crate) import_path_to_addresses: HashMap<String, Vec<TargetAddress>>,
    /// (main_package_absolute_dir, address) for every `go_binary(main=...)`.
    pub(crate) main_targets: Vec<(PathBuf, TargetAddress)>,
    /// (package_absolute_dir, address) for every `go_package`.
    pub(crate) package_targets: Vec<(PathBuf, TargetAddress)>,
}

/// Parse-time failure modes for a single target declaration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum GoTargetParseError {
    #[error("target has no name= or required kwarg (line {line})")]
    MissingRequiredKwarg { line: u32 },
    #[error("target has non-string-literal expression at line {line}: {snippet}")]
    NonStringLiteralValue { line: u32, snippet: String },
    #[error("unbalanced parens starting at line {line}")]
    UnbalancedParens { line: u32 },
}

/// Public entry — emits the design-tier `pkg:generic/go@<version>`
/// component when `pants.toml` `[golang] expected_version` is set.
/// Returns `Vec::new()` otherwise. Called from `read_all`.
///
/// The `exclude_set` parameter is unused here (kept for API symmetry
/// with the other Pants-family readers).
pub fn read(scan_root: &Path, _exclude_set: &ExclusionSet) -> Vec<PackageDbEntry> {
    let pants_toml = scan_root.join("pants.toml");
    if !pants_toml.is_file() {
        return Vec::new();
    }
    let bytes = match std::fs::read(&pants_toml) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                pants_toml = %pants_toml.display(),
                error = %e,
                "pants-go reader: pants.toml could not be read; skipping toolchain-pin emission"
            );
            return Vec::new();
        }
    };
    let cfg = match config::parse(&bytes) {
        Some(c) => c,
        None => {
            tracing::warn!(
                pants_toml = %pants_toml.display(),
                "pants-go reader: pants.toml could not be parsed as TOML; skipping toolchain-pin emission"
            );
            return Vec::new();
        }
    };
    let Some(golang) = cfg.golang else {
        return Vec::new();
    };
    let Some(version) = golang.expected_version.as_deref().filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let purl_str = format!("pkg:generic/go@{}", encode_purl_segment(version));
    let purl = match Purl::new(&purl_str) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                version = %version,
                error = %e,
                "pants-go reader: PURL construction failed for [golang] expected_version; skipping"
            );
            return Vec::new();
        }
    };
    let rel = pants_toml
        .strip_prefix(scan_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "pants.toml".to_string());
    let mut extra_annotations = BTreeMap::new();
    extra_annotations.insert("waybill:source-file".to_string(), json!(rel));
    // Milestone 236 (C151): pants_go expected_version design-tier reason.
    extra_annotations.insert(
        "waybill:unresolved-reason".to_string(),
        json!("pants_go expected_version declared; no matching go corpus component"),
    );
    vec![PackageDbEntry {
        purl,
        name: "go".to_string(),
        version: version.to_string(),
        arch: None,
        source_path: pants_toml.display().to_string(),
        depends: Vec::new(),
        maintainer: None,
        lifecycle_scope: Some(LifecycleScope::Development),
        requirement_ranges: Vec::new(),
        source_type: None,
        licenses: Vec::new(),
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        sbom_tier: Some("design".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    }]
}

/// Public enrichment entry — walks BUILD files, builds the ownership
/// index, and injects `waybill:pants-target` onto every matching
/// `pkg:golang/*` component in `components`. Called from
/// `scan_fs/mod.rs` at line ~1001 (post-m191, pre-m148). Runs on
/// the already-reconciled component set.
///
/// **Zero fabrication** (FR-012 / Principle IX): NEVER pushes new
/// components; only mutates `extra_annotations` in place.
pub fn enrich(
    scan_root: &Path,
    exclude_set: &ExclusionSet,
    components: &mut [ResolvedComponent],
) {
    let build_files = pants_common::discover_build_files(scan_root, exclude_set);
    let pants_toml_present = scan_root.join("pants.toml").is_file();
    if build_files.is_empty() && !pants_toml_present {
        return;
    }

    let build_files_discovered = build_files.len();
    let mut build_files_parsed_ok: usize = 0;
    let mut build_files_skipped_corrupt: usize = 0;
    let mut go_targets_found: usize = 0;

    let mut all_decls: Vec<(PathBuf, GoTargetDeclaration)> = Vec::new();

    for build_file in &build_files {
        let bytes = match std::fs::read(build_file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    build_file = %build_file.display(),
                    error = %e,
                    "pants-go reader: could not read BUILD file; skipping"
                );
                build_files_skipped_corrupt += 1;
                continue;
            }
        };
        let results = build_dsl::extract_targets(&bytes);
        if results.is_empty() {
            // No recognized Go targets — legal (BUILD files carry
            // many other target types). Counts as parsed_ok, not skip.
            build_files_parsed_ok += 1;
            continue;
        }
        let mut any_ok = false;
        let mut any_err = false;
        for r in results {
            match r {
                Ok(decl) => {
                    any_ok = true;
                    go_targets_found += 1;
                    all_decls.push((build_file.clone(), decl));
                }
                Err(e) => {
                    any_err = true;
                    tracing::warn!(
                        build_file = %build_file.display(),
                        error = %e,
                        "pants-go reader: target parse error; skipping this target"
                    );
                }
            }
        }
        if any_ok {
            build_files_parsed_ok += 1;
        } else if any_err {
            build_files_skipped_corrupt += 1;
        }
    }

    let index = ownership_index::build_index(&all_decls, scan_root);

    // Enrichment pass: iterate every pkg:golang/* component and
    // inject the annotation when the ownership index has matches.
    let mut components_annotated: usize = 0;
    let mut matched_import_paths: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for c in components.iter_mut() {
        if c.purl.ecosystem() != "golang" {
            continue;
        }
        let addresses = enrichment::collect_addresses_for_component(c, &index, scan_root);
        if addresses.is_empty() {
            continue;
        }
        // Track which import_paths were matched for the FR-012 orphan diagnostic.
        let import_path = match c.purl.namespace() {
            Some(ns) => format!("{ns}/{}", c.purl.name()),
            None => c.purl.name().to_string(),
        };
        matched_import_paths.insert(import_path);

        let joined = addresses
            .iter()
            .map(|a| a.0.clone())
            .collect::<Vec<_>>()
            .join(",");
        c.extra_annotations
            .insert("waybill:pants-target".to_string(), json!(joined));
        components_annotated += 1;
    }

    // FR-012 orphan diagnostic: named import_paths with no matching go.sum entry.
    for import_path in index.import_path_to_addresses.keys() {
        if !matched_import_paths.contains(import_path) {
            tracing::info!(
                import_path = %import_path,
                "pants-go reader: go_third_party_package named an import_path with no matching pkg:golang/* component; not fabricated"
            );
        }
    }

    // Toolchain-component-emitted signal — read()'s return already
    // pushed one into the components vector, so look for it.
    let toolchain_component_emitted = components
        .iter()
        .any(|c| c.purl.as_str().starts_with("pkg:generic/go@"))
        as usize;

    tracing::info!(
        build_files_discovered,
        build_files_parsed_ok,
        build_files_skipped_corrupt,
        go_targets_found,
        components_annotated,
        toolchain_component_emitted,
        "pants-go enrichment complete"
    );
}
