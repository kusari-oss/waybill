//! Milestone 143 — Haskell ecosystem reader.
//!
//! Discovers Haskell projects via five input artifacts:
//!
//! - `cabal.project.freeze` (cabal-install line-format pinned constraints) —
//!   source-tier per FR-002. Regex-extracted exact-pin entries
//!   (`<name> ==<version>`); flag toggles (`<name> +<flag>`) skipped; range
//!   constraints emit as design-tier with `waybill:requirement-range`.
//!
//! - `stack.yaml.lock` (YAML, Stack 2.1+) — source-tier per FR-003.
//!   Parsed via `serde_yaml` with Q3-style content-shape gate (top-level
//!   `snapshots:` array required). Schema-v1 covers the dominant production
//!   case circa 2026; `extra-deps` Hackage entries emit as
//!   `pkg:hackage/<name>@<version>`; git-source extra-deps warn-and-skip
//!   (deferred to v1.1).
//!
//! - `stack.yaml` (YAML) — resolver identifier source for the Stack
//!   snapshot placeholder per FR-005. Even when `stack.yaml.lock` is
//!   absent, `stack.yaml`'s `resolver:` field drives the snapshot
//!   placeholder emission (with `@unspecified` version when no lockfile
//!   SHA available).
//!
//! - `cabal.project` (cabal-DSL) — multi-package detection signal per
//!   FR-001. Filesystem walk catches all `*.cabal`s regardless of the
//!   `cabal.project`'s `packages:` field content, so the reader does NOT
//!   parse this file's body (presence-only signal per research §R7).
//!
//! - `*.cabal` (per-package Cabal-DSL descriptor) — main-module
//!   emission source per FR-013 + design-tier fallback per FR-007.
//!   Multi-stanza extraction (library / executable / test-suite /
//!   benchmark / foreign-library) drives the Q2 main-module `depends`
//!   union with per-stanza `lifecycle-scope` tagging (runtime vs
//!   development); most-binding scope wins on name collision.
//!
//! Five source discriminators per FR-004 + FR-005:
//!
//! - **`hackage-freeze`** — from `cabal.project.freeze` exact-pin entries.
//!   PURL `pkg:hackage/<lc-name>@<version>`.
//! - **`hackage-stack-lock`** — from `stack.yaml.lock` explicit extra-deps.
//!   PURL `pkg:hackage/<lc-name>@<version>` (Stack publishes via Hackage).
//! - **`hackage-snapshot`** — Stackage placeholder per FR-005. PURL
//!   `pkg:generic/stackage-<resolver>@<sha-or-unspecified>` for
//!   `lts-*`/`nightly-*`; `pkg:generic/<resolver>@<sha-or-unspecified>`
//!   for `ghc-*` resolvers (GHC-only, no Stackage bundle).
//! - **`hackage-cabal-design`** — design-tier fallback from `*.cabal`
//!   `build-depends:` when no lockfile present.
//! - **`hackage-main-module`** — per-package main-module from `*.cabal`'s
//!   `name:` + `version:` keywords.
//!
//! Three Q-clarifications drive the design:
//!
//! - **Q1 (GHC-stdlib annotation, FR-014)**: hardcoded ~22-name boot-library
//!   allowlist (`base`, `text`, `bytestring`, `containers`, etc.) emits
//!   `waybill:ghc-stdlib = "true"` on matching components. Informational
//!   — does NOT gate emission. Mirrors milestone-141 OTP-stdlib pattern.
//!
//! - **Q2 (multi-stanza union)**: main-module `depends` unions ALL stanzas'
//!   `build-depends:` (library + executable + test-suite + benchmark +
//!   build-tool-depends) with per-stanza `waybill:lifecycle-scope`
//!   (runtime vs development); most-binding scope wins on name collision.
//!
//! - **Q3 (Hpack detect-and-warn, FR-015)**: when `package.yaml` is found
//!   alongside a generated-by-Hpack `*.cabal` (header-regex match), emit
//!   `tracing::warn!` recommending regeneration. Reader does NOT parse
//!   `package.yaml` directly — avoids second-source-of-truth complexity.
//!
//! `waybill:source-type` value-set follows the milestone-122/137-142
//! prefixed convention with the `hackage-` prefix.
//!
//! Zero new Cargo dependencies — reuses workspace `regex`, `serde_yaml`,
//! `serde_json`, `tracing`, `anyhow`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::purl::Purl;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;

const MAX_HASKELL_WALK_DEPTH: usize = 12;

/// Hardcoded GHC boot-library allowlist per Q1 + FR-014. Allowlisted
/// entries emit as regular `pkg:hackage/<name>@<version>` components AND
/// additionally carry `waybill:ghc-stdlib = "true"`. The annotation is
/// informational — it does NOT gate emission; non-allowlisted entries
/// emit identically except for the additional property. Mirrors the
/// milestone-141 OTP-stdlib pattern.
const GHC_STDLIB_ALLOWLIST: &[&str] = &[
    "base",
    "ghc-prim",
    "template-haskell",
    "integer-gmp",
    "integer-simple",
    "array",
    "bytestring",
    "containers",
    "deepseq",
    "directory",
    "filepath",
    "ghc",
    "mtl",
    "parsec",
    "pretty",
    "process",
    "stm",
    "text",
    "time",
    "transformers",
    "unix",
    "Win32",
];

fn should_skip_descent(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".svn"
            | ".hg"
            | "dist-newstyle"
            | "dist"
            | ".stack-work"
            | "node_modules"
            | "target"
            | "_build"
            | ".idea"
            | ".vscode"
    )
}

// -----------------------------------------------------------------------
// Types (T003 + data-model §2)
// -----------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum CabalFreezeEntry {
    ExactPin {
        name: String, // lowercased
        version: String,
    },
    RangeConstraint {
        name: String,
        range: String, // raw range string preserved verbatim
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StackLockEntry {
    name: String,
    version: String,
}

#[derive(Debug, Clone)]
struct StackSnapshot {
    resolver: String,
    sha256: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CabalManifest {
    name: Option<String>,
    version: Option<String>,
    stanzas: Vec<CabalStanza>,
    hpack_generated: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CabalStanza {
    kind: StanzaKind,
    label: Option<String>,
    build_depends: Vec<DeclaredDep>,
    build_tool_depends: Vec<DeclaredDep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StanzaKind {
    Library,
    Executable,
    TestSuite,
    Benchmark,
    ForeignLibrary,
}

#[derive(Debug, Clone)]
struct DeclaredDep {
    name: String,          // lowercased per Hackage casing convention
    range: Option<String>, // raw range string when present
}

// -----------------------------------------------------------------------
// Q2 lifecycle-scope helpers (T005 + data-model §2.4)
// -----------------------------------------------------------------------

fn stanza_lifecycle_scope(kind: StanzaKind) -> LifecycleScope {
    match kind {
        StanzaKind::Library | StanzaKind::Executable | StanzaKind::ForeignLibrary => {
            LifecycleScope::Runtime
        }
        StanzaKind::TestSuite | StanzaKind::Benchmark => LifecycleScope::Development,
    }
}

/// Q2 most-binding-wins precedence: Runtime beats Development on name collision.
fn merge_scope(existing: LifecycleScope, new: LifecycleScope) -> LifecycleScope {
    match (existing, new) {
        (LifecycleScope::Runtime, _) | (_, LifecycleScope::Runtime) => LifecycleScope::Runtime,
        _ => LifecycleScope::Development,
    }
}

// -----------------------------------------------------------------------
// serde_yaml types for Stack lockfile (T007 + data-model §1.2 + §2.2)
// -----------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackYamlLock {
    snapshots: Vec<StackSnapshotYaml>,
    packages: Vec<StackPackageYaml>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackSnapshotYaml {
    completed: Option<StackSnapshotCompleted>,
    original: Option<StackSnapshotOriginal>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackSnapshotCompleted {
    sha256: Option<String>,
    size: Option<u64>,
    url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackSnapshotOriginal {
    resolver: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackPackageYaml {
    completed: Option<serde_yaml::Value>, // tolerate schema variations
    original: Option<StackPackageOriginal>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackPackageOriginal {
    hackage: Option<String>,
    // git/commit fields exist for git-source extra-deps but are out of
    // scope per research §R3 (warn-and-skip in v1).
    git: Option<String>,
}

/// `stack.yaml`'s top-level YAML (only `resolver:` field needed for FR-005
/// fallback when no `stack.yaml.lock` is present).
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
#[allow(dead_code)]
struct StackYaml {
    resolver: Option<String>,
}

// -----------------------------------------------------------------------
// Regex helpers — all hoisted to module scope (T006 + research §R9)
// -----------------------------------------------------------------------

fn constraints_keyword_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?ms)^constraints:\s*(.+?)(?:^\w|\z)").expect("static constraints regex")
    })
}

fn exact_pin_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9-]*)\s+==\s*([0-9][0-9\.]*(?:-[A-Za-z0-9]+)?)$")
            .expect("static exact-pin regex")
    })
}

fn flag_toggle_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9-]*)\s+[+-]([A-Za-z][A-Za-z0-9_-]*)$")
            .expect("static flag-toggle regex")
    })
}

fn range_constraint_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9-]*)\s+(.+)$").expect("static range regex")
    })
}

fn cabal_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?mi)^name:\s*(\S+)").expect("static cabal-name regex"))
}

fn cabal_version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?mi)^version:\s*(\S+)").expect("static cabal-version regex"))
}

fn cabal_stanza_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?mi)^(library|executable|test-suite|benchmark|foreign-library)(?:\s+(\S+))?\s*$")
            .expect("static stanza regex")
    })
}

fn cabal_build_depends_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?mis)^\s+build-depends:\s*([^\n][\s\S]*?)(?:\n\s*\n|\n\S|\z)")
            .expect("static build-depends regex")
    })
}

fn cabal_build_tool_depends_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?mis)^\s+build-tool-depends:\s*([^\n][\s\S]*?)(?:\n\s*\n|\n\S|\z)")
            .expect("static build-tool-depends regex")
    })
}

fn hpack_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^-- This file has been generated from package\.yaml by hpack version")
            .expect("static hpack header regex")
    })
}

// -----------------------------------------------------------------------
// Q3 content-shape validation (T008 + research §R3)
// -----------------------------------------------------------------------

fn validate_stack_lock_shape(value: &serde_yaml::Value) -> bool {
    value
        .as_mapping()
        .and_then(|m| m.get("snapshots"))
        .map(|v| v.is_sequence())
        .unwrap_or(false)
}

// -----------------------------------------------------------------------
// pub fn read — entry point
// -----------------------------------------------------------------------

pub fn read(
    rootfs: &Path,
    _include_dev: bool,
    exclude_set: &ExclusionSet,
) -> Vec<PackageDbEntry> {
    let mut out: Vec<PackageDbEntry> = Vec::new();
    let mut seen_purls: HashSet<String> = HashSet::new();

    // Phase A — discover all artifacts.
    let cabal_paths = discover_cabal_files(rootfs, exclude_set);
    let freeze_paths = discover_cabal_freezes(rootfs, exclude_set);
    let cabal_project_paths = discover_cabal_projects(rootfs, exclude_set);
    let stack_lock_paths = discover_stack_locks(rootfs, exclude_set);
    let stack_yaml_paths = discover_stack_yamls(rootfs, exclude_set);
    let package_yaml_paths = discover_package_yamls(rootfs, exclude_set);

    // FR-008 / SC-004: no-op when no Haskell artifacts present.
    if cabal_paths.is_empty()
        && freeze_paths.is_empty()
        && cabal_project_paths.is_empty()
        && stack_lock_paths.is_empty()
        && stack_yaml_paths.is_empty()
    {
        return out;
    }

    // Phase B — parse freeze files. Track parse-success per parent dir
    // so design-tier fallback (Phase G) can distinguish "lockfile exists
    // and parsed cleanly" from "lockfile exists but failed to parse"
    // (the FR-009 fallback case).
    let mut freeze_entries: Vec<CabalFreezeEntry> = Vec::new();
    let mut successful_freeze_dirs: HashSet<PathBuf> = HashSet::new();
    for path in &freeze_paths {
        match parse_cabal_freeze(path) {
            Ok(entries) => {
                freeze_entries.extend(entries);
                if let Some(dir) = path.parent() {
                    successful_freeze_dirs.insert(dir.to_path_buf());
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "haskell: failed to parse cabal.project.freeze; falling back to design-tier from sibling *.cabal (FR-009)",
                );
            }
        }
    }

    // Phase C — parse Stack lockfiles + extract snapshots from stack.yaml when lockfile absent.
    let mut stack_lock_entries: Vec<StackLockEntry> = Vec::new();
    let mut stack_snapshots: Vec<StackSnapshot> = Vec::new();
    let mut stack_lock_dirs: HashSet<PathBuf> = HashSet::new();
    for path in &stack_lock_paths {
        match parse_stack_lock(path) {
            Ok((entries, snapshots)) => {
                stack_lock_entries.extend(entries);
                stack_snapshots.extend(snapshots);
                if let Some(dir) = path.parent() {
                    stack_lock_dirs.insert(dir.to_path_buf());
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "haskell: failed to parse stack.yaml.lock; falling back per FR-009",
                );
            }
        }
    }
    // F3 remediation: stack.yaml-only fallback when no sibling stack.yaml.lock.
    for stack_yaml_path in &stack_yaml_paths {
        let Some(dir) = stack_yaml_path.parent() else {
            continue;
        };
        if stack_lock_dirs.contains(dir) {
            continue;
        }
        if let Ok(Some(snapshot)) = extract_resolver_from_stack_yaml(stack_yaml_path) {
            stack_snapshots.push(snapshot);
        }
    }

    // Phase D — parse *.cabal manifests.
    let mut cabal_manifests: Vec<(PathBuf, CabalManifest)> = Vec::new();
    // De-dup by directory: alphabetically-first *.cabal wins per Edge Case + FR-013.
    let mut seen_cabal_dirs: HashSet<PathBuf> = HashSet::new();
    for path in &cabal_paths {
        let Some(dir) = path.parent() else { continue };
        if !seen_cabal_dirs.insert(dir.to_path_buf()) {
            tracing::warn!(
                path = %path.display(),
                "haskell: multiple *.cabal files in same directory; alphabetically-first wins, this one skipped (FR-013 Edge Case)",
            );
            continue;
        }
        match parse_cabal_manifest(path) {
            Ok(manifest) => cabal_manifests.push((path.clone(), manifest)),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "haskell: failed to parse *.cabal; skipping (FR-009)",
                );
            }
        }
    }

    // Phase E — Q3 Hpack detect-and-warn (FR-015).
    emit_hpack_warnings(&cabal_manifests, &package_yaml_paths);

    // Phase F — emit lockfile-derived components.
    for entry in &freeze_entries {
        let component = build_freeze_component(entry);
        let purl_key = component.purl.as_str().to_string();
        if seen_purls.insert(purl_key) {
            out.push(component);
        }
    }
    for entry in &stack_lock_entries {
        let component = build_stack_lock_component(entry);
        let purl_key = component.purl.as_str().to_string();
        if seen_purls.insert(purl_key) {
            out.push(component);
        }
    }
    for snapshot in &stack_snapshots {
        let component = build_snapshot_placeholder(snapshot);
        let purl_key = component.purl.as_str().to_string();
        if seen_purls.insert(purl_key) {
            out.push(component);
        }
    }

    // Phase G — per-*.cabal emit: main-module + design-tier deps when no lockfile.
    // Per FR-009 + sc005 remediation: "successful lockfile parse" determines
    // the fallback path, not mere file presence — malformed freeze files
    // should still trigger design-tier emission.
    let any_successful_lockfile =
        !successful_freeze_dirs.is_empty() || !stack_lock_dirs.is_empty();
    for (cabal_path, manifest) in &cabal_manifests {
        let cabal_dir = cabal_path.parent().map(|d| {
            std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf())
        });
        let has_local_successful_lockfile = cabal_dir
            .as_ref()
            .map(|d| {
                successful_freeze_dirs.iter().any(|fd| {
                    std::fs::canonicalize(fd).unwrap_or_else(|_| fd.clone()) == *d
                }) || stack_lock_dirs.iter().any(|fd| {
                    std::fs::canonicalize(fd).unwrap_or_else(|_| fd.clone()) == *d
                })
            })
            .unwrap_or(false);
        let union_dep_names = collect_design_tier_deps(manifest)
            .into_iter()
            .map(|(dep, _scope)| dep.name.clone())
            .collect::<Vec<_>>();
        if let Some(main) = build_main_module(
            manifest,
            cabal_path,
            has_local_successful_lockfile || any_successful_lockfile,
            &union_dep_names,
        ) {
            let purl_key = main.purl.as_str().to_string();
            if seen_purls.insert(purl_key) {
                out.push(main);
            }
        }
        if !has_local_successful_lockfile && !any_successful_lockfile {
            for component in build_design_tier_components(manifest, cabal_path) {
                let purl_key = component.purl.as_str().to_string();
                if seen_purls.insert(purl_key) {
                    out.push(component);
                }
            }
        }
    }

    out
}

// -----------------------------------------------------------------------
// Discovery helpers (T010 + T016 + research §R11)
// -----------------------------------------------------------------------

fn discover_cabal_files(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_HASKELL_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                == Some("cabal")
        {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

fn discover_cabal_freezes(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    discover_by_filename(rootfs, exclude_set, "cabal.project.freeze")
}

fn discover_cabal_projects(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    discover_by_filename(rootfs, exclude_set, "cabal.project")
}

fn discover_stack_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    discover_by_filename(rootfs, exclude_set, "stack.yaml.lock")
}

fn discover_stack_yamls(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    discover_by_filename(rootfs, exclude_set, "stack.yaml")
}

fn discover_package_yamls(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    discover_by_filename(rootfs, exclude_set, "package.yaml")
}

fn discover_by_filename(
    rootfs: &Path,
    exclude_set: &ExclusionSet,
    filename: &str,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = crate::scan_fs::walk::WalkConfig {
        max_depth: MAX_HASKELL_WALK_DEPTH,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
                return true;
            };
            should_skip_descent(name)
        },
        exclude_set,
    };
    crate::scan_fs::walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some(filename) {
            out.push(path.to_path_buf());
        }
    });
    out.sort();
    out
}

// -----------------------------------------------------------------------
// cabal.project.freeze parsing (T011 + research §R2)
// -----------------------------------------------------------------------

fn parse_cabal_freeze(path: &Path) -> anyhow::Result<Vec<CabalFreezeEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let constraints_body = match constraints_keyword_re().captures(&text) {
        Some(caps) => caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default(),
        None => {
            anyhow::bail!("no `constraints:` keyword found in cabal.project.freeze");
        }
    };

    // Multi-line continuation: collapse whitespace/newlines into single spaces.
    let flattened: String = constraints_body
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");

    let mut out: Vec<CabalFreezeEntry> = Vec::new();
    for entry_str in flattened.split(',') {
        let trimmed = entry_str.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Try exact-pin first (most common in freeze files).
        if let Some(caps) = exact_pin_re().captures(trimmed) {
            let name = caps.get(1).map(|m| m.as_str().to_lowercase());
            let version = caps.get(2).map(|m| m.as_str().to_string());
            if let (Some(name), Some(version)) = (name, version) {
                out.push(CabalFreezeEntry::ExactPin { name, version });
                continue;
            }
        }
        // Then flag toggle — recognize + skip per Edge Case.
        if flag_toggle_re().is_match(trimmed) {
            tracing::debug!(entry = %trimmed, "haskell: skipping flag-toggle constraint (FR-002 + Edge Case)");
            continue;
        }
        // Fall through to range constraint (catch-all).
        if let Some(caps) = range_constraint_re().captures(trimmed) {
            let name = caps.get(1).map(|m| m.as_str().to_lowercase());
            let range = caps.get(2).map(|m| m.as_str().to_string());
            if let (Some(name), Some(range)) = (name, range) {
                out.push(CabalFreezeEntry::RangeConstraint { name, range });
                continue;
            }
        }
        tracing::debug!(entry = %trimmed, "haskell: skipping unrecognized freeze entry");
    }
    Ok(out)
}

// -----------------------------------------------------------------------
// stack.yaml.lock parsing (T017 + research §R3 + F3 split)
// -----------------------------------------------------------------------

fn parse_stack_lock(
    path: &Path,
) -> anyhow::Result<(Vec<StackLockEntry>, Vec<StackSnapshot>)> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let raw_value: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("YAML parse failed: {e}"))?;
    if !validate_stack_lock_shape(&raw_value) {
        anyhow::bail!(
            "Q3 content-shape validation failed — missing top-level snapshots array"
        );
    }
    let lock: StackYamlLock = serde_yaml::from_value(raw_value)
        .map_err(|e| anyhow::anyhow!("typed deserialize failed: {e}"))?;

    let mut snapshots: Vec<StackSnapshot> = Vec::new();
    for snap_yaml in &lock.snapshots {
        let resolver = snap_yaml
            .original
            .as_ref()
            .and_then(|o| o.resolver.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let sha256 = snap_yaml.completed.as_ref().and_then(|c| c.sha256.clone());
        snapshots.push(StackSnapshot { resolver, sha256 });
    }

    let mut entries: Vec<StackLockEntry> = Vec::new();
    for pkg_yaml in &lock.packages {
        let Some(orig) = &pkg_yaml.original else {
            continue;
        };
        if orig.git.is_some() {
            tracing::warn!(
                "haskell: skipping git-source stack extra-dep (out of scope for v1; deferred to v1.1)",
            );
            continue;
        }
        let Some(hackage_coord) = &orig.hackage else {
            continue;
        };
        // Format: "name-version" — split on LAST dash.
        let Some((name, version)) = split_hackage_coord(hackage_coord) else {
            tracing::warn!(coord = %hackage_coord, "haskell: failed to parse hackage coord");
            continue;
        };
        entries.push(StackLockEntry { name, version });
    }
    Ok((entries, snapshots))
}

/// Split a `<name>-<version>` coord on the LAST dash (since names like
/// `aeson-pretty-0.8.10` would mis-split on the first dash).
fn split_hackage_coord(coord: &str) -> Option<(String, String)> {
    // Stack's `original.hackage` may include a `@sha256:...` suffix; strip it.
    let coord = coord.split('@').next().unwrap_or(coord);
    let dash_idx = coord.rfind('-')?;
    let name = coord[..dash_idx].to_lowercase();
    let version = coord[dash_idx + 1..].to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

/// F3 remediation: stack.yaml-only fallback when no sibling lockfile.
fn extract_resolver_from_stack_yaml(path: &Path) -> anyhow::Result<Option<StackSnapshot>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;
    let parsed: StackYaml = serde_yaml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("YAML parse failed: {e}"))?;
    Ok(parsed.resolver.map(|r| StackSnapshot {
        resolver: r,
        sha256: None,
    }))
}

// -----------------------------------------------------------------------
// *.cabal parsing (T013 + T022 + research §R4)
// -----------------------------------------------------------------------

fn parse_cabal_manifest(path: &Path) -> anyhow::Result<CabalManifest> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read failed: {e}"))?;

    let name = cabal_name_re()
        .captures(&text)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_lowercase()));
    let version = cabal_version_re()
        .captures(&text)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
    let hpack_generated = hpack_header_re().is_match(&text);
    let stanzas = extract_stanzas(&text);

    Ok(CabalManifest {
        name,
        version,
        stanzas,
        hpack_generated,
    })
}

/// Extract per-stanza `build-depends:` + `build-tool-depends:` blocks
/// from a *.cabal body. Per research §R4.
fn extract_stanzas(text: &str) -> Vec<CabalStanza> {
    let mut out: Vec<CabalStanza> = Vec::new();
    // Collect all stanza openers + their character offsets.
    let opener_matches: Vec<_> = cabal_stanza_re()
        .captures_iter(text)
        .filter_map(|caps| {
            let m = caps.get(0)?;
            let kind = caps.get(1).and_then(|k| {
                Some(match k.as_str().to_lowercase().as_str() {
                    "library" => StanzaKind::Library,
                    "executable" => StanzaKind::Executable,
                    "test-suite" => StanzaKind::TestSuite,
                    "benchmark" => StanzaKind::Benchmark,
                    "foreign-library" => StanzaKind::ForeignLibrary,
                    _ => return None,
                })
            })?;
            let label = caps.get(2).map(|l| l.as_str().to_string());
            Some((m.start(), m.end(), kind, label))
        })
        .collect();

    for (i, (_open_start, open_end, kind, label)) in opener_matches.iter().enumerate() {
        let block_start = *open_end;
        let block_end = opener_matches
            .get(i + 1)
            .map(|(next_start, _, _, _)| *next_start)
            .unwrap_or(text.len());
        let block = &text[block_start..block_end];

        let build_depends = extract_build_depends_block(block, cabal_build_depends_re());
        let build_tool_depends = extract_build_depends_block(block, cabal_build_tool_depends_re());

        out.push(CabalStanza {
            kind: *kind,
            label: label.clone(),
            build_depends,
            build_tool_depends,
        });
    }
    out
}

fn extract_build_depends_block(block: &str, re: &Regex) -> Vec<DeclaredDep> {
    let Some(caps) = re.captures(block) else {
        return Vec::new();
    };
    let Some(body) = caps.get(1) else {
        return Vec::new();
    };
    parse_dep_list(body.as_str())
}

/// Parse a comma-separated dep list (potentially multi-line) into
/// `Vec<DeclaredDep>`. Each entry is `<name> [<range>]`; we split on
/// first whitespace to separate name from range.
fn parse_dep_list(body: &str) -> Vec<DeclaredDep> {
    // Flatten multi-line continuations: collapse whitespace + newlines.
    let flattened: String = body
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out: Vec<DeclaredDep> = Vec::new();
    for entry_str in flattened.split(',') {
        let trimmed = entry_str.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed.split_once(char::is_whitespace) {
            Some((name, rest)) => out.push(DeclaredDep {
                name: name.to_lowercase(),
                range: Some(rest.trim().to_string()),
            }),
            None => out.push(DeclaredDep {
                name: trimmed.to_lowercase(),
                range: None,
            }),
        }
    }
    out
}

// -----------------------------------------------------------------------
// Q2 union of multi-stanza build-depends (T023 + Q2 most-binding-wins)
// -----------------------------------------------------------------------

fn collect_design_tier_deps(manifest: &CabalManifest) -> Vec<(DeclaredDep, LifecycleScope)> {
    let mut by_name: HashMap<String, (DeclaredDep, LifecycleScope)> = HashMap::new();
    for stanza in &manifest.stanzas {
        let scope = stanza_lifecycle_scope(stanza.kind);
        for dep in &stanza.build_depends {
            by_name
                .entry(dep.name.clone())
                .and_modify(|(_, s)| *s = merge_scope(*s, scope))
                .or_insert_with(|| (dep.clone(), scope));
        }
        for dep in &stanza.build_tool_depends {
            // Build-tool-depends ALWAYS Development per FR-010, regardless of containing stanza.
            by_name
                .entry(dep.name.clone())
                .and_modify(|(_, s)| *s = merge_scope(*s, LifecycleScope::Development))
                .or_insert_with(|| (dep.clone(), LifecycleScope::Development));
        }
    }
    by_name.into_values().collect()
}

// -----------------------------------------------------------------------
// Component builders (T012 + T018 + T019 + T024)
// -----------------------------------------------------------------------

fn build_freeze_component(entry: &CabalFreezeEntry) -> PackageDbEntry {
    match entry {
        CabalFreezeEntry::ExactPin { name, version } => {
            let purl_str = format!("pkg:hackage/{name}@{version}");
            let purl = Purl::new(&purl_str)
                .unwrap_or_else(|_| fallback_purl());
            let mut extra_annotations = base_annotations("hackage-freeze");
            apply_ghc_stdlib_annotation(&mut extra_annotations, name);
            PackageDbEntry {
                purl,
                name: name.clone(),
                version: version.clone(),
                arch: None,
                source_path: String::new(),
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: Some(LifecycleScope::Runtime),
                requirement_ranges: Vec::new(),
                source_type: Some("hackage-freeze".to_string()),
                buildinfo_status: None,
                sbom_tier: Some("source".to_string()),
                evidence_kind: Some("cabal-freeze".to_string()),
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
                shade_relocation: None,
                extra_annotations,
                binary_role: None,
                build_inclusion: None,
            }
        }
        CabalFreezeEntry::RangeConstraint { name, range } => {
            let sanitized = sanitize_purl_version(range);
            let purl_str = format!("pkg:hackage/{name}@{sanitized}");
            let purl = Purl::new(&purl_str).unwrap_or_else(|_| fallback_purl());
            let mut extra_annotations = base_annotations("hackage-freeze");
            apply_ghc_stdlib_annotation(&mut extra_annotations, name);
            PackageDbEntry {
                purl,
                name: name.clone(),
                version: sanitized,
                arch: None,
                source_path: String::new(),
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: Some(LifecycleScope::Runtime),
                requirement_ranges: vec![range.clone()],
                source_type: Some("hackage-freeze".to_string()),
                buildinfo_status: None,
                sbom_tier: Some("design".to_string()),
                evidence_kind: Some("cabal-freeze".to_string()),
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
                shade_relocation: None,
                extra_annotations,
                binary_role: None,
                build_inclusion: None,
            }
        }
    }
}

fn build_stack_lock_component(entry: &StackLockEntry) -> PackageDbEntry {
    let purl_str = format!("pkg:hackage/{}@{}", entry.name, entry.version);
    let purl = Purl::new(&purl_str).unwrap_or_else(|_| fallback_purl());
    let mut extra_annotations = base_annotations("hackage-stack-lock");
    apply_ghc_stdlib_annotation(&mut extra_annotations, &entry.name);
    PackageDbEntry {
        purl,
        name: entry.name.clone(),
        version: entry.version.clone(),
        arch: None,
        source_path: String::new(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(LifecycleScope::Runtime),
        requirement_ranges: Vec::new(),
        source_type: Some("hackage-stack-lock".to_string()),
        buildinfo_status: None,
        sbom_tier: Some("source".to_string()),
        evidence_kind: Some("stack-yaml-lock".to_string()),
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
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    }
}

fn build_snapshot_placeholder(snapshot: &StackSnapshot) -> PackageDbEntry {
    let version_slot = snapshot
        .sha256
        .clone()
        .unwrap_or_else(|| "unspecified".to_string());
    let purl_str = if snapshot.resolver.starts_with("lts-")
        || snapshot.resolver.starts_with("nightly-")
    {
        format!(
            "pkg:generic/stackage-{}@{}",
            snapshot.resolver, version_slot
        )
    } else if snapshot.resolver.starts_with("ghc-") {
        format!("pkg:generic/{}@{}", snapshot.resolver, version_slot)
    } else {
        // Defensive fallback — unknown resolver shape.
        format!("pkg:generic/{}@{}", snapshot.resolver, version_slot)
    };
    let purl = Purl::new(&purl_str).unwrap_or_else(|_| fallback_purl());
    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "waybill:source-type".to_string(),
        serde_json::Value::String("hackage-snapshot".to_string()),
    );
    extra_annotations.insert(
        "waybill:stackage-resolver".to_string(),
        serde_json::Value::String(snapshot.resolver.clone()),
    );
    let sbom_tier = if snapshot.sha256.is_some() {
        "source"
    } else {
        "design"
    };
    PackageDbEntry {
        purl,
        name: snapshot.resolver.clone(),
        version: version_slot,
        arch: None,
        source_path: String::new(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: Some(LifecycleScope::Runtime),
        requirement_ranges: Vec::new(),
        source_type: Some("hackage-snapshot".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("stack-yaml-lock".to_string()),
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
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    }
}

fn build_main_module(
    manifest: &CabalManifest,
    cabal_path: &Path,
    has_lockfile: bool,
    union_dep_names: &[String],
) -> Option<PackageDbEntry> {
    let name = manifest
        .name
        .clone()
        .or_else(|| {
            cabal_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
        })
        .unwrap_or_else(|| "unknown".to_string());
    if name.is_empty() {
        return None;
    }
    // Milestone 197 US3 (#567): emit versionless canonical PURL per
    // purl-spec when the .cabal / stack.yaml manifest has no `version:` —
    // matches m191 fix pattern.
    let raw_version = manifest.version.clone();
    let version = raw_version
        .clone()
        .unwrap_or_else(|| "0.0.0-unknown".to_string());

    let purl_str = if raw_version.as_deref().unwrap_or("").is_empty() {
        format!("pkg:hackage/{name}")
    } else {
        format!("pkg:hackage/{name}@{version}")
    };
    let purl = Purl::new(&purl_str).ok()?;

    // Filter union_dep_names to exclude self-ref (the main-module's own name).
    let depends: Vec<String> = union_dep_names
        .iter()
        .filter(|n| *n != &name)
        .cloned()
        .collect();

    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert(
        "waybill:component-role".to_string(),
        serde_json::Value::String("main-module".to_string()),
    );
    extra_annotations.insert(
        "waybill:source-type".to_string(),
        serde_json::Value::String("hackage-main-module".to_string()),
    );
    let sbom_tier = if has_lockfile { "source" } else { "design" };

    Some(PackageDbEntry {
        purl,
        name,
        version,
        arch: None,
        source_path: cabal_path.to_string_lossy().into_owned(),
        depends,
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: Some("hackage-main-module".to_string()),
        buildinfo_status: None,
        sbom_tier: Some(sbom_tier.to_string()),
        evidence_kind: Some("cabal-pkg-descriptor".to_string()),
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
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
        build_inclusion: None,
    })
}

fn build_design_tier_components(
    manifest: &CabalManifest,
    cabal_path: &Path,
) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    // Filter out self-ref: the main-module's own package name doesn't emit
    // as a separate dep component (would collide via PURL dedup anyway).
    let main_name = manifest.name.clone().unwrap_or_default();
    for (dep, scope) in collect_design_tier_deps(manifest) {
        if dep.name == main_name {
            continue;
        }
        let range = dep.range.clone().unwrap_or_else(|| "unspecified".to_string());
        let sanitized = sanitize_purl_version(&range);
        let purl_str = format!("pkg:hackage/{}@{}", dep.name, sanitized);
        let purl = match Purl::new(&purl_str) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let mut extra_annotations = base_annotations("hackage-cabal-design");
        // Milestone 199: always-array shape. Haskell design-tier writes
        // plural (1-element for its own single-dep case); reconciler may
        // later accumulate onto a survivor if a source-tier match exists.
        extra_annotations.insert(
            "waybill:requirement-ranges".to_string(),
            serde_json::json!([dep.range.clone().unwrap_or_default()]),
        );
        apply_ghc_stdlib_annotation(&mut extra_annotations, &dep.name);
        out.push(PackageDbEntry {
            purl,
            name: dep.name.clone(),
            version: sanitized,
            arch: None,
            source_path: cabal_path.to_string_lossy().into_owned(),
            depends: Vec::new(),
            maintainer: None,
            licenses: Vec::new(),
            lifecycle_scope: Some(scope),
            requirement_ranges: dep.range.clone().into_iter().collect(),
            source_type: Some("hackage-cabal-design".to_string()),
            buildinfo_status: None,
            sbom_tier: Some("design".to_string()),
            evidence_kind: Some("cabal-pkg-descriptor".to_string()),
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
            shade_relocation: None,
            extra_annotations,
            binary_role: None,
            build_inclusion: None,
        });
    }
    out
}

// -----------------------------------------------------------------------
// Q3 Hpack detect-and-warn (T026 + FR-015)
// -----------------------------------------------------------------------

fn emit_hpack_warnings(
    cabal_manifests: &[(PathBuf, CabalManifest)],
    package_yaml_paths: &[PathBuf],
) {
    let package_yaml_dirs: HashSet<PathBuf> = package_yaml_paths
        .iter()
        .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
        .collect();
    for (cabal_path, manifest) in cabal_manifests {
        if !manifest.hpack_generated {
            continue;
        }
        let Some(cabal_dir) = cabal_path.parent() else {
            continue;
        };
        if !package_yaml_dirs.contains(cabal_dir) {
            continue;
        }
        let package_yaml = cabal_dir.join("package.yaml");
        tracing::warn!(
            cabal_path = %cabal_path.display(),
            package_yaml = %package_yaml.display(),
            "haskell: Hpack-generated *.cabal detected alongside package.yaml — run 'hpack' to regenerate before scanning if package.yaml has been edited",
        );
    }
}

// -----------------------------------------------------------------------
// Helpers — annotations + utilities
// -----------------------------------------------------------------------

fn base_annotations(source_type: &str) -> BTreeMap<String, serde_json::Value> {
    let mut m: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    m.insert(
        "waybill:source-type".to_string(),
        serde_json::Value::String(source_type.to_string()),
    );
    m
}

fn apply_ghc_stdlib_annotation(
    annotations: &mut BTreeMap<String, serde_json::Value>,
    name: &str,
) {
    if GHC_STDLIB_ALLOWLIST
        .iter()
        .any(|s| s.eq_ignore_ascii_case(name))
    {
        annotations.insert(
            "waybill:ghc-stdlib".to_string(),
            serde_json::Value::String("true".to_string()),
        );
    }
}

fn sanitize_purl_version(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '?' | '#' | ' ' => '_',
            other => other,
        })
        .collect()
}

fn fallback_purl() -> Purl {
    Purl::new("pkg:hackage/unknown@unknown").expect("fallback PURL")
}

// -----------------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn merge_scope_runtime_wins() {
        assert_eq!(
            merge_scope(LifecycleScope::Runtime, LifecycleScope::Development),
            LifecycleScope::Runtime
        );
        assert_eq!(
            merge_scope(LifecycleScope::Development, LifecycleScope::Runtime),
            LifecycleScope::Runtime
        );
    }

    #[test]
    fn merge_scope_development_idempotent() {
        assert_eq!(
            merge_scope(LifecycleScope::Development, LifecycleScope::Development),
            LifecycleScope::Development
        );
    }

    #[test]
    fn stanza_scope_mappings() {
        assert_eq!(stanza_lifecycle_scope(StanzaKind::Library), LifecycleScope::Runtime);
        assert_eq!(stanza_lifecycle_scope(StanzaKind::Executable), LifecycleScope::Runtime);
        assert_eq!(stanza_lifecycle_scope(StanzaKind::ForeignLibrary), LifecycleScope::Runtime);
        assert_eq!(stanza_lifecycle_scope(StanzaKind::TestSuite), LifecycleScope::Development);
        assert_eq!(stanza_lifecycle_scope(StanzaKind::Benchmark), LifecycleScope::Development);
    }

    #[test]
    fn parse_cabal_freeze_exact_pins() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cabal.project.freeze");
        std::fs::write(
            &p,
            "constraints: aeson ==2.2.0.0,\n             text ==2.0.2,\n             base ==4.18.0.0\n",
        )
        .unwrap();
        let entries = parse_cabal_freeze(&p).unwrap();
        assert_eq!(entries.len(), 3);
        match &entries[0] {
            CabalFreezeEntry::ExactPin { name, version } => {
                assert_eq!(name, "aeson");
                assert_eq!(version, "2.2.0.0");
            }
            _ => panic!("expected ExactPin"),
        }
    }

    #[test]
    fn parse_cabal_freeze_skips_flag_toggles() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cabal.project.freeze");
        std::fs::write(
            &p,
            "constraints: foo +bar, baz ==1.0.0",
        )
        .unwrap();
        let entries = parse_cabal_freeze(&p).unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0] {
            CabalFreezeEntry::ExactPin { name, .. } => assert_eq!(name, "baz"),
            _ => panic!("expected ExactPin"),
        }
    }

    #[test]
    fn parse_cabal_freeze_range_constraint_emits_design_tier_variant() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cabal.project.freeze");
        std::fs::write(&p, "constraints: text >=2.0 && <2.1").unwrap();
        let entries = parse_cabal_freeze(&p).unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0] {
            CabalFreezeEntry::RangeConstraint { name, range } => {
                assert_eq!(name, "text");
                assert_eq!(range, ">=2.0 && <2.1");
            }
            _ => panic!("expected RangeConstraint"),
        }
    }

    #[test]
    fn parse_stack_lock_schema_v1_extracts_snapshot_and_extras() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stack.yaml.lock");
        std::fs::write(
            &p,
            r#"# Lock file, version 1
snapshots:
  - completed:
      sha256: abc123
      size: 100
      url: https://example.com/lts.yaml
    original:
      resolver: lts-22.0
packages:
  - completed:
      hackage: aeson-2.2.0.0@sha256:def,200
    original:
      hackage: aeson-2.2.0.0
  - completed:
      hackage: lens-5.2.3@sha256:ghi,300
    original:
      hackage: lens-5.2.3
"#,
        )
        .unwrap();
        let (entries, snapshots) = parse_stack_lock(&p).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].resolver, "lts-22.0");
        assert_eq!(snapshots[0].sha256.as_deref(), Some("abc123"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "aeson");
        assert_eq!(entries[0].version, "2.2.0.0");
        assert_eq!(entries[1].name, "lens");
        assert_eq!(entries[1].version, "5.2.3");
    }

    #[test]
    fn validate_stack_lock_shape_rejects_missing_snapshots() {
        let v: serde_yaml::Value = serde_yaml::from_str("packages: []").unwrap();
        assert!(!validate_stack_lock_shape(&v));
    }

    #[test]
    fn validate_stack_lock_shape_accepts_valid() {
        let v: serde_yaml::Value = serde_yaml::from_str("snapshots: []\npackages: []").unwrap();
        assert!(validate_stack_lock_shape(&v));
    }

    #[test]
    fn parse_stack_lock_rejects_invalid_shape() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stack.yaml.lock");
        std::fs::write(&p, "unrelated: data\nno_snapshots: true\n").unwrap();
        let result = parse_stack_lock(&p);
        assert!(result.is_err());
    }

    #[test]
    fn split_hackage_coord_handles_multi_dash_names() {
        let (name, version) = split_hackage_coord("aeson-pretty-0.8.10").unwrap();
        assert_eq!(name, "aeson-pretty");
        assert_eq!(version, "0.8.10");
    }

    #[test]
    fn split_hackage_coord_strips_sha_suffix() {
        let (name, version) =
            split_hackage_coord("aeson-2.2.0.0@sha256:abc,200").unwrap();
        assert_eq!(name, "aeson");
        assert_eq!(version, "2.2.0.0");
    }

    #[test]
    fn extract_resolver_from_stack_yaml_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stack.yaml");
        std::fs::write(&p, "resolver: lts-22.0\npackages:\n- .\n").unwrap();
        let snap = extract_resolver_from_stack_yaml(&p).unwrap().unwrap();
        assert_eq!(snap.resolver, "lts-22.0");
        assert!(snap.sha256.is_none());
    }

    #[test]
    fn parse_cabal_manifest_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("my-app.cabal");
        std::fs::write(
            &p,
            r#"name: my-app
version: 1.2.3
license: BSD-3-Clause

library
  build-depends: base, text, aeson
"#,
        )
        .unwrap();
        let m = parse_cabal_manifest(&p).unwrap();
        assert_eq!(m.name.as_deref(), Some("my-app"));
        assert_eq!(m.version.as_deref(), Some("1.2.3"));
        assert!(!m.hpack_generated);
        assert_eq!(m.stanzas.len(), 1);
        assert_eq!(m.stanzas[0].kind, StanzaKind::Library);
        assert_eq!(m.stanzas[0].build_depends.len(), 3);
    }

    #[test]
    fn parse_cabal_manifest_multi_stanza() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("my-app.cabal");
        std::fs::write(
            &p,
            r#"name: my-app
version: 0.1.0

library
  build-depends: base, text

executable cli
  build-depends: base, optparse-applicative

test-suite spec
  build-depends: base, hspec

benchmark perf
  build-depends: base, criterion
"#,
        )
        .unwrap();
        let m = parse_cabal_manifest(&p).unwrap();
        assert_eq!(m.stanzas.len(), 4);
        assert_eq!(m.stanzas[0].kind, StanzaKind::Library);
        assert_eq!(m.stanzas[1].kind, StanzaKind::Executable);
        assert_eq!(m.stanzas[2].kind, StanzaKind::TestSuite);
        assert_eq!(m.stanzas[3].kind, StanzaKind::Benchmark);
    }

    #[test]
    fn parse_cabal_manifest_detects_hpack_header() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("my-app.cabal");
        std::fs::write(
            &p,
            r#"-- This file has been generated from package.yaml by hpack version 0.36.0.
name: my-app
version: 0.1.0
"#,
        )
        .unwrap();
        let m = parse_cabal_manifest(&p).unwrap();
        assert!(m.hpack_generated);
    }

    #[test]
    fn collect_design_tier_deps_q2_union_with_scope_merge() {
        let manifest = CabalManifest {
            name: Some("my-app".to_string()),
            version: Some("0.1.0".to_string()),
            stanzas: vec![
                CabalStanza {
                    kind: StanzaKind::Library,
                    label: None,
                    build_depends: vec![
                        DeclaredDep { name: "base".to_string(), range: None },
                        DeclaredDep { name: "text".to_string(), range: None },
                    ],
                    build_tool_depends: vec![],
                },
                CabalStanza {
                    kind: StanzaKind::TestSuite,
                    label: Some("spec".to_string()),
                    build_depends: vec![
                        DeclaredDep { name: "base".to_string(), range: None },
                        DeclaredDep { name: "hspec".to_string(), range: None },
                    ],
                    build_tool_depends: vec![],
                },
            ],
            hpack_generated: false,
        };
        let unioned: HashMap<String, LifecycleScope> = collect_design_tier_deps(&manifest)
            .into_iter()
            .map(|(d, s)| (d.name, s))
            .collect();
        // Q2 most-binding-wins: `base` appears in library + test, resolves to Runtime
        assert_eq!(unioned.get("base"), Some(&LifecycleScope::Runtime));
        // text only in library → Runtime
        assert_eq!(unioned.get("text"), Some(&LifecycleScope::Runtime));
        // hspec only in test-suite → Development
        assert_eq!(unioned.get("hspec"), Some(&LifecycleScope::Development));
    }

    #[test]
    fn build_freeze_component_purl_shape() {
        let entry = CabalFreezeEntry::ExactPin {
            name: "aeson".to_string(),
            version: "2.2.0.0".to_string(),
        };
        let c = build_freeze_component(&entry);
        assert_eq!(c.purl.as_str(), "pkg:hackage/aeson@2.2.0.0");
    }

    #[test]
    fn build_freeze_component_ghc_stdlib_annotation_applied() {
        let entry = CabalFreezeEntry::ExactPin {
            name: "base".to_string(),
            version: "4.18.0.0".to_string(),
        };
        let c = build_freeze_component(&entry);
        assert_eq!(
            c.extra_annotations.get("waybill:ghc-stdlib"),
            Some(&serde_json::Value::String("true".to_string()))
        );
    }

    #[test]
    fn build_freeze_component_no_ghc_stdlib_on_non_boot_lib() {
        let entry = CabalFreezeEntry::ExactPin {
            name: "aeson".to_string(),
            version: "2.2.0.0".to_string(),
        };
        let c = build_freeze_component(&entry);
        assert!(!c.extra_annotations.contains_key("waybill:ghc-stdlib"));
    }

    #[test]
    fn build_snapshot_placeholder_lts_purl_shape() {
        let snap = StackSnapshot {
            resolver: "lts-22.0".to_string(),
            sha256: Some("abc123".to_string()),
        };
        let c = build_snapshot_placeholder(&snap);
        assert_eq!(c.purl.as_str(), "pkg:generic/stackage-lts-22.0@abc123");
    }

    #[test]
    fn build_snapshot_placeholder_nightly_purl_shape() {
        let snap = StackSnapshot {
            resolver: "nightly-2024-01-15".to_string(),
            sha256: Some("xyz789".to_string()),
        };
        let c = build_snapshot_placeholder(&snap);
        assert_eq!(c.purl.as_str(), "pkg:generic/stackage-nightly-2024-01-15@xyz789");
    }

    #[test]
    fn build_snapshot_placeholder_ghc_no_stackage_prefix() {
        let snap = StackSnapshot {
            resolver: "ghc-9.6.4".to_string(),
            sha256: None,
        };
        let c = build_snapshot_placeholder(&snap);
        assert_eq!(c.purl.as_str(), "pkg:generic/ghc-9.6.4@unspecified");
    }
}
