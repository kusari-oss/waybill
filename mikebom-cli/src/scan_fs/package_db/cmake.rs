//! CMake source-tree reader (milestone 102 US2 / milestone 103 implementation).
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
//!
//! `find_package(X)` declarations are NOT parsed per FR-007 — they
//! resolve to system-installed packages and would double-count
//! against OS-package readers + vcpkg + Conan.
//!
//! Walks at depth 1: scan root for `CMakeLists.txt`; `cmake/`,
//! `Modules/`, `third_party/` for `*.cmake` files. Per FR-005.
//!
//! Cross-platform; no `#[cfg(unix)]` gates. Zero new Cargo deps —
//! uses workspace `regex` + std.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mikebom_common::types::hash::ContentHash;
use mikebom_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

pub fn read(scan_root: &Path, include_vendored: bool) -> Vec<PackageDbEntry> {
    let cmake_files = discover_cmake_files(scan_root);
    let mut entries = Vec::new();
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
    }
    entries
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
/// target names. Milestone 102's FR-007 (`find_package_does_not_emit_components`
/// regression test) remains green — the collector is a parallel pass
/// that reads the same files but only populates a name set.
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
    let cmake_files = discover_cmake_files(scan_root);
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

/// Discover CMake files: top-level `CMakeLists.txt` + any `*.cmake`
/// (and `CMakeLists.txt`) at depth 1 of `cmake/`, `Modules/`,
/// `third_party/`. Non-recursive per FR-005.
fn discover_cmake_files(scan_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let top = scan_root.join("CMakeLists.txt");
    if top.is_file() {
        out.push(top);
    }
    for subdir in &["cmake", "Modules", "third_party"] {
        let dir = scan_root.join(subdir);
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let p = entry.path();
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
                if (is_cmake_module || is_cmakelists) && p.is_file() {
                    out.push(p);
                }
            }
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
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_range: None,
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
        assert!(read(tmp.path(), false).is_empty());
    }

    #[test]
    fn fetchcontent_github_emits_pkg_github() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"FetchContent_Declare(googletest GIT_REPOSITORY https://github.com/google/googletest.git GIT_TAG release-1.14.0)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false);
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
        let entries = read(tmp.path(), false);
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
        let entries = read(tmp.path(), false);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].purl.as_str().starts_with("pkg:generic/boost@"));
    }

    #[test]
    fn find_package_does_not_emit_components() {
        let tmp = tempfile::tempdir().unwrap();
        // ONLY find_package, no FetchContent_Declare or ExternalProject_Add.
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"find_package(zlib REQUIRED)"#,
        )
        .unwrap();
        let entries = read(tmp.path(), false);
        assert!(
            entries.is_empty(),
            "find_package(X) MUST NOT emit components per FR-007; got {entries:?}"
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
        assert!(read(tmp.path(), false).is_empty());

        // With include_vendored = true: 1 component with vendored annotation.
        let entries = read(tmp.path(), true);
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
        let entries = read(tmp.path(), true);
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
        let entries = read(tmp.path(), false);
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
        let entries = read(tmp.path(), false);
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
        let entries = read(tmp.path(), false);
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
        let entries = read(tmp.path(), true);
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
        // FR-008a guarantee: the collector populates a name set but
        // does NOT contribute to PackageDbEntry emission. Calling
        // both `read()` AND `collect_find_package_targets()` against
        // the same input MUST behave identically: `read()` ignores
        // find_package per milestone 102's FR-007.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("CMakeLists.txt"),
            r#"
                find_package(zlib REQUIRED)
                add_library(MyAlias::MyAlias ALIAS myimpl)
            "#,
        )
        .unwrap();
        let entries = read(tmp.path(), false);
        let names = collect_find_package_targets(tmp.path());
        assert!(entries.is_empty(), "read() MUST emit zero components for find_package + ALIAS input");
        assert!(!names.is_empty(), "collector MUST populate the set");
    }
}
