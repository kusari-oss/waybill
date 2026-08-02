//! Milestone 225: Pants shell reader — BUILD-file walker + tool-pin discovery.
//!
//! Discovers `BUILD` files under the scan root (via `safe_walk`),
//! extracts `shell_source` / `shell_sources` / `shunit2_test` /
//! `shunit2_tests` target declarations via a regex-scoped Pants-DSL
//! parser (Constitution Principle I — no embedded Python interpreter),
//! resolves each target's `source=` / `sources=[glob...]` expression
//! against the BUILD file's own directory, and emits one
//! `pkg:generic/*` file-tier component per resolved `.sh` file with a
//! SHA-256 fingerprint and a `waybill:pants-target=<address>`
//! annotation.
//!
//! Also parses `pants.toml` at the scan root for `[shellcheck]` /
//! `[shfmt]` / `[shunit2]` `version = "..."` pins and emits each as
//! a design-tier `pkg:generic/*` build-tool component.
//!
//! Fail-open contract: per-file AND per-target corruption logs a
//! WARN and is skipped; standalone coursier lockfiles / non-Pants
//! BUILD files log INFO and are skipped; the whole scan never aborts
//! on shell-reader issues.
//!
//! See `specs/225-pants-shell-reader/` for spec + plan + contracts.

pub mod build_dsl;
pub mod component_emit;
pub mod config;
pub mod target_resolver;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use waybill_common::resolution::LifecycleScope;

use super::exclude_path::ExclusionSet;
use super::PackageDbEntry;
use crate::scan_fs::walk::{safe_walk, WalkConfig};

/// The four built-in Pants shell backend target types we recognize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellTargetKind {
    /// `shell_source(name=..., source="a.sh")` — single file, runtime.
    ShellSource,
    /// `shell_sources(name=..., sources=["*.sh"])` — glob, runtime.
    ShellSources,
    /// `shunit2_test(name=..., source="a_test.sh")` — single file, dev.
    Shunit2Test,
    /// `shunit2_tests(name=..., sources=["*_test.sh"])` — glob, dev.
    Shunit2Tests,
}

impl ShellTargetKind {
    /// FR-008 lifecycle-scope classification: shunit2 variants tag
    /// Development; shell_source variants tag Runtime.
    pub(crate) fn lifecycle_scope(self) -> LifecycleScope {
        match self {
            Self::Shunit2Test | Self::Shunit2Tests => LifecycleScope::Development,
            Self::ShellSource | Self::ShellSources => LifecycleScope::Runtime,
        }
    }

    /// Function-call name (matches the DSL target-function identifier).
    /// Diagnostic helper; not currently invoked from production paths.
    #[allow(dead_code)]
    pub(crate) fn as_dsl_name(self) -> &'static str {
        match self {
            Self::ShellSource => "shell_source",
            Self::ShellSources => "shell_sources",
            Self::Shunit2Test => "shunit2_test",
            Self::Shunit2Tests => "shunit2_tests",
        }
    }
}

/// One parsed target declaration extracted from a BUILD file.
#[derive(Debug, Clone)]
pub(crate) struct TargetDeclaration {
    /// Which of the 4 built-in shell target types this declaration invokes.
    pub(crate) kind: ShellTargetKind,
    /// The `name=` kwarg value. `None` for `shell_sources` /
    /// `shunit2_tests` when omitted (Pants defaults to the dir name);
    /// the resolver fills that in as the parent directory's basename.
    pub(crate) name: Option<String>,
    /// The source expression — either a single string literal or a
    /// list of glob patterns.
    pub(crate) source: TargetSource,
    /// 1-based line number of the target's opening `(` for diagnostics.
    /// Currently surfaced only in `TargetParseError` variants.
    #[allow(dead_code)]
    pub(crate) start_line: u32,
}

/// Source expression: single-string vs list of glob patterns.
#[derive(Debug, Clone)]
pub(crate) enum TargetSource {
    /// From `source="path.sh"`.
    Single(String),
    /// From `sources=["*.sh", ...]`. Empty vec = operator omitted the
    /// `sources=` kwarg (Pants default applies — R1 says `["*.sh", "*.bash"]`).
    Globs(Vec<String>),
}

/// Fully-resolved target: address + on-disk files.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedTarget {
    /// Canonical Pants target address (`<dir>:<name>` — bare `<name>`
    /// for root-BUILD-file targets).
    pub(crate) address: String,
    /// Preserved so the emit layer knows which lifecycle scope to tag.
    pub(crate) kind: ShellTargetKind,
    /// Zero or more `.sh` files that survive existence check on disk.
    /// Empty for globs with no matches (INFO diagnostic, not a WARN).
    pub(crate) files: Vec<PathBuf>,
}

/// Parse-time failure modes for a single target declaration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum TargetParseError {
    #[error("target has no name= or source= kwarg (line {line})")]
    MissingRequiredKwarg { line: u32 },
    #[error("target has non-string-literal source expression at line {line}: {snippet}")]
    NonStringLiteralSource { line: u32, snippet: String },
    #[error("unbalanced parens starting at line {line}")]
    UnbalancedParens { line: u32 },
}

/// Discover every `BUILD` file under `scan_root`. Uses `safe_walk`
/// (respects symlink-cycle guard, `--exclude-path`, depth limits).
fn discover_build_files(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let cfg = WalkConfig {
        max_depth: 32,
        should_skip: &|_candidate, _rootfs| false,
        exclude_set,
    };
    safe_walk(scan_root, &cfg, |path| {
        if path.is_file()
            && path.file_name().and_then(|s| s.to_str()) == Some("BUILD")
        {
            out.push(path.to_path_buf());
        }
    });
    out
}

/// Public entry — orchestrates BUILD-file discovery, regex extraction,
/// target resolution, component emission, and `pants.toml` tool-pin
/// discovery. Returns `Vec::new()` and emits NO log line when zero
/// BUILD files are discovered AND no `pants.toml` is present at the
/// scan root (byte-identity guarantee per FR-011 / SC-003).
pub fn read(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry> {
    let build_files = discover_build_files(scan_root, exclude_set);
    let pants_toml_path = scan_root.join("pants.toml");
    let pants_toml_present = pants_toml_path.is_file();
    if build_files.is_empty() && !pants_toml_present {
        return Vec::new();
    }

    let build_files_discovered = build_files.len();
    let mut build_files_parsed_ok: usize = 0;
    let mut build_files_skipped_corrupt: usize = 0;
    let mut shell_targets_found: usize = 0;

    // File-path → (kind, [owning target addresses])
    // Multiple targets may resolve to the same file (SC-006 dedup).
    let mut file_to_owners: BTreeMap<
        PathBuf,
        (ShellTargetKind, Vec<String>),
    > = BTreeMap::new();

    for build_file in &build_files {
        let bytes = match std::fs::read(build_file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    build_file = %build_file.display(),
                    error = %e,
                    "pants-shell reader: could not read BUILD file; skipping"
                );
                build_files_skipped_corrupt += 1;
                continue;
            }
        };
        let results = build_dsl::extract_targets(&bytes);
        if results.is_empty() {
            // No recognized shell targets in this BUILD file — this is
            // legal (BUILD files can carry other target types like
            // python_source, resource, etc.). NOT a skip.
            build_files_parsed_ok += 1;
            continue;
        }
        let mut any_ok = false;
        let mut any_err = false;
        for r in results {
            match r {
                Ok(decl) => {
                    any_ok = true;
                    shell_targets_found += 1;
                    let resolved =
                        target_resolver::resolve_target(&decl, build_file, scan_root);
                    for f in resolved.files {
                        let canonical =
                            std::fs::canonicalize(&f).unwrap_or_else(|_| f.clone());
                        let entry = file_to_owners
                            .entry(canonical)
                            .or_insert_with(|| (resolved.kind, Vec::new()));
                        // Lifecycle-scope wins for Development if ANY
                        // owning target is a shunit2 (dev-scope wins
                        // over runtime for shared files — dev is the
                        // safer default per contracts §"Lifecycle-
                        // scope on merged targets").
                        if resolved.kind.lifecycle_scope() == LifecycleScope::Development {
                            entry.0 = resolved.kind;
                        }
                        entry.1.push(resolved.address.clone());
                    }
                }
                Err(e) => {
                    any_err = true;
                    tracing::warn!(
                        build_file = %build_file.display(),
                        error = %e,
                        "pants-shell reader: target parse error; skipping this target"
                    );
                }
            }
        }
        // Per-file counts: parsed_ok if at least one target parsed;
        // skipped_corrupt only when EVERY target failed AND we found
        // no successes (rare — happens on totally-broken BUILD files).
        if any_ok {
            build_files_parsed_ok += 1;
        } else if any_err {
            build_files_skipped_corrupt += 1;
        }
    }

    // Emit one component per unique file, with all owning targets
    // merged into the annotation.
    let mut components: Vec<PackageDbEntry> = Vec::new();
    for (file, (kind, mut addresses)) in file_to_owners {
        addresses.sort();
        addresses.dedup();
        if let Some(pkg) = component_emit::script_to_package_db_entry(
            &file, &addresses, kind, scan_root,
        ) {
            components.push(pkg);
        }
    }
    let script_components_emitted = components.len();

    // pants.toml tool pins.
    let mut tool_components_emitted: usize = 0;
    if pants_toml_present {
        if let Ok(bytes) = std::fs::read(&pants_toml_path) {
            match config::parse(&bytes) {
                Some(cfg) => {
                    let mut emit_tool = |name: &str, section: Option<&config::ExternalToolSection>| {
                        if let Some(sec) = section {
                            if let Some(v) = sec.version.as_deref() {
                                if !v.is_empty() {
                                    if let Some(pkg) = component_emit::tool_to_package_db_entry(
                                        name,
                                        v,
                                        &pants_toml_path,
                                        scan_root,
                                    ) {
                                        components.push(pkg);
                                        tool_components_emitted += 1;
                                    }
                                }
                            }
                        }
                    };
                    emit_tool("shellcheck", cfg.shellcheck.as_ref());
                    emit_tool("shfmt", cfg.shfmt.as_ref());
                    emit_tool("shunit2", cfg.shunit2.as_ref());
                }
                None => {
                    tracing::warn!(
                        pants_toml = %pants_toml_path.display(),
                        "pants-shell reader: pants.toml could not be parsed as TOML; skipping tool-pin extraction"
                    );
                }
            }
        } else {
            tracing::warn!(
                pants_toml = %pants_toml_path.display(),
                "pants-shell reader: pants.toml could not be read; skipping tool-pin extraction"
            );
        }
    }

    tracing::info!(
        build_files_discovered,
        build_files_parsed_ok,
        build_files_skipped_corrupt,
        shell_targets_found,
        script_components_emitted,
        tool_components_emitted,
        "pants-shell reader complete"
    );

    components
}
