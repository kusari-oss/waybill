//! CMake source-tree reader (milestone 102 US2 / milestone 103 impl;
//! extended by milestone 155 to parse `find_package` +
//! `pkg_check_modules` — reversing the milestone-102 FR-007 refusal).
//!
//! Parses `CMakeLists.txt` + included `.cmake` files for:
//! - `FetchContent_Declare(<name> GIT_REPOSITORY ... GIT_TAG ...)` — emits
//!   `pkg:github/<owner>/<repo>@<tag>` for GitHub-hosted URLs, otherwise
//!   `pkg:generic/<name>@<tag>` with `mikebom:download-url`.
//! - `FetchContent_Declare(<name> URL ... URL_HASH SHA256=...)` and
//!   `ExternalProject_Add(<name> URL ...)` — emits `pkg:generic/<name>@<version>`
//!   with URL + SHA-256.
//! - `add_subdirectory(third_party|vendor/<name>)` — opt-in via the
//!   `include_vendored` parameter (wired from PR-A's CLI flag); emits
//!   `pkg:generic/<name>@<version-from-version.txt>` with the JSON
//!   boolean `mikebom:vendored = true` annotation.
//! - **milestone 155**: `find_package(<Name> [<Version>])` — emits
//!   `pkg:generic/<lowercased-name>[@<highest-declared-version>]` with
//!   `mikebom:source-mechanism = "cmake-find-package"`. Multi-file
//!   same-name declarations are consolidated to the highest declared
//!   version (Q1 clarification). Original casing preserved in the
//!   `mikebom:cmake-find-package-name` annotation when it differs from
//!   the lowercased PURL. Same-PURL cross-mechanism double-counting is
//!   prevented by the production `resolve::deduplicator` pass.
//! - **milestone 155**: `pkg_check_modules(<TARGET> <modules>)` +
//!   `pkg_search_module(<TARGET> <modules>)` — emits one
//!   `pkg:generic/<module>` per module (target variable discarded,
//!   version constraints stripped) with
//!   `mikebom:source-mechanism = "cmake-pkg-check-modules"`.
//!
//! Walks the scan root for `CMakeLists.txt` at depth-0. Under
//! `cmake/` and `Modules/`, recursive descent captures every
//! `.cmake` file + `CMakeLists.txt` at any depth (milestone 156 —
//! reaches nested `Find*.cmake` files like Kamailio's
//! `cmake/modules/Find*.cmake`). Under `third_party/`, depth-1 walk
//! by default (matches milestone-102 behavior); opt in to recursive
//! descent via `--cmake-third-party-recursive` (env alias
//! `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`) to also walk vendored
//! deps' own `find_package` declarations.
//!
//! Cross-platform; no `#[cfg(unix)]` gates. Zero new Cargo deps —
//! uses workspace `regex` + std.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

/// Milestone 155 — one hit per successfully matched `find_package(<Name>
/// [<Version>])` call site. Accumulated across all discovered CMake
/// files by `read()`, then consumed by `emit_find_package_entries`.
struct FindPackageHit {
    lowercased_name: String,
    original_casing: String,
    declared_version: Option<String>,
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

/// Milestone 155 — pick the highest declared version across a group of
/// same-name `find_package` sites (Q1 clarification: highest declared
/// version wins). Component-wise numeric SemVer with zero-padding when
/// every part parses as `u64`; otherwise lexicographic with a
/// `tracing::warn` diagnostic. Returns `None` when all inputs are None.
fn pick_highest_version(versions: &[Option<String>]) -> Option<String> {
    let some_versions: Vec<&String> = versions.iter().filter_map(|v| v.as_ref()).collect();
    if some_versions.is_empty() {
        return None;
    }
    if some_versions.len() == 1 {
        return Some(some_versions[0].clone());
    }
    // Check if EVERY segment of EVERY version parses as u64.
    let parsed: Option<Vec<Vec<u64>>> = some_versions
        .iter()
        .map(|v| {
            v.split('.')
                .map(|seg| seg.parse::<u64>().ok())
                .collect::<Option<Vec<u64>>>()
        })
        .collect();
    if let Some(mut all_numeric) = parsed {
        // Zero-pad to the same length for component-wise comparison.
        let max_len = all_numeric.iter().map(|v| v.len()).max().unwrap_or(0);
        for v in all_numeric.iter_mut() {
            v.resize(max_len, 0);
        }
        // Find the index of the largest.
        let (best_idx, _) = all_numeric
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.cmp(b.1))
            .expect("some_versions non-empty");
        return Some(some_versions[best_idx].clone());
    }
    // Fallback: lexicographic with a warn diagnostic.
    tracing::warn!(
        versions = ?some_versions,
        "milestone-155: mixed-format version strings; lexicographic ordering used — highest may not be semantically correct"
    );
    some_versions
        .iter()
        .max()
        .map(|s| (*s).clone())
}

pub fn read(
    scan_root: &Path,
    include_vendored: bool,
    exclude_set: &super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    // Milestone 156: opt-in recursive descent for third_party/. Default
    // depth-1 (matches milestone-102 behavior) unless the CLI flag
    // `--cmake-third-party-recursive` OR the env var
    // `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` is set. Mirrors the
    // milestone-102 MIKEBOM_INCLUDE_VENDORED env-var propagation
    // pattern at read_all:1193 to avoid plumbing a new bool through
    // the 75-callsite scan_path -> read_all chain.
    let include_third_party_recursive = std::env::var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE")
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
            r"(?im)^[^#\n]*?\bfind_package\s*\(\s*([A-Za-z0-9_:.+-]+)(?:\s+([0-9][A-Za-z0-9._-]*))?",
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
        let version = cap.get(2).map(|m| m.as_str().to_string());
        let lowercased = name_raw.to_lowercase();
        out.push(FindPackageHit {
            lowercased_name: lowercased,
            original_casing: name_raw.to_string(),
            declared_version: version,
            source_path: source_path.to_string(),
        });
    }
    out
}

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
/// hits, applying the Q1 highest-declared-version-wins rule per group
/// of same-lowercased-name hits. Emits ONE entry per input hit (with
/// the group's chosen winning version) so downstream milestone-148
/// source-file-paths union works naturally.
fn emit_find_package_entries(hits: Vec<FindPackageHit>) -> Vec<PackageDbEntry> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<FindPackageHit>> = BTreeMap::new();
    for hit in hits {
        groups.entry(hit.lowercased_name.clone()).or_default().push(hit);
    }
    let mut out = Vec::new();
    for (lowercased_name, group) in groups {
        let versions: Vec<Option<String>> =
            group.iter().map(|h| h.declared_version.clone()).collect();
        let winner_version = pick_highest_version(&versions);
        let purl_str = match &winner_version {
            Some(v) => format!(
                "pkg:generic/{}@{}",
                encode_purl_segment(&lowercased_name),
                encode_purl_segment(v)
            ),
            None => format!("pkg:generic/{}", encode_purl_segment(&lowercased_name)),
        };
        for hit in group {
            let purl = match Purl::new(&purl_str) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let mut entry = build_cmake_entry(
                &lowercased_name,
                winner_version.as_deref().unwrap_or(""),
                &hit.source_path,
                purl,
                None,
                None,
                false,
                "cmake-find-package",
            );
            // FR-008: preserve original casing when it differs from the
            // lowercased PURL name — parity-bridging annotation.
            if hit.original_casing != hit.lowercased_name {
                entry.extra_annotations.insert(
                    "mikebom:cmake-find-package-name".to_string(),
                    serde_json::json!(hit.original_casing),
                );
            }
            // Preserve per-site version forensics when the site declared
            // a different version than the group's chosen winner.
            if let Some(site_v) = hit.declared_version.as_deref() {
                if winner_version.as_deref() != Some(site_v) {
                    entry.raw_version = Some(site_v.to_string());
                }
            }
            // R10 correction: leave evidence_kind = None (matches
            // existing cmake.rs FetchContent/ExternalProject/vendored
            // precedent). The canonical evidence-kind enum in
            // cyclonedx/builder.rs does not admit a "declared" value
            // and rejects entries carrying non-enum strings. The
            // manifest-declared semantic is already carried by
            // sbom_tier = "source" set inside build_cmake_entry.
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
/// entry as `mikebom:build-reference: "declared-and-used"` (the
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
///   `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1` env var).
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
    let mut out = Vec::new();
    for c in outer.captures_iter(content) {
        let name = c.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        let body = c.get(2).map(|m| m.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let git_repo = extract_keyword(body, "GIT_REPOSITORY");
        let git_tag = extract_keyword(body, "GIT_TAG");
        let url = extract_keyword(body, "URL");
        let url_hash_sha256 = extract_url_hash_sha256(body);

        let (purl_str, download_url, version) = if let (Some(g), Some(t)) =
            (git_repo.as_deref(), git_tag.as_deref())
        {
            // GIT form — check for GitHub URL → pkg:github/<owner>/<repo>@<tag>;
            // otherwise pkg:generic/<name>@<tag>.
            let github_pair = github_re
                .as_ref()
                .and_then(|r| r.captures(g))
                .and_then(|m| {
                    let owner = m.get(1)?.as_str();
                    let repo = m.get(2)?.as_str();
                    Some((owner.to_string(), repo.to_string()))
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
                url_hash_sha256.as_deref(),
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
    re.captures(body)?.get(1).map(|m| m.as_str().to_string())
}

/// Extract `URL_HASH SHA256=<hex>` — CMake's compound-keyword form.
fn extract_url_hash_sha256(body: &str) -> Option<String> {
    let re = Regex::new(r"URL_HASH\s+SHA256\s*=\s*([0-9a-fA-F]+)").ok()?;
    re.captures(body)?.get(1).map(|m| m.as_str().to_string())
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
            // `mikebom:shade-relocation` precedent.
            entry.extra_annotations.insert(
                "mikebom:vendored".to_string(),
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
    sha256_hex: Option<&str>,
    _vendored: bool,
    source_mechanism: &str,
) -> PackageDbEntry {
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(url) = download_url {
        extra_annotations.insert(
            "mikebom:download-url".to_string(),
            serde_json::json!(url),
        );
    }
    // C/C++ provenance: explicit `mikebom:source-mechanism` annotation
    // so operators can grep/filter components by origin without
    // reverse-engineering the PURL prefix + per-reader annotations.
    // Closed enum across cmake / vcpkg / conan / bazel:
    //   cmake-fetchcontent-git, cmake-fetchcontent-url,
    //   cmake-externalproject, cmake-vendored,
    //   cmake-find-package, cmake-pkg-check-modules,  // milestone 155
    //   bazel-http-archive, vcpkg-manifest, conan-recipe.
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::json!(source_mechanism),
    );
    let hashes = sha256_hex
        .and_then(|hex| ContentHash::sha256(hex).ok())
        .map(|h| vec![h])
        .unwrap_or_default();

    PackageDbEntry {
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
                .get("mikebom:download-url")
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
                .get("mikebom:source-mechanism")
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
            entries[0].extra_annotations.get("mikebom:vendored"),
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
                .get("mikebom:source-mechanism")
                .and_then(|v| v.as_str()),
            Some("cmake-fetchcontent-git"),
            "FetchContent_Declare GIT form should be `cmake-fetchcontent-git`; got: {:?}",
            entries[0].extra_annotations.get("mikebom:source-mechanism"),
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
                .get("mikebom:source-mechanism")
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
                .get("mikebom:source-mechanism")
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
                .get("mikebom:source-mechanism")
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
                    .get("mikebom:source-mechanism")
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
                    .get("mikebom:source-mechanism")
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
                .get("mikebom:cmake-find-package-name")
                .and_then(|v| v.as_str()),
            Some("Foo")
        );
    }

    #[test]
    fn find_package_with_version_emits_at_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            "find_package(OpenSSL 1.1.0)\n",
        )
        .unwrap();
        let entries = read(tmp.path(), false, &crate::scan_fs::package_db::exclude_path::ExclusionSet::new_empty());
        let fp = by_mechanism(&entries, "cmake-find-package");
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/openssl@1.1.0");
        assert_eq!(
            fp[0].extra_annotations
                .get("mikebom:cmake-find-package-name")
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
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/boost@1.75.0");
        assert_eq!(
            fp[0].extra_annotations
                .get("mikebom:cmake-find-package-name")
                .and_then(|v| v.as_str()),
            Some("BOOST")
        );
    }

    #[test]
    fn find_package_multiple_versions_highest_wins() {
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
        // Two emissions (one per site) — but both carry the same
        // winning-version PURL so downstream milestone-148 union
        // merges them via same-canonical-PURL matching.
        assert_eq!(fp.len(), 2);
        for e in &fp {
            assert_eq!(e.purl.as_str(), "pkg:generic/openssl@3.0");
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
            assert_eq!(
                e.purl.as_str(),
                "pkg:generic/openssl@1.1.0",
                "versioned declaration wins over version-less"
            );
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
                .contains_key("mikebom:cmake-find-package-name"),
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
        // Contracts/mikebom-cmake-find-package-name.md conditional
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
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/zlib@1.2.11");
        assert!(
            !fp[0]
                .extra_annotations
                .contains_key("mikebom:cmake-find-package-name"),
            "all-lowercase input MUST NOT emit mikebom:cmake-find-package-name"
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
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/boost@1.75.0");
    }

    // ============================================================
    // Milestone 156 unit tests (SC-008 floor ≥6). All 6 test fns
    // named with the `discover_cmake_files_` prefix so the SC-008
    // grep count command
    //   grep -cE "^\s+fn discover_cmake_files_" mikebom-cli/src/scan_fs/package_db/cmake.rs
    // returns ≥6 (F2 remediation from /speckit-analyze 2026-07-02).
    // ============================================================

    /// Serial guard for env-var mutation tests. cargo runs tests
    /// concurrently by default; `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE`
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
        unsafe { std::env::remove_var("MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE") };
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
        assert_eq!(fp[0].purl.as_str(), "pkg:generic/libev@1.4.0");
        assert!(
            fp[0]
                .source_path
                .contains("cmake/modules/FindLibev.cmake")
                || fp[0].source_path.contains("cmake\\modules\\FindLibev.cmake"),
            "source path should point at the depth-2 file; got {}",
            fp[0].source_path
        );
    }
}
