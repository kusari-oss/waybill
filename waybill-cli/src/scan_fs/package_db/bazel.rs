//! Bazel source-tree reader (milestone 102 US1 / milestone 103 implementation).
//!
//! Parses two manifest formats:
//! - `MODULE.bazel` (Bzlmod, Bazel 6+) — `bazel_dep(name = ..., version = ...)`.
//! - `WORKSPACE.bazel` / `WORKSPACE` (legacy) — `http_archive`, `http_file`,
//!   `git_repository` rules.
//!
//! Emits `pkg:bazel/<name>@<version>` components per FR-002 + FR-003.
//! Declared upstream URLs surface as `mikebom:download-url`; declared
//! `sha256` values populate `hashes[]` as SHA-256 ContentHashes per
//! FR-004. `dev_dependency = True` sets `LifecycleScope::Development`
//! (which maps to standards-native CDX `scope` per Principle V).
//!
//! Per spec FR-001..FR-004 + FR-011 (cross-platform) + FR-013
//! (skip-with-warn on parse errors).
//!
//! Cross-platform; no `#[cfg(unix)]` gates. Zero new Cargo deps —
//! uses workspace `regex` + std.

use std::path::Path;

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

const MODULE_BAZEL: &str = "MODULE.bazel";
const WORKSPACE_BAZEL: &str = "WORKSPACE.bazel";
const WORKSPACE: &str = "WORKSPACE";

/// Short-SHA truncation length per research §8. Full SHA discarded
/// for now; future milestone could preserve via `mikebom:bazel-commit-sha`.
const SHORT_SHA_LEN: usize = 7;
/// Full git-SHA length (the threshold above which we treat the version
/// as a SHA worth truncating; below it, it's a tag, used verbatim).
const FULL_SHA_LEN: usize = 40;

/// Walk `scan_root` for Bazel manifests and emit one `PackageDbEntry`
/// per declared dependency. MODULE.bazel deps come first; WORKSPACE
/// deps follow. Cross-file dedup-by-name preserves MODULE.bazel wins
/// per Contract 3.
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let mut entries = Vec::new();

    // MODULE.bazel — Bzlmod (Bazel 6+), preferred per FR-002.
    let module_path = scan_root.join(MODULE_BAZEL);
    if module_path.is_file() {
        entries.extend(parse_module_bazel(&module_path));
    }

    // WORKSPACE.bazel + WORKSPACE (legacy) per FR-003. Bazel only
    // loads one — prefer the `.bazel` suffix; fall back to the
    // unsuffixed file if absent.
    for ws_name in &[WORKSPACE_BAZEL, WORKSPACE] {
        let ws_path = scan_root.join(ws_name);
        if ws_path.is_file() {
            entries.extend(parse_workspace_bazel(&ws_path));
            break;
        }
    }

    dedup_by_name(entries)
}

fn parse_module_bazel(path: &Path) -> Vec<PackageDbEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read MODULE.bazel (FR-013)"
            );
            return Vec::new();
        }
    };
    // Matches `bazel_dep(name = "X", version = "Y" [, dev_dependency = (True|False)])`
    // per research §1. `(?ms)` enables multiline + dotall.
    let re = match Regex::new(
        r#"(?ms)bazel_dep\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*version\s*=\s*"([^"]+)"(?:\s*,\s*dev_dependency\s*=\s*(True|False))?\s*\)"#,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to compile MODULE.bazel regex");
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().to_string();
    re.captures_iter(&content)
        .filter_map(|c| {
            let name = c.get(1)?.as_str();
            let version = c.get(2)?.as_str();
            let dev = c.get(3).map(|m| m.as_str() == "True").unwrap_or(false);
            build_bazel_entry(name, version, &source_path, None, None, dev)
        })
        .collect()
}

fn parse_workspace_bazel(path: &Path) -> Vec<PackageDbEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read WORKSPACE.bazel (FR-013)"
            );
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().to_string();
    let mut entries = Vec::new();

    // http_archive + http_file outer envelope. Two-pass approach
    // per research §2: outer captures rule type + name + arg body;
    // inner extractors below pull urls/url/sha256 from the body in
    // any order.
    if let Ok(http_re) = Regex::new(
        r#"(?ms)(http_archive|http_file)\s*\(\s*name\s*=\s*"([^"]+)"\s*,(.*?)\)"#,
    ) {
        for c in http_re.captures_iter(&content) {
            let name = c.get(2).map(|m| m.as_str()).unwrap_or("");
            let body = c.get(3).map(|m| m.as_str()).unwrap_or("");
            let archive_url = extract_first_url(body);
            let sha256 = extract_sha256(body);
            let version = archive_url
                .as_deref()
                .and_then(parse_version_from_url)
                .unwrap_or_else(|| "unknown".to_string());
            if let Some(entry) = build_bazel_entry(
                name,
                &version,
                &source_path,
                archive_url.as_deref(),
                sha256.as_deref(),
                false,
            ) {
                entries.push(entry);
            }
        }
    }

    // git_repository outer envelope.
    if let Ok(git_re) = Regex::new(
        r#"(?ms)git_repository\s*\(\s*name\s*=\s*"([^"]+)"\s*,(.*?)\)"#,
    ) {
        for c in git_re.captures_iter(&content) {
            let name = c.get(1).map(|m| m.as_str()).unwrap_or("");
            let body = c.get(2).map(|m| m.as_str()).unwrap_or("");
            let remote_url = extract_simple_string(body, "remote");
            let commit = extract_simple_string(body, "commit");
            let tag = extract_simple_string(body, "tag");
            // Prefer commit (truncated to short-SHA); fall back to tag.
            let version = commit
                .as_deref()
                .map(|sha| {
                    if sha.len() >= FULL_SHA_LEN {
                        sha[..SHORT_SHA_LEN].to_string()
                    } else {
                        sha.to_string()
                    }
                })
                .or_else(|| tag.clone())
                .unwrap_or_else(|| "unknown".to_string());
            if let Some(entry) = build_bazel_entry(
                name,
                &version,
                &source_path,
                remote_url.as_deref(),
                None,
                false,
            ) {
                entries.push(entry);
            }
        }
    }

    entries
}

fn extract_first_url(body: &str) -> Option<String> {
    // `urls = ["..."]` form (first URL only — Bazel allows mirror lists).
    if let Ok(re) = Regex::new(r#"urls\s*=\s*\[\s*"([^"]+)""#) {
        if let Some(c) = re.captures(body) {
            return c.get(1).map(|m| m.as_str().to_string());
        }
    }
    // `url = "..."` singular form.
    extract_simple_string(body, "url")
}

fn extract_simple_string(body: &str, key: &str) -> Option<String> {
    let pattern = format!(r#"\b{}\s*=\s*"([^"]+)""#, regex::escape(key));
    let re = Regex::new(&pattern).ok()?;
    re.captures(body)?.get(1).map(|m| m.as_str().to_string())
}

fn extract_sha256(body: &str) -> Option<String> {
    let re = Regex::new(r#"sha256\s*=\s*"([0-9a-fA-F]+)""#).ok()?;
    re.captures(body)?.get(1).map(|m| m.as_str().to_string())
}

/// Parse a semver-ish version from an archive URL filename — matches
/// `name-1.2.3.tar.gz`, `name_1.2.3.zip`, `/1.2.3.tar.gz` (the
/// GitHub `/archive/<tag>.tar.gz` shape), and `/v1.2.3.tar.gz`.
/// Returns None when no version segment is found.
fn parse_version_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"[-_/]v?([0-9]+\.[0-9]+(?:\.[0-9]+)?)").ok()?;
    re.captures(url)?.get(1).map(|m| m.as_str().to_string())
}

fn build_bazel_entry(
    name: &str,
    version: &str,
    source_path: &str,
    download_url: Option<&str>,
    sha256_hex: Option<&str>,
    dev_dependency: bool,
) -> Option<PackageDbEntry> {
    let purl_str = format!(
        "pkg:bazel/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version)
    );
    let purl = Purl::new(&purl_str).ok()?;

    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(url) = download_url {
        extra_annotations.insert(
            "mikebom:download-url".to_string(),
            serde_json::json!(url),
        );
        extra_annotations.insert(
            "mikebom:bazel-archive-name".to_string(),
            serde_json::json!(name),
        );
    }
    // C/C++ provenance: explicit source-mechanism annotation
    // (closed-enum value `bazel-http-archive`). See cmake.rs for
    // the full rationale + enum docs.
    extra_annotations.insert(
        "mikebom:source-mechanism".to_string(),
        serde_json::json!("bazel-http-archive"),
    );

    let hashes = sha256_hex
        .and_then(|hex| ContentHash::sha256(hex).ok())
        .map(|h| vec![h])
        .unwrap_or_default();

    let lifecycle_scope = if dev_dependency {
        Some(LifecycleScope::Development)
    } else {
        None
    };

    Some(PackageDbEntry {
        build_inclusion: None,
        purl,
        name: name.to_string(),
        version: version.to_string(),
        arch: None,
        source_path: source_path.to_string(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope,
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
    })
}

/// Dedup by name; preserves first-seen order. Since MODULE.bazel
/// entries are appended before WORKSPACE entries, MODULE.bazel wins
/// on same-name conflicts per Contract 3.
fn dedup_by_name(entries: Vec<PackageDbEntry>) -> Vec<PackageDbEntry> {
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        if seen.insert(e.name.clone()) {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn empty_when_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read(tmp.path()).is_empty());
    }

    #[test]
    fn module_bazel_emits_pkg_bazel_purl() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("MODULE.bazel"),
            r#"bazel_dep(name = "abseil-cpp", version = "20240722.0")"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].purl.as_str(),
            "pkg:bazel/abseil-cpp@20240722.0"
        );
        assert_eq!(entries[0].lifecycle_scope, None);
    }

    #[test]
    fn module_bazel_dev_dependency_sets_development_scope() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("MODULE.bazel"),
            r#"bazel_dep(name = "googletest", version = "1.14.0", dev_dependency = True)"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].lifecycle_scope,
            Some(LifecycleScope::Development)
        );
    }

    #[test]
    fn workspace_http_archive_emits_url_and_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("WORKSPACE.bazel"),
            r#"
http_archive(
    name = "rules_python",
    urls = ["https://github.com/bazelbuild/rules_python/archive/0.30.0.tar.gz"],
    sha256 = "abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890abc",
)
"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "rules_python");
        assert_eq!(entries[0].version, "0.30.0");
        assert_eq!(entries[0].hashes.len(), 1);
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:download-url")
                .and_then(|v| v.as_str()),
            Some("https://github.com/bazelbuild/rules_python/archive/0.30.0.tar.gz")
        );
        assert_eq!(
            entries[0]
                .extra_annotations
                .get("mikebom:bazel-archive-name")
                .and_then(|v| v.as_str()),
            Some("rules_python")
        );
    }

    #[test]
    fn workspace_git_repository_uses_short_sha() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("WORKSPACE.bazel"),
            r#"
git_repository(
    name = "rules_foo",
    remote = "https://github.com/foo/rules_foo.git",
    commit = "deadbeef0123456789abcdef0123456789abcdef",
)
"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        // Short-SHA truncation: first 7 chars of the 40-char commit.
        assert_eq!(entries[0].purl.as_str(), "pkg:bazel/rules_foo@deadbee");
    }

    #[test]
    fn module_wins_on_name_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("MODULE.bazel"),
            r#"bazel_dep(name = "foo", version = "2.0.0")"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("WORKSPACE.bazel"),
            r#"
http_archive(
    name = "foo",
    urls = ["https://example.com/foo-1.0.0.tar.gz"],
)
"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, "2.0.0"); // MODULE wins
    }

    #[test]
    fn source_mechanism_annotation_bazel_http_archive() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("WORKSPACE"),
            r#"http_archive(
    name = "zlib",
    urls = ["https://zlib.net/zlib-1.3.1.tar.gz"],
    sha256 = "9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23",
)
"#,
        )
        .unwrap();
        let entries = read(tmp.path());
        assert!(!entries.is_empty());
        for e in &entries {
            assert_eq!(
                e.extra_annotations
                    .get("mikebom:source-mechanism")
                    .and_then(|v| v.as_str()),
                Some("bazel-http-archive"),
                "every bazel entry should carry source-mechanism: bazel-http-archive; got: {:?}",
                e.extra_annotations.get("mikebom:source-mechanism"),
            );
        }
    }
}
