//! CMake source-tree reader (milestone 102 US2 / milestone 103 impl;
//! extended by milestone 155 to parse `find_package` +
//! `pkg_check_modules` — reversing the milestone-102 FR-007 refusal).
//!
//! Parses `CMakeLists.txt` + included `.cmake` files for:
//! - `FetchContent_Declare(<name> GIT_REPOSITORY ... GIT_TAG ...)` — emits
//!   `pkg:github/<owner>/<repo>@<tag>` for GitHub-hosted URLs, otherwise
//!   `pkg:generic/<name>@<tag>` with `waybill:download-url`.
//! - `FetchContent_Declare(<name> URL ... URL_HASH SHA256=...)` and
//!   `ExternalProject_Add(<name> URL ...)` — emits `pkg:generic/<name>@<version>`
//!   with URL + SHA-256.
//! - `add_subdirectory(third_party|vendor/<name>)` — opt-in via the
//!   `include_vendored` parameter (wired from PR-A's CLI flag); emits
//!   `pkg:generic/<name>@<version-from-version.txt>` with the JSON
//!   boolean `waybill:vendored = true` annotation.
//! - **milestone 155**: `find_package(<Name> [<Version>])` — emits
//!   `pkg:generic/<lowercased-name>[@<highest-declared-version>]` with
//!   `waybill:source-mechanism = "cmake-find-package"`. Multi-file
//!   same-name declarations are consolidated to the highest declared
//!   version (Q1 clarification). Original casing preserved in the
//!   `waybill:cmake-find-package-name` annotation when it differs from
//!   the lowercased PURL. Same-PURL cross-mechanism double-counting is
//!   prevented by the production `resolve::deduplicator` pass.
//! - **milestone 155**: `pkg_check_modules(<TARGET> <modules>)` +
//!   `pkg_search_module(<TARGET> <modules>)` — emits one
//!   `pkg:generic/<module>` per module (target variable discarded,
//!   version constraints stripped) with
//!   `waybill:source-mechanism = "cmake-pkg-check-modules"`.
//!
//! Walks the scan root for `CMakeLists.txt` at depth-0. Under
//! `cmake/` and `Modules/`, recursive descent captures every
//! `.cmake` file + `CMakeLists.txt` at any depth (milestone 156 —
//! reaches nested `Find*.cmake` files like Kamailio's
//! `cmake/modules/Find*.cmake`). Under `third_party/`, depth-1 walk
//! by default (matches milestone-102 behavior); opt in to recursive
//! descent via `--cmake-third-party-recursive` (env alias
//! `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE=1`) to also walk vendored
//! deps' own `find_package` declarations.
//!
//! Cross-platform; no `#[cfg(unix)]` gates. Zero new Cargo deps —
//! uses workspace `regex` + std.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use waybill_common::types::hash::{ContentHash, HashAlgorithm};
use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

// Milestone 664 US2 T056: shared-walker marker-detect registration.
use crate::scan_fs::walk_registry::{
    globset_from_patterns_case_insensitive, ReaderId, ReaderRegistration, SharedWalkerContext,
};

/// Milestone 155 — one hit per successfully matched `find_package(<Name>
/// [<Version>])` call site. Accumulated across all discovered CMake
/// files by `read()`, then consumed by `emit_find_package_entries`.
struct FindPackageHit {
    lowercased_name: String,
    original_casing: String,
    /// The version requirement the call states, as a range (#1520):
    /// `>=3.9`, `==2.1` with `EXACT`, or `>=3.9, <=4.0` for `3.9...4.0`.
    requirement: Option<String>,
    source_path: String,
}

/// Milestone 155 — one hit per module in a `pkg_check_modules` /
/// `pkg_search_module` module list. Accumulated across all discovered
/// CMake files by `read()`, then consumed by `emit_pkg_check_module_entries`.
struct PkgCheckHit {
    lowercased_module: String,
    #[allow(dead_code)] // Reserved for future case-preservation traceability.
    original_casing: String,
    source_path: String,
}

/// Milestone 664 US2 T056: marker-detect state. CMake's discovery
/// is subdir-targeted; the shared walker registration only gates the
/// existing `read()` on presence of a `CMakeLists.txt` OR `*.cmake`
/// marker anywhere in the tree.
#[derive(Default, Debug)]
pub(crate) struct CMakeMarkerState {
    pub(crate) seen: bool,
}

fn on_cmake_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    // `is_cmake_file` matches basename `CMakeLists.txt`
    // (case-insensitive) OR extension `.cmake` (case-insensitive).
    if !is_cmake_file(path) {
        return;
    }
    let Some(state) = ctx.state::<Mutex<CMakeMarkerState>>(ReaderId::CMAKE) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.seen = true;
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    // Match `CMakeLists.txt` + `*.cmake` case-insensitive.
    let patterns = globset_from_patterns_case_insensitive(&[
        "**/CMakeLists.txt",
        "**/*.cmake",
    ])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::CMAKE,
        state: Some(Arc::new(Mutex::new(CMakeMarkerState::default()))),
        patterns,
        on_file: Some(on_cmake_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_marker(registration: &ReaderRegistration) -> CMakeMarkerState {
    let Some(state_arc) = registration.state.as_ref() else {
        return CMakeMarkerState::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<CMakeMarkerState>>() else {
        return CMakeMarkerState::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Post-walker entry — gates the subdir-targeted discovery on marker
/// presence + defensive fs-existence fallback (scan-root
/// `CMakeLists.txt` OR any of the three targeted subdirs).
pub(crate) fn finalize(
    marker: CMakeMarkerState,
    scan_root: &Path,
    include_vendored: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    if !marker.seen
        && !scan_root.join("CMakeLists.txt").is_file()
        && !scan_root.join("cmake").is_dir()
        && !scan_root.join("Modules").is_dir()
        && !scan_root.join("third_party").is_dir()
    {
        return Vec::new();
    }
    read(scan_root, include_vendored, exclude_set)
}

pub fn read(
    scan_root: &Path,
    include_vendored: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    // Milestone 156: opt-in recursive descent for third_party/. Default
    // depth-1 (matches milestone-102 behavior) unless the CLI flag
    // `--cmake-third-party-recursive` OR the env var
    // `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE=1` is set. Mirrors the
    // milestone-102 WAYBILL_INCLUDE_VENDORED env-var propagation
    // pattern at read_all:1193 to avoid plumbing a new bool through
    // the 75-callsite scan_path -> read_all chain.
    let include_third_party_recursive = std::env::var("WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let cmake_files = discover_cmake_files(scan_root, include_third_party_recursive, exclude_set);
    let mut entries = Vec::new();
    // Milestone 155: accumulate find_package + pkg_check_modules hits
    // across ALL discovered CMake files so we can pick the highest
    // declared version per name (Q1 clarification) before emitting.
    let mut find_package_hits: Vec<FindPackageHit> = Vec::new();
    let mut pkg_check_hits: Vec<PkgCheckHit> = Vec::new();
    for path in &cmake_files {
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to read CMake file (FR-013)"
                );
                continue;
            }
        };
        let source_path = path.to_string_lossy().to_string();
        entries.extend(parse_fetch_block(
            &content,
            &source_path,
            "FetchContent_Declare",
        ));
        entries.extend(parse_fetch_block(
            &content,
            &source_path,
            "ExternalProject_Add",
        ));
        if include_vendored {
            entries.extend(parse_vendored(&content, &source_path, scan_root));
        }
        find_package_hits.extend(parse_find_package_calls(&content, &source_path));
        pkg_check_hits.extend(parse_pkg_check_modules_calls(&content, &source_path));
    }
    entries.extend(emit_find_package_entries(find_package_hits));
    entries.extend(emit_pkg_check_module_entries(pkg_check_hits));
    entries
}

/// Milestone 155 — parse `find_package(<Name> [<Version>])` call sites
/// from a CMake file body. Returns one hit per call site (dedup + version
/// consolidation happen later in `emit_find_package_entries`).
///
/// Regex per research §R2.1:
/// - `^[^#\n]*?` comment-strip prefix (FR-011): rejects lines where
///   `#` precedes the token.
/// - `\bfind_package\s*\(` requires the immediate open-paren so
///   `find_package_handle_standard_args(...)` (FR-009) does NOT match
///   — the `_handle_standard_args` suffix prevents `\s*\(` from matching
///   immediately after the `find_package` prefix.
/// - Capture group 1: name in the CMake identifier alphabet
///   `[A-Za-z0-9_:.+-]+`. Excludes `$`, `{`, `}` so `find_package(${VAR})`
///   (FR-010) fails to capture.
/// - Capture group 2 (optional): version — MUST start with a digit to
///   distinguish from modifier keywords like `REQUIRED`, `QUIET`,
///   `EXACT`, `CONFIG`, `MODULE`, `COMPONENTS`, `NO_MODULE`.
fn parse_find_package_calls(content: &str, source_path: &str) -> Vec<FindPackageHit> {
    static FIND_PACKAGE_V155: OnceLock<Regex> = OnceLock::new();
    let re = FIND_PACKAGE_V155.get_or_init(|| {
        Regex::new(
            r"(?im)^[^#\n]*?\bfind_package\s*\(\s*([A-Za-z0-9_:.+-]+)(?:\s+([0-9][A-Za-z0-9._-]*(?:<[0-9][A-Za-z0-9._-]*)?))?([^)]*)",
        )
        .expect("find_package v155 regex compiles")
    });

    // Secondary pattern for the `find_package(${VAR})` diagnostic
    // (FR-010). Best-effort log-only; does not affect emission.
    static FIND_PACKAGE_VAR: OnceLock<Regex> = OnceLock::new();
    let var_re = FIND_PACKAGE_VAR
        .get_or_init(|| {
            Regex::new(r"(?im)\bfind_package\s*\(\s*\$\{")
                .expect("find_package var-interp regex compiles")
        });
    if var_re.is_match(content) {
        tracing::debug!(
            source = %source_path,
            "milestone-155: find_package(${{VAR}}) skipped — CMake variable interpolation not resolved"
        );
    }

    let mut out = Vec::new();
    for cap in re.captures_iter(content) {
        let name_raw = match cap.get(1) {
            Some(m) => m.as_str(),
            None => continue,
        };
        let exact = cap
            .get(3)
            .is_some_and(|m| m.as_str().split_whitespace().any(|t| t == "EXACT"));
        let requirement = cap.get(2).map(|m| find_package_requirement(m.as_str(), exact));
        let lowercased = name_raw.to_lowercase();
        out.push(FindPackageHit {
            lowercased_name: lowercased,
            original_casing: name_raw.to_string(),
            requirement,
            source_path: source_path.to_string(),
        });
    }
    out
}

/// The range a `find_package` version argument states (#1520). A bare
/// version is a minimum unless `EXACT` follows; `min...max` includes `max`
/// and `min...<max` excludes it (CMake 3.19 version ranges).
fn find_package_requirement(version: &str, exact: bool) -> String {
    match version.split_once("...") {
        Some((min, max)) => match max.strip_prefix('<') {
            Some(max) => format!(">={min}, <{max}"),
            None => format!(">={min}, <={max}"),
        },
        None if exact => format!("=={version}"),
        None => format!(">={version}"),
    }
}

/// C151 reason for every `find_package` component (#1520): the call states
/// a requirement, and which version satisfies it is decided at build time.
pub(crate) const CMAKE_FIND_PACKAGE_REASON: &str =
    "find_package() states a requirement; the version found at build time is not in source";

/// Milestone 155 — parse `pkg_check_modules` / `pkg_search_module` call
/// sites. Returns one hit per module in each call's module list.
///
/// The regex captures the CMake TARGET variable (discarded) and the raw
/// module-list body; the body is split on whitespace and each token is:
/// - Filtered against the modifier-keyword set
///   `{REQUIRED, IMPORTED_TARGET, GLOBAL, QUIET, NO_CMAKE_PATH,
///     NO_CMAKE_ENVIRONMENT_PATH}` (case-insensitive).
/// - Stripped of any pkg-config version-constraint suffix
///   (`>=X.Y`, `<=X.Y`, `>X.Y`, `<X.Y`, `=X.Y`, `==X.Y`).
/// - Lowercased for PURL emission.
fn parse_pkg_check_modules_calls(content: &str, source_path: &str) -> Vec<PkgCheckHit> {
    static PKG_CHECK: OnceLock<Regex> = OnceLock::new();
    let re = PKG_CHECK.get_or_init(|| {
        Regex::new(
            r"(?im)^[^#\n]*?\bpkg_(?:check_modules|search_module)\s*\(\s*([A-Za-z0-9_]+)((?:\s+[A-Za-z0-9_>=<.+-]+)+)",
        )
        .expect("pkg_check_modules v155 regex compiles")
    });

    // Version-comparator stripper: matches a leading module name in the
    // identifier alphabet, optionally followed by a comparator + version.
    static MODULE_NAME: OnceLock<Regex> = OnceLock::new();
    let module_re = MODULE_NAME.get_or_init(|| {
        Regex::new(r"^([A-Za-z0-9_.+-]+)(?:[<>=]=?.*)?$")
            .expect("pkg-config module name regex compiles")
    });

    let modifier_keywords: &[&str] = &[
        "REQUIRED",
        "IMPORTED_TARGET",
        "GLOBAL",
        "QUIET",
        "NO_CMAKE_PATH",
        "NO_CMAKE_ENVIRONMENT_PATH",
    ];

    let mut out = Vec::new();
    for cap in re.captures_iter(content) {
        let body = match cap.get(2) {
            Some(m) => m.as_str(),
            None => continue,
        };
        for token in body.split_whitespace() {
            let upper = token.to_ascii_uppercase();
            if modifier_keywords.contains(&upper.as_str()) {
                continue;
            }
            // Strip version constraint if present.
            let module_name = match module_re.captures(token) {
                Some(c) => match c.get(1) {
                    Some(m) => m.as_str().to_string(),
                    None => continue,
                },
                None => continue,
            };
            if module_name.is_empty() {
                continue;
            }
            let lowercased = module_name.to_lowercase();
            out.push(PkgCheckHit {
                lowercased_module: lowercased,
                original_casing: module_name,
                source_path: source_path.to_string(),
            });
        }
    }
    out
}

/// Milestone 155 — emit `PackageDbEntry` instances for `find_package`
/// hits: one entry per call site, so the milestone-148 source-file-paths
/// union merges them by PURL.
///
/// #1520: a `find_package` version is a requirement, not a resolution, so
/// the PURL carries no version. Every call site's range is recorded in
/// `waybill:requirement-ranges`, and the component is design-tier with a
/// C151 reason. Before #1520 the highest declared minimum became the
/// component version (milestone 155, Q1).
fn emit_find_package_entries(hits: Vec<FindPackageHit>) -> Vec<PackageDbEntry> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut groups: BTreeMap<String, Vec<FindPackageHit>> = BTreeMap::new();
    for hit in hits {
        groups.entry(hit.lowercased_name.clone()).or_default().push(hit);
    }
    let mut out = Vec::new();
    for (lowercased_name, group) in groups {
        let ranges: Vec<String> = group
            .iter()
            .filter_map(|h| h.requirement.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let purl_str = format!("pkg:generic/{}", encode_purl_segment(&lowercased_name));
        for hit in group {
            let purl = match Purl::new(&purl_str) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let mut entry = build_cmake_entry(
                &lowercased_name,
                "",
                &hit.source_path,
                purl,
                None,
                None,
                false,
                "cmake-find-package",
            );
            entry.requirement_ranges = ranges.clone();
            entry.sbom_tier = Some("design".to_string());
            entry.extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(CMAKE_FIND_PACKAGE_REASON.to_string()),
            );
            // FR-008: preserve original casing when it differs from the
            // lowercased PURL name — parity-bridging annotation.
            if hit.original_casing != hit.lowercased_name {
                entry.extra_annotations.insert(
                    "waybill:cmake-find-package-name".to_string(),
                    serde_json::json!(hit.original_casing),
                );
            }
            out.push(entry);
        }
    }
    out
}

/// Milestone 155 — emit `PackageDbEntry` instances for `pkg_check_modules`
/// hits. No version consolidation (pkg-config version constraints are
/// stripped at parse time); each hit emits one entry.
fn emit_pkg_check_module_entries(hits: Vec<PkgCheckHit>) -> Vec<PackageDbEntry> {
    let mut out = Vec::new();
    for hit in hits {
        let purl_str = format!(
            "pkg:generic/{}",
            encode_purl_segment(&hit.lowercased_module)
        );
        let purl = match Purl::new(&purl_str) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let entry = build_cmake_entry(
            &hit.lowercased_module,
            "",
            &hit.source_path,
            purl,
            None,
            None,
            false,
            "cmake-pkg-check-modules",
        );
        out.push(entry);
    }
    out
}

/// Collect the set of `find_package(<target> ...)` target names AND
/// `add_library(... ALIAS ...)` alias/target names declared anywhere
/// under the scan root's CMake files. Used by the milestone-105
/// `git-submodule` reader (FR-008a) to classify each `.gitmodules`
/// entry as `waybill:build-reference: "declared-and-used"` (the
/// submodule's path basename appears in this set) or
/// `"declared-only"` (it doesn't).
///
/// Names are case-folded to lowercase for case-insensitive matching
/// per FR-008a. The returned set is order-independent
/// (`BTreeSet<String>`) so submodule classification is deterministic
/// regardless of filesystem walk order (SC-010).
///
/// IMPORTANT: this fn does NOT emit components. It only collects
/// target names for the milestone-105 US6 `git-submodule` classification
/// pipeline. Milestone 155 later added a SEPARATE emission path
/// (`parse_find_package_calls` + `emit_find_package_entries`) that
/// DOES emit `pkg:generic/<name>` PackageDbEntry instances from the
/// same `find_package(...)` call sites. The two pipelines are
/// orthogonal — this collector populates a name set for submodule
/// classification, while the emitter produces the SBOM components.
/// Both walk the same discovered CMake files.
///
/// Dynamic aliases set inside CMake macros / functions are not
/// chased (per spec edge case "target aliases"). Only statically
/// visible `add_library(... ALIAS ...)` declarations contribute.
///
/// `#[allow(dead_code)]`: the consumer is the milestone-105 US6
/// `git_submodule` reader (T089), which lands in a later commit of
/// this milestone. Tests in this module exercise the collector now,
/// but no production call site exists yet — clippy's `dead_code` lint
/// would otherwise reject the public fn.
pub fn collect_find_package_targets(scan_root: &Path) -> BTreeSet<String> {
    // Milestone 156 — reuses the extended discover_cmake_files but
    // passes false for `include_third_party_recursive` (the collector's
    // name-set semantics don't depend on third_party depth) and an
    // empty ExclusionSet (the collector is a milestone-105 US6 helper
    // orthogonal to operator-supplied --exclude-path).
    let cmake_files = discover_cmake_files(
        scan_root,
        false,
        &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty(),
    );
    let mut out: BTreeSet<String> = BTreeSet::new();
    for path in &cmake_files {
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to read CMake file for find_package collection (FR-013)"
                );
                continue;
            }
        };
        collect_into(&content, &mut out);
    }
    out
}

/// Internal helper: extract `find_package(...)` + `add_library(...
/// ALIAS ...)` names from a single CMake file body and insert them
/// into the accumulator. Pure function — used by the public collector
/// and by unit tests. `#[allow(dead_code)]` per the public fn's note
/// (consumer lands in US6 / T089).
/// Spec 1530 FR-008, research R10: directories CMake files reference by
/// path — `add_subdirectory(<dir>)`, and the `SOURCE_DIR <dir>` argument of
/// `ExternalProject_Add` / `FetchContent_Declare`. Each is resolved against
/// the referencing file's directory after substituting the common
/// source-directory variables, and lexically normalised. A path that still
/// holds `${` after substitution is ignored.
///
/// Reads the files [`discover_cmake_files`] returns, so it shares #1516's
/// limit: nested projects outside the root, `cmake/`, `Modules/` and
/// `third_party/` are not seen.
pub fn collect_cmake_path_references(scan_root: &Path) -> BTreeSet<PathBuf> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let mut out = BTreeSet::new();
    let Some(re) = RE
        .get_or_init(|| {
            Regex::new(r#"(?i)(?:\badd_subdirectory\s*\(\s*|\bSOURCE_DIR\s+)(?:"([^"]*)"|([^\s)"]+))"#).ok()
        })
        .as_ref()
    else {
        return out;
    };
    let root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let files = discover_cmake_files(
        scan_root,
        false,
        &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty(),
    );
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else { continue };
        let dir = file
            .parent()
            .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf()))
            .unwrap_or_else(|| root.clone());
        let dir_s = dir.to_string_lossy().into_owned();
        let root_s = root.to_string_lossy().into_owned();
        for c in re.captures_iter(&content) {
            let Some(raw) = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()) else { continue };
            let v = raw
                .replace("${CMAKE_CURRENT_SOURCE_DIR}", &dir_s)
                .replace("${CMAKE_CURRENT_LIST_DIR}", &dir_s)
                .replace("${PROJECT_SOURCE_DIR}", &root_s)
                .replace("${CMAKE_SOURCE_DIR}", &root_s);
            if v.contains("${") || v.is_empty() {
                continue;
            }
            let p = Path::new(&v);
            let abs = if p.is_absolute() { p.to_path_buf() } else { dir.join(p) };
            out.insert(lexical_normalize(&abs));
        }
    }
    out
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Spec 1530 FR-008 (C57): `declared-and-used` when a `find_package` target
/// name case-insensitively equals the submodule directory's last segment,
/// or a CMake path reference names the submodule directory or a path
/// inside it; `declared-only` otherwise. `submodule_dir` is absolute and
/// normalised; `find_package_targets` are lowercased by the collector.
pub(crate) fn build_reference(
    submodule_dir: &Path,
    find_package_targets: &BTreeSet<String>,
    path_refs: &BTreeSet<PathBuf>,
) -> &'static str {
    let by_name = submodule_dir
        .file_name()
        .map(|n| find_package_targets.contains(&n.to_string_lossy().to_lowercase()))
        .unwrap_or(false);
    let by_path = path_refs.iter().any(|r| r.starts_with(submodule_dir));
    if by_name || by_path {
        "declared-and-used"
    } else {
        "declared-only"
    }
}

fn collect_into(content: &str, out: &mut BTreeSet<String>) {
    // `find_package(<target> ...)` — target is the first token after
    // `(`. CMake target names per the language reference are letters
    // / digits / `_`, plus the namespace separator `::`. Trailing
    // characters (whitespace, keywords like REQUIRED, CONFIG, version
    // numbers) are not captured.
    static FIND_PACKAGE: OnceLock<Regex> = OnceLock::new();
    let find_package_re = FIND_PACKAGE.get_or_init(|| {
        Regex::new(r"(?i)\bfind_package\s*\(\s*([A-Za-z0-9_:.+-]+)")
            .expect("find_package regex compiles")
    });
    for cap in find_package_re.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            insert_name(name.as_str(), out);
        }
    }

    // `add_library(<alias-name> ALIAS <target>)`. Both the alias
    // name AND the target name go into the set so downstream
    // submodule classification matches either form a `find_package`
    // call might use.
    //
    // Recognized shapes (whitespace + comment tolerant):
    //   add_library(Foo::Foo ALIAS foo)
    //   add_library(Foo ALIAS foo)
    //   add_library(SomeLib::SomeLib ALIAS someimpl)
    static ALIAS: OnceLock<Regex> = OnceLock::new();
    let alias_re = ALIAS.get_or_init(|| {
        Regex::new(
            r"(?i)\badd_library\s*\(\s*([A-Za-z0-9_:.+-]+)\s+ALIAS\s+([A-Za-z0-9_:.+-]+)",
        )
        .expect("add_library ALIAS regex compiles")
    });
    for cap in alias_re.captures_iter(content) {
        if let Some(alias_name) = cap.get(1) {
            insert_name(alias_name.as_str(), out);
        }
        if let Some(target_name) = cap.get(2) {
            insert_name(target_name.as_str(), out);
        }
    }
}

/// Normalize a CMake target name for the find_package_targets set:
/// case-fold to lowercase, strip a leading namespace if present
/// (`Foo::Bar` → `bar`; also insert `foo` separately so submodule
/// paths matching either part are recognized). `#[allow(dead_code)]`
/// per the public fn's note (consumer lands in US6 / T089).
#[allow(dead_code)]
fn insert_name(raw: &str, out: &mut BTreeSet<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    // Insert the full name (case-folded). Useful when the name has
    // no `::` namespace separator at all.
    out.insert(trimmed.to_lowercase());
    // If the name has a `Foo::Bar` form, ALSO insert each segment
    // separately so a submodule named after either Foo or Bar is
    // recognized.
    if trimmed.contains("::") {
        for segment in trimmed.split("::") {
            let s = segment.trim();
            if !s.is_empty() {
                out.insert(s.to_lowercase());
            }
        }
    }
}

/// Milestone 156 helper: predicate for CMake file discovery. A path
/// counts as a CMake file when it has a `.cmake` extension
/// (case-insensitive) OR its filename is `CMakeLists.txt`
/// (case-insensitive).
fn is_cmake_file(p: &Path) -> bool {
    let is_cmake_module = p
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("cmake"))
        .unwrap_or(false);
    let is_cmakelists = p
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("CMakeLists.txt"))
        .unwrap_or(false);
    is_cmake_module || is_cmakelists
}

/// Milestone 156 helper: depth-1 walk of a directory. Preserves the
/// milestone-102 behavior for `third_party/` when the
/// `--cmake-third-party-recursive` opt-in is not set. Push every
/// CMake file at depth-1 (no recursive descent).
fn collect_cmake_files_depth1(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let p = entry.path();
        if p.is_file() && is_cmake_file(&p) {
            out.push(p);
        }
    }
}

/// Milestone 156 helper: recursive walk of a directory via
/// milestone-054's `safe_walk`. Inherits symlink-cycle safety
/// (canonicalize-keyed visited-set), rootfs sandbox enforcement,
/// and `tracing::debug!` skip logging.
///
/// Milestone-113 `--exclude-path` integration is done here rather
/// than via safe_walk's `exclude_set` config: safe_walk relativizes
/// candidate paths against its `rootfs` argument (`subdir_root` in
/// our case, e.g. `scan_root/cmake/`), but operators write
/// `--exclude-path` values relative to the SCAN ROOT
/// (`scan_root/`). So we pass safe_walk an empty ExclusionSet and
/// perform the match ourselves using the scan-root-relative path.
///
/// `max_depth = 16` is a defensive backstop for the canonicalize-keyed
/// visited-set: if canonicalization is unavailable (sandboxed
/// filesystem, missing realpath perms), max_depth guarantees bounded
/// termination. Realistic projects have depth <5; 16 accommodates any
/// legitimate hierarchy.
fn collect_cmake_files_recursive(
    scan_root: &Path,
    subdir_root: &Path,
    exclude_set: &super::exclude_path::ExclusionSet,
    out: &mut Vec<PathBuf>,
) {
    use crate::scan_fs::walk::{safe_walk, WalkConfig};
    // Pass safe_walk an empty ExclusionSet (rootfs mismatch would
    // silently miss operator-supplied scan-root-relative patterns).
    // We do the match ourselves in the visit closure below.
    let empty = super::exclude_path::ExclusionSet::new_empty();
    let cfg = WalkConfig {
        max_depth: 16,
        should_skip: &|_candidate: &Path, _rootfs: &Path| false,
        exclude_set: &empty,
    };
    // FR-005 permanent escape hatch — this walker cannot migrate to
    // walk_registry because it's anchored to a specific cmake-family
    // subdirectory (`cmake/`, `Modules/`, `third_party/`), not to the
    // scan root. CMake discovery is subdir-targeted; a whole-tree
    // shared walker would over-visit. Milestone 664 US2 T056.
    safe_walk(subdir_root, &cfg, |path: &Path| {
        if !path.is_file() || !is_cmake_file(path) {
            return;
        }
        // Milestone-113 --exclude-path integration: match against the
        // scan-root-relative path form that operators write.
        if !exclude_set.is_empty() {
            if let Ok(rel) = path.strip_prefix(scan_root) {
                let rel_str = rel.to_string_lossy();
                if exclude_set.matches(&rel_str) {
                    tracing::debug!(
                        candidate = %rel.display(),
                        cause = "exclude-path",
                        "cmake walker: skipping file matched by --exclude-path"
                    );
                    return;
                }
            }
        }
        out.push(path.to_path_buf());
    });
}

/// Discover CMake files (milestone 156 extended scope):
/// - Top-level `<scan_root>/CMakeLists.txt` (depth-0, unchanged from
///   milestone 102).
/// - `<scan_root>/cmake/**` — recursive descent. Every `.cmake` file
///   AND every `CMakeLists.txt` file at any depth beneath `cmake/`
///   is discovered. Reaches Kamailio's `cmake/modules/Find*.cmake`
///   files that the pre-156 depth-1 walker missed.
/// - `<scan_root>/Modules/**` — same recursive descent.
/// - `<scan_root>/third_party/` — depth-1 walk by default (matches
///   milestone-102 behavior). Recursive descent applied only when
///   `include_third_party_recursive = true` (from the
///   `--cmake-third-party-recursive` CLI flag or
///   `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE=1` env var).
///
/// Reuses milestone-054's `safe_walk` for recursive descent —
/// symlink cycles caught by the canonicalize-keyed visited-set;
/// cross-scan-root symlinks refused by the rootfs sandbox;
/// milestone-113 `--exclude-path` matches consulted per-descent.
fn discover_cmake_files(
    scan_root: &Path,
    include_third_party_recursive: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let top = scan_root.join("CMakeLists.txt");
    if top.is_file() {
        out.push(top);
    }
    for subdir in &["cmake", "Modules"] {
        let dir = scan_root.join(subdir);
        if dir.is_dir() {
            collect_cmake_files_recursive(scan_root, &dir, exclude_set, &mut out);
        }
    }
    let third_party = scan_root.join("third_party");
    if third_party.is_dir() {
        if include_third_party_recursive {
            collect_cmake_files_recursive(scan_root, &third_party, exclude_set, &mut out);
        } else {
            collect_cmake_files_depth1(&third_party, &mut out);
        }
    }
    out
}

/// Parse a `FetchContent_Declare(...)` or `ExternalProject_Add(...)`
/// block. Parameterized over `rule_name` so the same body handles both.
/// Returns one `PackageDbEntry` per matched rule per research §3+§4.
/// C151 reason for a fetch whose `URL`, `GIT_REPOSITORY` or `GIT_TAG` uses a
/// variable with no single literal `set()` in the same file (#1517).
pub(crate) const CMAKE_UNRESOLVED_VARIABLE_REASON: &str =
    "URL or GIT_TAG uses a CMake variable with no literal set() in the same file";

/// Same-file `set(<VAR> <value>)` calls with exactly one literal value
/// (#1517). A variable set more than once is ambiguous without evaluating
/// control flow, so it maps to `None` and stays unresolved.
fn collect_set_vars(content: &str) -> std::collections::HashMap<String, Option<String>> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let mut vars: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    let Some(re) = RE
        .get_or_init(|| {
            Regex::new(r#"(?im)^\s*set\s*\(\s*([A-Za-z_][A-Za-z0-9_.+-]*)\s+(?:"([^"\n]*)"|([^\s()"]+))\s*\)"#).ok()
        })
        .as_ref()
    else {
        return vars;
    };
    for c in re.captures_iter(content) {
        let name = c[1].to_string();
        let value = c.get(2).or_else(|| c.get(3)).map(|m| m.as_str().to_string());
        vars.entry(name)
            .and_modify(|seen| {
                if *seen != value {
                    *seen = None;
                }
            })
            .or_insert(value);
    }
    vars
}

/// Substitute every `${VAR}` whose value is known, repeatedly, so a
/// variable built from another resolves. Unknown references stay as written.
fn expand_known_vars(text: &str, vars: &std::collections::HashMap<String, Option<String>>) -> String {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let Some(re) = RE
        .get_or_init(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_.+-]*)\}").ok())
        .as_ref()
    else {
        return text.to_string();
    };
    let mut out = text.to_string();
    for _ in 0..8 {
        let next = re
            .replace_all(&out, |c: &regex::Captures<'_>| match vars.get(&c[1]) {
                Some(Some(v)) => v.clone(),
                _ => c[0].to_string(),
            })
            .into_owned();
        if next == out {
            break;
        }
        out = next;
    }
    out
}

fn parse_fetch_block(content: &str, source_path: &str, rule_name: &str) -> Vec<PackageDbEntry> {
    // Outer envelope: rule_name + first whitespace-separated token
    // (the dep name) + everything until the matching `)`. Non-greedy
    // dotall.
    let outer_pattern = format!(
        r"(?ms){}\s*\(\s*(\S+)(.*?)\)",
        regex::escape(rule_name)
    );
    let outer = match Regex::new(&outer_pattern) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let github_re = Regex::new(r"^https?://github\.com/([^/]+)/([^/\.\s]+)").ok();
    let vars = collect_set_vars(content);
    let mut out = Vec::new();
    for c in outer.captures_iter(content) {
        let name = c.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        let body = expand_known_vars(c.get(2).map(|m| m.as_str()).unwrap_or(""), &vars);
        let body = body.as_str();
        if name.is_empty() {
            continue;
        }

        let git_repo = extract_keyword(body, "GIT_REPOSITORY");
        let git_tag = extract_keyword(body, "GIT_TAG");
        let url = extract_keyword(body, "URL");
        let url_hash = extract_url_hash(body);

        // #1517: a `${VAR}` left after expansion has no literal same-file
        // value. Say so (C151) instead of guessing a version.
        let unresolved = [&git_repo, &git_tag, &url]
            .iter()
            .any(|v| v.as_deref().is_some_and(|v| v.contains("${")));
        if unresolved && (url.is_some() || (git_repo.is_some() && git_tag.is_some())) {
            let Ok(purl) = Purl::new(&format!("pkg:generic/{}", encode_purl_segment(name))) else {
                continue;
            };
            let source_mechanism = if rule_name == "ExternalProject_Add" {
                "cmake-externalproject"
            } else if git_repo.is_some() {
                "cmake-fetchcontent-git"
            } else {
                "cmake-fetchcontent-url"
            };
            let mut entry =
                build_cmake_entry(name, "", source_path, purl, None, url_hash, false, source_mechanism);
            entry.sbom_tier = Some("design".to_string());
            entry.extra_annotations.insert(
                "waybill:unresolved-reason".to_string(),
                serde_json::Value::String(CMAKE_UNRESOLVED_VARIABLE_REASON.to_string()),
            );
            out.push(entry);
            continue;
        }

        let (purl_str, download_url, version) = if let (Some(g), Some(t)) =
            (git_repo.as_deref(), git_tag.as_deref())
        {
            // GIT form — check for GitHub URL → pkg:github/<owner>/<repo>@<tag>;
            // otherwise pkg:generic/<name>@<tag>.
            let github_pair = github_re
                .as_ref()
                .and_then(|r| r.captures(g))
                .and_then(|m| {
                    // Spec 1530 FR-015: purl-spec's `github` definition is
                    // case-insensitive and lowercased, so a CMake fetch and a
                    // git submodule of `ROCm/x` at one commit share a PURL.
                    let fold = super::forge::lowercases_owner_and_name("github");
                    let case = |s: &str| if fold { s.to_lowercase() } else { s.to_string() };
                    let owner = m.get(1)?.as_str();
                    let repo = m.get(2)?.as_str();
                    Some((case(owner), case(repo)))
                });
            let purl = match github_pair {
                Some((owner, repo)) => format!(
                    "pkg:github/{}/{}@{}",
                    encode_purl_segment(&owner),
                    encode_purl_segment(&repo),
                    encode_purl_segment(t)
                ),
                None => format!(
                    "pkg:generic/{}@{}",
                    encode_purl_segment(name),
                    encode_purl_segment(t)
                ),
            };
            (purl, Some(g.to_string()), t.to_string())
        } else if let Some(u) = url.as_deref() {
            // URL form — parse version from filename.
            let version = parse_version_from_url(u).unwrap_or_else(|| "unknown".to_string());
            (
                format!(
                    "pkg:generic/{}@{}",
                    encode_purl_segment(name),
                    encode_purl_segment(&version)
                ),
                Some(u.to_string()),
                version,
            )
        } else {
            continue;
        };

        if let Ok(purl) = Purl::new(&purl_str) {
            // Distinguish source-mechanism by rule name + presence
            // of GIT_REPOSITORY (FetchContent_Declare with git form)
            // vs URL form. ExternalProject_Add only supports URL
            // form in our parser.
            let source_mechanism = if rule_name == "ExternalProject_Add" {
                "cmake-externalproject"
            } else if git_repo.is_some() {
                "cmake-fetchcontent-git"
            } else {
                "cmake-fetchcontent-url"
            };
            out.push(build_cmake_entry(
                name,
                &version,
                source_path,
                purl,
                download_url.as_deref(),
                url_hash,
                false,
                source_mechanism,
            ));
        }
    }
    out
}

/// Extract a CMake keyword-value pair. CMake syntax: `KEYWORD value`
/// (whitespace-separated, no `=`). Value is the next non-whitespace
/// token. Returns None when keyword not found.
fn extract_keyword(body: &str, keyword: &str) -> Option<String> {
    let pattern = format!(r"\b{}\s+(\S+)", regex::escape(keyword));
    let re = Regex::new(&pattern).ok()?;
    let value = re.captures(body)?.get(1)?.as_str();
    // A quoted argument is CMake's ordinary string form (#1517).
    Some(value.trim_matches('"').to_string())
}

/// Extract `URL_HASH <ALGO>=<hex>`, quoted or not (#1517). Algorithms
/// waybill has no representation for (SHA384, SHA224, SHA3_*) yield `None`.
fn extract_url_hash(body: &str) -> Option<ContentHash> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r#"URL_HASH\s+"?\s*([A-Za-z0-9_]+)\s*=\s*([0-9a-fA-F]+)"#).ok())
        .as_ref()?;
    let c = re.captures(body)?;
    let algorithm = match c[1].to_ascii_uppercase().as_str() {
        "SHA256" => HashAlgorithm::Sha256,
        "SHA512" => HashAlgorithm::Sha512,
        "SHA1" => HashAlgorithm::Sha1,
        "MD5" => HashAlgorithm::Md5,
        other => {
            tracing::debug!(algorithm = other, "cmake: URL_HASH algorithm has no representation; hash dropped");
            return None;
        }
    };
    ContentHash::with_algorithm(algorithm, &c[2]).ok()
}

/// Parse the vendored-dep block — `add_subdirectory(third_party/<name>)`
/// or `add_subdirectory(vendor/<name>)`. Per FR-008. Only called when
/// `include_vendored = true`. Reads `<scan_root>/<prefix>/<name>/version.txt`
/// for version backfill per FR-009 + research §6.
fn parse_vendored(
    content: &str,
    source_path: &str,
    scan_root: &Path,
) -> Vec<PackageDbEntry> {
    let re = match Regex::new(
        r"(?ms)add_subdirectory\s*\(\s*(third_party|vendor)/([^)\s]+)\s*\)",
    ) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for c in re.captures_iter(content) {
        let prefix = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let name = c.get(2).map(|m| m.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        // Version backfill from <scan_root>/<prefix>/<name>/version.txt
        // first non-empty line.
        let version_path = scan_root.join(prefix).join(name).join("version.txt");
        let version = std::fs::read_to_string(&version_path).ok().and_then(|s| {
            s.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        });
        let purl_str = match &version {
            Some(v) => format!(
                "pkg:generic/{}@{}",
                encode_purl_segment(name),
                encode_purl_segment(v)
            ),
            None => format!("pkg:generic/{}", encode_purl_segment(name)),
        };
        if let Ok(purl) = Purl::new(&purl_str) {
            let mut entry = build_cmake_entry(
                name,
                version.as_deref().unwrap_or(""),
                source_path,
                purl,
                None,
                None,
                true,
                "cmake-vendored",
            );
            // FR-009: JSON boolean `true` per the milestone-009
            // `waybill:shade-relocation` precedent.
            entry.extra_annotations.insert(
                "waybill:vendored".to_string(),
                serde_json::json!(true),
            );
            out.push(entry);
        }
    }
    out
}

/// Parse a semver-ish version from an archive URL filename.
/// Same regex as bazel.rs's helper; copied here to keep modules
/// independent.
fn parse_version_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"[-_/]v?([0-9]+\.[0-9]+(?:\.[0-9]+)?)").ok()?;
    re.captures(url)?.get(1).map(|m| m.as_str().to_string())
}

#[allow(clippy::too_many_arguments)]
fn build_cmake_entry(
    name: &str,
    version: &str,
    source_path: &str,
    purl: Purl,
    download_url: Option<&str>,
    hash: Option<ContentHash>,
    _vendored: bool,
    source_mechanism: &str,
) -> PackageDbEntry {
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(url) = download_url {
        extra_annotations.insert(
            "waybill:download-url".to_string(),
            serde_json::json!(url),
        );
    }
    // C/C++ provenance: explicit `waybill:source-mechanism` annotation
    // so operators can grep/filter components by origin without
    // reverse-engineering the PURL prefix + per-reader annotations.
    // Closed enum across cmake / vcpkg / conan / bazel:
    //   cmake-fetchcontent-git, cmake-fetchcontent-url,
    //   cmake-externalproject, cmake-vendored,
    //   cmake-find-package, cmake-pkg-check-modules,  // milestone 155
    //   bazel-http-archive, vcpkg-manifest, conan-recipe.
    extra_annotations.insert(
        "waybill:source-mechanism".to_string(),
        serde_json::json!(source_mechanism),
    );
    let hashes = hash.into_iter().collect();

    PackageDbEntry {
        extra_source_paths: Vec::new(),
        depends_ecosystem: None,
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: None,
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
        hashes,
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn empty_when_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty()).is_empty());
    }

    #[test]
    fn fetchcontent_github_emits_pkg_github() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"FetchContent_Declare(googletest GIT_REPOSITORY https://github.com/google/googletest.git GIT_TAG release-1.14.0)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:github/google/googletest@release-1.14.0"
        );
    }

    #[test]
    fn fetchcontent_url_emits_pkg_generic_with_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"FetchContent_Declare(zlib URL https://zlib.net/zlib-1.3.1.tar.gz URL_HASH SHA256=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:generic/zlib@1.3.1");
        assert_eq!(entries[0].hashes.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:download-url")
                .and_then(|v| v.as_str()),
            Some("https://zlib.net/zlib-1.3.1.tar.gz")
        );
    }

    #[test]
    fn externalproject_add_url_form() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"ExternalProject_Add(boost URL https://example.com/boost_1_84_0.tar.gz)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].purl.as_str().starts_with("pkg:generic/boost@"));
    }

    #[test]
    fn find_package_emits_pkg_generic_since_milestone_155() {
        // Milestone 155 REVERSES the milestone-102 FR-007 refusal.
        // Previously (pre-155) this test asserted `entries.is_empty()`;
        // now it asserts the emission per milestone-155 FR-002 (Q1
        // clarification codified in spec.md §Clarifications 2026-07-02).
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"find_package(zlib REQUIRED)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(
            entries.len(),
            1,
            "milestone 155 REVERSES milestone-102 FR-007: find_package(X) MUST now emit exactly one component; got {entries:?}"
        );
        assert_eq!(entries[0].purl.as_str(), "pkg:generic/zlib");
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-find-package")
        );
    }

    #[test]
    fn vendored_emits_only_when_include_vendored_set() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"add_subdirectory(third_party/foo)"#,
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("third_party/foo")).unwrap();
        std::fs::write(
            tmp.path().join("third_party/foo/version.txt"),
            "1.2.3",
        )
        .unwrap();

        // Default off: no emission.
        assert!(read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty()).is_empty());

        // With include_vendored = true: 1 component with vendored annotation.
        let entries = read(tmp.path(), true, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:generic/foo@1.2.3");
        assert_eq!(
            entries[0].extra_annotations.get("waybill:vendored"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn vendored_path_prefix_gate_rejects_first_party() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"add_subdirectory(src)
add_subdirectory(tests)"#,
        )
        .unwrap();
        // Even with include_vendored=true, first-party `src/`/`tests/`
        // sub-modules MUST NOT emit per FR-008's path-prefix gate.
        let entries = read(tmp.path(), true, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert!(
            entries.is_empty(),
            "first-party add_subdirectory(src) MUST NOT emit; got {entries:?}"
        );
    }

    // --- C/C++ provenance: source-mechanism annotation ---------------------

    #[test]
    fn source_mechanism_annotation_fetchcontent_git() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"FetchContent_Declare(googletest GIT_REPOSITORY https://github.com/google/googletest.git GIT_TAG release-1.14.0)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-fetchcontent-git"),
            "FetchContent_Declare GIT form should be `cmake-fetchcontent-git`; got: {:?}",
            entries[0].extra_annotations.get("waybill:source-mechanism"),
        );
    }

    #[test]
    fn source_mechanism_annotation_fetchcontent_url() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"FetchContent_Declare(zlib URL https://zlib.net/zlib-1.3.1.tar.gz URL_HASH SHA256=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-fetchcontent-url"),
        );
    }

    #[test]
    fn source_mechanism_annotation_externalproject() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"ExternalProject_Add(boost URL https://example.com/boost_1_84_0.tar.gz)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-externalproject"),
        );
    }

    #[test]
    fn source_mechanism_annotation_vendored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"add_subdirectory(third_party/foo)"#,
        )
        .unwrap();
        // Vendored dir needs a version source — use third_party/foo/version.txt
        std::fs::create_dir_all(tmp.path().join("third_party/foo")).unwrap();
        std::fs::write(tmp.path().join("third_party/foo/version.txt"), "1.2.3").unwrap();
        let entries = read(tmp.path(), true, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("waybill:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-vendored"),
        );
    }

    // ----------------------------------------------------------------
    // Milestone 105 phase 2B — find_package + add_library ALIAS
    // collector (FR-008a). Component emission MUST stay disabled
    // (preserves milestone 102's FR-007); the collector only
    // populates a name set consumed by the git_submodule reader.
    // ----------------------------------------------------------------

    #[test]
    fn collect_find_package_basic() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"
                find_package(zlib REQUIRED)
                find_package(OpenSSL 1.1 REQUIRED COMPONENTS SSL Crypto)
                find_package(Boost CONFIG)
            "#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.contains("zlib"), "got: {names:?}");
        assert!(names.contains("openssl"), "got: {names:?}");
        assert!(names.contains("boost"), "got: {names:?}");
    }

    #[test]
    fn collect_find_package_case_folded() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"find_package(ZLib REQUIRED)"#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        // Per FR-008a, names are case-folded for case-insensitive
        // matching against submodule path basenames.
        assert!(names.contains("zlib"), "got: {names:?}");
        assert!(!names.contains("ZLib"), "should be case-folded; got: {names:?}");
    }

    #[test]
    fn collect_add_library_alias_namespaced() {
        // `add_library(SomeLib::SomeLib ALIAS someimpl)` should
        // contribute BOTH `somelib` (the namespace prefix) AND
        // `someimpl` (the underlying target) to the set so a
        // submodule named `third_party/someimpl/` matches.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"add_library(SomeLib::SomeLib ALIAS someimpl)"#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.contains("somelib"), "namespace; got: {names:?}");
        assert!(names.contains("someimpl"), "alias target; got: {names:?}");
    }

    #[test]
    fn collect_add_library_alias_unnamespaced() {
        // Plain form: `add_library(Foo ALIAS foo)` — both `foo`
        // (from the alias name case-folded) and `foo` (the target)
        // collapse to one entry.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"add_library(Foo ALIAS foo)"#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.contains("foo"), "got: {names:?}");
    }

    #[test]
    fn collect_combined_find_package_and_alias() {
        // Realistic gRPC-like case: find_package(SomeLib) + an
        // add_library alias that maps SomeLib to a differently-named
        // implementation target. Both names go into the set so a
        // submodule named either way is recognized as declared-and-used.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"
                find_package(SomeLib REQUIRED CONFIG)
                add_library(SomeLib::SomeLib ALIAS someimpl)
            "#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.contains("somelib"), "got: {names:?}");
        assert!(names.contains("someimpl"), "got: {names:?}");
    }

    #[test]
    fn collect_returns_empty_when_no_cmake() {
        // No CMakeLists.txt → empty set, no panic.
        let tmp = tempfile::tempdir().unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.is_empty(), "got: {names:?}");
    }

    #[test]
    fn collect_does_not_emit_components_invariant() {
        // FR-008a guarantee: the collector populates a name set. It is
        // orthogonal to milestone-155's `PackageDbEntry` emission — the
        // collector operates on a DIFFERENT data pipeline (milestone-105
        // US6 `git-submodule` classification) and does not interact
        // with `read()`'s emitted entries. Post-milestone-155, `read()`
        // DOES emit for `find_package` (previously refused per
        // milestone-102 FR-007, now reversed); this test locks in that
        // the collector's name set is populated INDEPENDENTLY of the
        // emit path — both should surface `find_package(zlib)` per
        // their respective conventions.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"
                find_package(zlib REQUIRED)
                add_library(MyAlias::MyAlias ALIAS myimpl)
            "#,
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let names = collect_find_package_targets(tmp.path());
        // Milestone 155 REVERSAL: `read()` now emits for find_package.
        let cmake_fp: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.extra_annotations
                    .get("waybill:source-mechanism")
                    .and_then(|v| v.as_str())
                    == Some("cmake-find-package")
            })
            .collect();
        assert_eq!(
            cmake_fp.len(),
            1,
            "milestone 155: read() MUST emit exactly one cmake-find-package entry for `find_package(zlib REQUIRED)`"
        );
        assert!(!names.is_empty(), "collector MUST populate the set");
    }

    // ============================================================
    // Milestone 155 unit tests (SC-006 floor ≥8; total = 14 tests)
    // ============================================================

    /// Helper: filter entries by source-mechanism annotation value.
    fn by_mechanism<'a>(
        entries: &'a [PackageDbEntry],
        mechanism: &str,
    ) -> Vec<&'a PackageDbEntry> {
        entries
            .iter()
            .filter(|e| {
                e.extra_annotations
                    .get("waybill:source-mechanism")
                    .and_then(|v| v.as_str())
                    == Some(mechanism)
            })
            .collect()
    }

    // ---- T004: 10 core unit tests per research §R6 ----

    #[test]
    fn find_package_simple_no_version_emits_pkg_generic() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(Foo REQUIRED)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/foo");
        assert_eq!(
            fp[0].extra_annotations
                .get("waybill:cmake-find-package-name")
                .and_then(|v| v.as_str()),
            Some("Foo")
        );
    }

    #[test]
    fn find_package_with_version_is_a_minimum_range() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(OpenSSL 1.1.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/openssl");
        assert_eq!(fp[0].requirement_ranges, [">=1.1.0"]);
        assert_eq!(
            fp[0].extra_annotations
                .get("waybill:cmake-find-package-name")
                .and_then(|v| v.as_str()),
            Some("OpenSSL")
        );
    }

    #[test]
    fn find_package_case_normalization() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(BOOST 1.75.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/boost");
        assert_eq!(fp[0].requirement_ranges, [">=1.75.0"]);
        assert_eq!(
            fp[0].extra_annotations
                .get("waybill:cmake-find-package-name")
                .and_then(|v| v.as_str()),
            Some("BOOST")
        );
    }

    #[test]
    fn find_package_multiple_versions_record_every_range() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("cmake")).unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(openssl 1.1.0)\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("cmake").join("defs.cmake"),
            "find_package(openssl 3.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        // Two emissions (one per site), with the same versionless PURL
        // so the milestone-148 union merges them; each carries both
        // sites' ranges (#1520).
        assert_eq!(fp.len(), 2);
        for e in &fp {
            assert_eq!(e.purl.as_str(), "pkg:generic/openssl");
            assert_eq!(e.requirement_ranges, [">=1.1.0", ">=3.0"]);
        }
    }

    #[test]
    fn find_package_mixed_version_and_no_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("cmake")).unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(openssl 1.1.0)\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("cmake").join("defs.cmake"),
            "find_package(openssl REQUIRED)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 2);
        for e in &fp {
            assert_eq!(e.purl.as_str(), "pkg:generic/openssl");
            assert_eq!(e.requirement_ranges, [">=1.1.0"], "the version-less site adds no range");
        }
    }

    #[test]
    fn find_package_handle_standard_args_not_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package_handle_standard_args(Foo DEFAULT_MSG FOO_LIBRARY FOO_INCLUDE_DIR)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(
            fp.len(),
            0,
            "FR-009: find_package_handle_standard_args MUST NOT emit"
        );
    }

    #[test]
    fn find_package_variable_interpolation_not_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(${MY_LIB})\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 0, "FR-010: variable-interpolated find_package MUST NOT emit");
    }

    #[test]
    fn find_package_commented_out_not_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "# find_package(SomeUnusedDep)\n    # find_package(AnotherUnusedDep 1.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 0, "FR-011: commented-out find_package MUST NOT emit");
    }

    #[test]
    fn pkg_check_modules_single_module() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "pkg_check_modules(RADIUS REQUIRED IMPORTED_TARGET radcli)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let pcm = by_mechanism(&entries, "cmake-pkg-check-modules");
        assert_eq!(pcm.len(), 1);
        assert_eq!(pcm[0].purl.as_str(), "pkg:generic/radcli");
        assert!(
            !pcm[0]
                .extra_annotations
                .contains_key("waybill:cmake-find-package-name"),
            "pkg_check_modules emissions MUST NOT carry cmake-find-package-name"
        );
    }

    #[test]
    fn pkg_check_modules_multi_module_with_version_constraints() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "pkg_check_modules(GLIB REQUIRED glib-2.0>=2.42 gio-2.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let pcm = by_mechanism(&entries, "cmake-pkg-check-modules");
        assert_eq!(pcm.len(), 2);
        let purls: std::collections::BTreeSet<_> =
            pcm.iter().map(|e| e.purl.as_str().to_string()).collect();
        assert!(purls.contains("pkg:generic/glib-2.0"));
        assert!(purls.contains("pkg:generic/gio-2.0"));
    }

    // ---- T005: 4 supplementary tests ----

    #[test]
    fn find_package_targets_collector_unaffected() {
        // R9 regression guard: collect_find_package_targets remains
        // orthogonal to milestone-155 emission behavior. Its return
        // set is independent of read()'s emitted entries.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"
                find_package(Foo)
                add_library(Foo::Foo ALIAS foo)
            "#,
        )
        .unwrap();
        let names = collect_find_package_targets(tmp.path());
        assert!(names.contains("foo"), "collector MUST contain foo (lowercased); got: {names:?}");
    }

    #[test]
    fn find_package_all_lowercase_no_annotation() {
        // Contracts/waybill-cmake-find-package-name.md conditional
        // emission: when the original casing equals the lowercased
        // name, the annotation MUST NOT be emitted.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(zlib 1.2.11)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/zlib");
        assert_eq!(fp[0].requirement_ranges, [">=1.2.11"]);
        assert!(
            !fp[0]
                .extra_annotations
                .contains_key("waybill:cmake-find-package-name"),
            "all-lowercase input MUST NOT emit waybill:cmake-find-package-name"
        );
    }

    #[test]
    fn find_package_pkg_search_module_alias() {
        // FR-004: pkg_search_module is the sibling of pkg_check_modules.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "pkg_search_module(ZLIB REQUIRED zlib)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let pcm = by_mechanism(&entries, "cmake-pkg-check-modules");
        assert_eq!(pcm.len(), 1);
        assert_eq!(pcm[0].purl.as_str(), "pkg:generic/zlib");
    }

    #[test]
    fn find_package_modifier_keywords_ignored() {
        // F4 remediation: assert that modifier keywords (NO_MODULE,
        // COMPONENTS, PATHS, etc.) don't contaminate the emitted PURL.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(Boost 1.75.0 NO_MODULE COMPONENTS system filesystem thread PATHS /usr/local/lib)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1, "single component per parent package; NO sub-component emission");
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/boost");
        assert_eq!(fp[0].requirement_ranges, [">=1.75.0"]);
    }

    // ============================================================
    // Milestone 156 unit tests (SC-008 floor ≥6). All 6 test fns
    // named with the `discover_cmake_files_` prefix so the SC-008
    // grep count command
    //   grep -cE "^\s+fn discover_cmake_files_" waybill-cli/src/scan_fs/package_db/cmake.rs
    // returns ≥6 (F2 remediation from /speckit-analyze 2026-07-02).
    // ============================================================

    /// Serial guard for env-var mutation tests. cargo runs tests
    /// concurrently by default; `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE`
    /// is process-global, so tests #3 + #4 (default vs opt-in
    /// behavior) MUST NOT run concurrently.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn empty_exclude_set() -> crate::scan_fs::package_db::exclude_path::ExclusionSet {
        crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty()
    }

    #[test]
    fn discover_cmake_files_walks_cmake_recursively() {
        // FR-001: recursive descent under cmake/. Fixture has a
        // depth-2 `cmake/modules/FindFoo.cmake` file.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("cmake").join("modules")).unwrap();
        std::fs::write(
            tmp.path().join("cmake").join("modules").join("FindFoo.cmake"),
            "# depth-2 file\n",
        )
        .unwrap();
        let files = discover_cmake_files(tmp.path(), false, &empty_exclude_set());
        assert!(
            files.iter().any(|p| p.ends_with("FindFoo.cmake")),
            "milestone 156 FR-001: cmake/modules/FindFoo.cmake at depth-2 MUST be discovered; got {files:?}"
        );
    }

    #[test]
    fn discover_cmake_files_walks_modules_recursively() {
        // FR-001: recursive descent under Modules/.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("Modules").join("utils")).unwrap();
        std::fs::write(
            tmp.path().join("Modules").join("utils").join("Extra.cmake"),
            "# depth-2 file\n",
        )
        .unwrap();
        let files = discover_cmake_files(tmp.path(), false, &empty_exclude_set());
        assert!(
            files.iter().any(|p| p.ends_with("Extra.cmake")),
            "milestone 156 FR-001: Modules/utils/Extra.cmake at depth-2 MUST be discovered; got {files:?}"
        );
    }

    #[test]
    fn discover_cmake_files_depth1_third_party_by_default() {
        // FR-019: without --cmake-third-party-recursive, third_party/
        // stays at depth-1 (matches milestone-102 behavior).
        let _guard = ENV_MUTEX.lock().unwrap();
        // SAFETY: guarded by ENV_MUTEX above; single-threaded across
        // this test and the opt-in variant below.
        unsafe { std::env::remove_var("WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE") };
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("third_party").join("subdir")).unwrap();
        std::fs::write(
            tmp.path().join("third_party").join("depth1.cmake"),
            "# depth-1 file\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("third_party").join("subdir").join("depth2.cmake"),
            "# depth-2 file\n",
        )
        .unwrap();
        let files = discover_cmake_files(tmp.path(), false, &empty_exclude_set());
        assert!(
            files.iter().any(|p| p.ends_with("depth1.cmake")),
            "third_party/depth1.cmake at depth-1 MUST be discovered by default; got {files:?}"
        );
        assert!(
            !files.iter().any(|p| p.ends_with("depth2.cmake")),
            "milestone 156 FR-019 default: third_party/subdir/depth2.cmake at depth-2 MUST NOT be discovered without the flag; got {files:?}"
        );
    }

    #[test]
    fn discover_cmake_files_recursive_third_party_when_opt_in() {
        // FR-019 opt-in: passing include_third_party_recursive=true
        // extends the recursive descent to third_party/ too.
        let _guard = ENV_MUTEX.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("third_party").join("subdir")).unwrap();
        std::fs::write(
            tmp.path().join("third_party").join("depth1.cmake"),
            "# depth-1 file\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("third_party").join("subdir").join("depth2.cmake"),
            "# depth-2 file\n",
        )
        .unwrap();
        let files = discover_cmake_files(tmp.path(), true, &empty_exclude_set());
        assert!(
            files.iter().any(|p| p.ends_with("depth1.cmake")),
            "third_party/depth1.cmake at depth-1 MUST be discovered when flag is set; got {files:?}"
        );
        assert!(
            files.iter().any(|p| p.ends_with("depth2.cmake")),
            "milestone 156 FR-019 opt-in: third_party/subdir/depth2.cmake at depth-2 MUST be discovered when include_third_party_recursive=true; got {files:?}"
        );
    }

    #[test]
    fn discover_cmake_files_respects_exclude_set() {
        // FR-005: --exclude-path integration. A file under an excluded
        // subdirectory is not walked.
        use crate::scan_fs::package_db::exclude_path::ExclusionSet;
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("cmake").join("modules")).unwrap();
        std::fs::write(
            tmp.path().join("cmake").join("modules").join("FindFoo.cmake"),
            "# excluded\n",
        )
        .unwrap();
        let excludes = ExclusionSet::from_iter(["cmake/modules"]).unwrap();
        let files = discover_cmake_files(tmp.path(), false, &excludes);
        assert!(
            !files.iter().any(|p| p.ends_with("FindFoo.cmake")),
            "milestone 156 FR-005: excluded cmake/modules/ MUST NOT contribute FindFoo.cmake; got {files:?}"
        );
    }

    #[test]
    fn discover_cmake_files_emits_find_package_at_depth2() {
        // End-to-end via read(): fixture at cmake/modules/FindLibev.cmake
        // (depth-2) contains find_package(Libev 1.4.0). Assert emission
        // via the full milestone-155 pipeline.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("cmake").join("modules")).unwrap();
        std::fs::write(
            tmp.path().join("cmake").join("modules").join("FindLibev.cmake"),
            "find_package(Libev 1.4.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &empty_exclude_set());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/libev");
        assert_eq!(fp[0].requirement_ranges, [">=1.4.0"]);
        assert!(
            fp[0]
                .source_path
                .contains("cmake/modules/FindLibev.cmake")
                || fp[0].source_path.contains("cmake\\modules\\FindLibev.cmake"),
            "source path should point at the depth-2 file; got {}",
            fp[0].source_path
        );
    }

    fn s1517_read(cmake: &str) -> Vec<PackageDbEntry> {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("CMakeLists.txt"), cmake).unwrap();
        let mut out = read(
            tmp.path(),
            false,
            &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty(),
        );
        out.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
        out
    }

    fn s1517_hashes(e: &PackageDbEntry) -> Vec<String> {
        e.hashes.iter().map(|h| format!("{}:{}", h.algorithm, h.value.as_str())).collect()
    }

    /// #1517: the issue's repro, all four forms.
    #[test]
    fn s1517_quoted_and_sha512_hashes_and_set_variables() {
        let h1 = "1".repeat(64);
        let h2 = "2".repeat(64);
        let h3 = "3".repeat(128);
        let h4 = "4".repeat(64);
        let out = s1517_read(&format!(
            "cmake_minimum_required(VERSION 3.25)\nproject(p)\ninclude(FetchContent)\n\
             FetchContent_Declare(waybill-fixture-plain\n  URL https://example.invalid/waybill-fixture-plain-1.2.3.tar.gz\n  URL_HASH SHA256={h1})\n\
             FetchContent_Declare(waybill-fixture-quoted\n  URL https://example.invalid/waybill-fixture-quoted-2.0.0.tar.gz\n  URL_HASH \"SHA256={h2}\")\n\
             FetchContent_Declare(waybill-fixture-sha512\n  URL https://example.invalid/waybill-fixture-sha512-3.0.0.tar.gz\n  URL_HASH SHA512={h3})\n\
             set(FX_VERSION 4.5.6)\n\
             FetchContent_Declare(waybill-fixture-var\n  URL https://example.invalid/waybill-fixture-var-${{FX_VERSION}}.tar.gz\n  URL_HASH SHA256={h4})\n"
        ));
        let got: Vec<(String, Vec<String>)> =
            out.iter().map(|e| (e.purl.as_str().to_string(), s1517_hashes(e))).collect();
        assert_eq!(
            got,
            vec![
                ("pkg:generic/waybill-fixture-plain@1.2.3".to_string(), vec![format!("sha256:{h1}")]),
                ("pkg:generic/waybill-fixture-quoted@2.0.0".to_string(), vec![format!("sha256:{h2}")]),
                ("pkg:generic/waybill-fixture-sha512@3.0.0".to_string(), vec![format!("sha512:{h3}")]),
                ("pkg:generic/waybill-fixture-var@4.5.6".to_string(), vec![format!("sha256:{h4}")]),
            ]
        );
        let var = &out[3];
        assert_eq!(
            var.extra_annotations.get("waybill:download-url").and_then(|v| v.as_str()),
            Some("https://example.invalid/waybill-fixture-var-4.5.6.tar.gz"),
        );
        assert_eq!(var.sbom_tier.as_deref(), Some("source"));
    }

    /// #1517: quoted `set()` values, a variable built from another, a
    /// variable hash, and a variable `GIT_TAG`.
    #[test]
    fn s1517_nested_quoted_variables_and_git_tag() {
        let h = "a".repeat(64);
        let out = s1517_read(&format!(
            "set(FX_BASE \"https://example.invalid/dl\")\nset(FX_VERSION 7.8.9)\n\
             set(FX_FILE waybill-fixture-nested-${{FX_VERSION}}.tar.gz)\nset(FX_SHA {h})\nset(FX_TAG \"v1.2.3\")\n\
             FetchContent_Declare(waybill-fixture-nested\n  URL \"${{FX_BASE}}/${{FX_FILE}}\"\n  URL_HASH SHA256=${{FX_SHA}})\n\
             FetchContent_Declare(waybill-fixture-git\n  GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-git.git\n  GIT_TAG ${{FX_TAG}})\n"
        ));
        let purls: Vec<&str> = out.iter().map(|e| e.purl.as_str()).collect();
        assert_eq!(
            purls,
            ["pkg:generic/waybill-fixture-nested@7.8.9", "pkg:github/waybill-org/waybill-fixture-git@v1.2.3"]
        );
        assert_eq!(s1517_hashes(&out[0]), vec![format!("sha256:{h}")]);
        assert_eq!(
            out[0].extra_annotations.get("waybill:download-url").and_then(|v| v.as_str()),
            Some("https://example.invalid/dl/waybill-fixture-nested-7.8.9.tar.gz"),
        );
    }

    /// #1517: a variable with no literal `set()` in the same file is
    /// reported as unresolved (C151), never as `@unknown`.
    #[test]
    fn s1517_unresolved_variable_is_design_tier_with_reason() {
        let out = s1517_read(
            "FetchContent_Declare(waybill-fixture-unset\n  URL https://example.invalid/waybill-fixture-unset-${FX_UNSET}.tar.gz)\n",
        );
        assert_eq!(out.len(), 1, "{out:?}");
        let e = &out[0];
        assert_eq!(e.purl.as_str(), "pkg:generic/waybill-fixture-unset");
        assert_eq!(e.version, "");
        assert_eq!(e.sbom_tier.as_deref(), Some("design"));
        assert_eq!(
            e.extra_annotations.get("waybill:unresolved-reason").and_then(|v| v.as_str()),
            Some(CMAKE_UNRESOLVED_VARIABLE_REASON),
        );
        assert!(!e.extra_annotations.contains_key("waybill:download-url"));
    }

    /// #1517: a variable set twice to different values depends on control
    /// flow the reader does not evaluate, so it stays unresolved; set twice
    /// to the same value, it resolves.
    #[test]
    fn s1517_variable_set_twice_resolves_only_when_both_agree() {
        let out = s1517_read(
            "if(WIN32)\n  set(FX_V 1.0.0)\nelse()\n  set(FX_V 2.0.0)\nendif()\n\
             set(FX_SAME 3.0.0)\nset(FX_SAME 3.0.0)\n\
             FetchContent_Declare(waybill-fixture-branchy URL https://example.invalid/waybill-fixture-branchy-${FX_V}.tar.gz)\n\
             FetchContent_Declare(waybill-fixture-same URL https://example.invalid/waybill-fixture-same-${FX_SAME}.tar.gz)\n",
        );
        let purls: Vec<&str> = out.iter().map(|e| e.purl.as_str()).collect();
        assert_eq!(purls, ["pkg:generic/waybill-fixture-branchy", "pkg:generic/waybill-fixture-same@3.0.0"]);
        assert_eq!(out[0].sbom_tier.as_deref(), Some("design"));
    }

    /// #1520: the issue's repro, plus both CMake version-range forms.
    #[test]
    fn s1520_find_package_versions_are_requirements_not_resolutions() {
        let out = s1517_read(
            "cmake_minimum_required(VERSION 3.25)\nproject(p)\n\
             find_package(WaybillFixtureLib 3.9 REQUIRED)\n\
             find_package(WaybillFixtureExact 2.1 EXACT REQUIRED)\n\
             find_package(WaybillFixtureRange 3.9...4.0)\n\
             find_package(WaybillFixtureHalfOpen 3.9...<4.0 CONFIG)\n\
             find_package(WaybillFixtureBare REQUIRED)\n",
        );
        let got: Vec<(&str, Vec<String>, Option<&str>)> = out
            .iter()
            .map(|e| (e.purl.as_str(), e.requirement_ranges.clone(), e.sbom_tier.as_deref()))
            .collect();
        let design = Some("design");
        assert_eq!(
            got,
            vec![
                ("pkg:generic/waybillfixturebare", vec![], design),
                ("pkg:generic/waybillfixtureexact", vec!["==2.1".to_string()], design),
                ("pkg:generic/waybillfixturehalfopen", vec![">=3.9, <4.0".to_string()], design),
                ("pkg:generic/waybillfixturelib", vec![">=3.9".to_string()], design),
                ("pkg:generic/waybillfixturerange", vec![">=3.9, <=4.0".to_string()], design),
            ]
        );
        for e in &out {
            assert_eq!(e.version, "");
            assert_eq!(
                e.extra_annotations.get("waybill:unresolved-reason").and_then(|v| v.as_str()),
                Some(CMAKE_FIND_PACKAGE_REASON),
            );
        }
    }

    /// Spec 1530 FR-015: a mixed-case GitHub repository is lowercased, as
    /// purl-spec's `github` definition requires.
    #[test]
    fn s1530_fetchcontent_github_purl_is_lowercased() {
        let out = s1517_read(
            "FetchContent_Declare(waybill-fixture-mixed\n  GIT_REPOSITORY https://github.com/ROCm/Waybill-Fixture-Mixed.git\n  GIT_TAG v1.0.0)\n",
        );
        let purls: Vec<&str> = out.iter().map(|e| e.purl.as_str()).collect();
        assert_eq!(purls, ["pkg:github/rocm/waybill-fixture-mixed@v1.0.0"]);
    }
}
