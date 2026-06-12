//! Conan recipe reader (milestone 102 US3).
//!
//! Parses `conanfile.txt` (INI-style sections) and `conanfile.py` (Python
//! recipe — heuristic regex over literal `requires = [...]` and
//! `tool_requires = [...]` assignments). Each `<name>/<version>` token
//! becomes one `pkg:conan/<name>@<version>` component. `[tool_requires]`
//! / `tool_requires =` entries get `LifecycleScope::Build` per
//! Constitution Principle V (standards-native scope mapping). Per spec
//! FR-008 + FR-009 + Contract 8.
//!
//! Parse failures emit `tracing::warn!` and return zero components per
//! FR-015. Cross-platform (no `#[cfg(unix)]` per FR-013).
//!
//! No new Cargo deps — uses workspace `regex` + std.

use std::path::Path;

use mikebom_common::resolution::LifecycleScope;
use mikebom_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

const CONANFILE_TXT: &str = "conanfile.txt";
const CONANFILE_PY: &str = "conanfile.py";

/// Walk `scan_root` for conanfile.txt and conanfile.py; emit one
/// `PackageDbEntry` per declared dependency from both. `[tool_requires]`
/// lines (txt) and `tool_requires =` (py) emit with
/// `LifecycleScope::Build` per FR-008.
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    let mut entries = Vec::new();

    let txt_path = scan_root.join(CONANFILE_TXT);
    if txt_path.is_file() {
        entries.extend(parse_conanfile_txt(&txt_path));
    }

    let py_path = scan_root.join(CONANFILE_PY);
    if py_path.is_file() {
        entries.extend(parse_conanfile_py(&py_path));
    }

    entries
}

/// Parse a conanfile.txt — INI-style section headers (`[requires]`,
/// `[tool_requires]`, `[options]`, …) with `<name>/<version>` lines
/// inside each section. Comments (`#`) and blank lines are skipped.
fn parse_conanfile_txt(path: &Path) -> Vec<PackageDbEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read conanfile.txt (FR-015)"
            );
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().to_string();
    let mut entries = Vec::new();
    let mut current_section: Section = Section::Other;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(header) = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
        {
            current_section = match header.trim() {
                "requires" => Section::Requires,
                "tool_requires" | "build_requires" => Section::ToolRequires,
                _ => Section::Other,
            };
            continue;
        }
        let scope = match current_section {
            Section::Requires => None,
            Section::ToolRequires => Some(LifecycleScope::Build),
            Section::Other => continue,
        };
        // Strip inline `# comment` if present.
        let token = trimmed.split('#').next().unwrap_or("").trim();
        if let Some(entry) = parse_dep_token(token, &source_path, scope) {
            entries.push(entry);
        }
    }
    entries
}

#[derive(Copy, Clone)]
enum Section {
    Requires,
    ToolRequires,
    Other,
}

/// Parse a conanfile.py — best-effort regex over LITERAL list
/// assignments. Non-literal cases (`requires = base + ["zlib/1.2.13"]`)
/// are documented out-of-scope per SC-005's 80% heuristic ceiling.
fn parse_conanfile_py(path: &Path) -> Vec<PackageDbEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "failed to read conanfile.py (FR-015)"
            );
            return Vec::new();
        }
    };
    let source_path = path.to_string_lossy().to_string();
    let mut entries = Vec::new();

    // `(?ms)` for multiline / dotall.
    // Match `requires = [...]` and `tool_requires = [...]` literal-list
    // assignments. The inner `([^\]]+)` is non-greedy by virtue of the
    // negated character class — first `]` ends the list.
    let assign_re = match Regex::new(
        r"(?m)^\s*(requires|tool_requires|build_requires)\s*=\s*\[([^\]]*)\]",
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "failed to compile conanfile.py regex");
            return Vec::new();
        }
    };
    // Match each `"<name>/<version>"` (or single-quoted) string literal
    // inside the list body.
    let token_re = match Regex::new(r#"["']([^"'/]+/[^"',\s]+)["']"#) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    for cap in assign_re.captures_iter(&content) {
        let kind = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        let body = cap.get(2).map(|m| m.as_str()).unwrap_or_default();
        let scope = match kind {
            "tool_requires" | "build_requires" => Some(LifecycleScope::Build),
            _ => None,
        };
        for tok in token_re.captures_iter(body) {
            if let Some(t) = tok.get(1) {
                if let Some(entry) = parse_dep_token(t.as_str(), &source_path, scope) {
                    entries.push(entry);
                }
            }
        }
    }
    entries
}

/// Parse a `<name>/<version>` Conan dep token into a `PackageDbEntry`.
/// Returns None on malformed tokens (no `/`, empty halves).
fn parse_dep_token(
    token: &str,
    source_path: &str,
    scope: Option<LifecycleScope>,
) -> Option<PackageDbEntry> {
    let token = token.trim();
    // Conan supports `name/version@user/channel` and `name/version` —
    // strip the optional `@user/channel` suffix.
    let core = token.split('@').next()?;
    let mut parts = core.splitn(2, '/');
    let name = parts.next()?.trim();
    let version = parts.next()?.trim();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    let purl = Purl::new(&format!(
        "pkg:conan/{}@{}",
        encode_purl_segment(name),
        encode_purl_segment(version)
    ))
    .ok()?;
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
        lifecycle_scope: scope,
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
        hashes: Vec::new(),
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations: {
            // C/C++ provenance: explicit source-mechanism annotation
            // (closed-enum value `conan-recipe`). See cmake.rs for
            // the full rationale + enum docs.
            let mut a: std::collections::BTreeMap<String, serde_json::Value> =
                Default::default();
            a.insert(
                "mikebom:source-mechanism".to_string(),
                serde_json::json!("conan-recipe"),
            );
            a
        },
        binary_role: None,
    })
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
    fn conanfile_txt_requires_emits_runtime_scope() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.txt"),
            "[requires]\nzlib/1.2.13\nopenssl/3.0.0\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .any(|e| e.purl.as_str() == "pkg:conan/zlib@1.2.13"
                && e.lifecycle_scope.is_none()));
        assert!(entries
            .iter()
            .any(|e| e.purl.as_str() == "pkg:conan/openssl@3.0.0"
                && e.lifecycle_scope.is_none()));
    }

    #[test]
    fn conanfile_txt_tool_requires_emits_build_scope() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.txt"),
            "[tool_requires]\ncmake/3.27.0\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:conan/cmake@3.27.0");
        assert_eq!(entries[0].lifecycle_scope, Some(LifecycleScope::Build));
    }

    #[test]
    fn conanfile_py_literal_list_emits_components() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.py"),
            "from conan import ConanFile\nclass T(ConanFile):\n    requires = [\"zlib/1.2.13\", \"openssl/3.0.0\"]\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.purl.as_str() == "pkg:conan/zlib@1.2.13"));
        assert!(entries.iter().any(|e| e.purl.as_str() == "pkg:conan/openssl@3.0.0"));
    }

    #[test]
    fn conanfile_py_tool_requires_emits_build_scope() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.py"),
            "class T:\n    tool_requires = [\"cmake/3.27.0\"]\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lifecycle_scope, Some(LifecycleScope::Build));
    }

    #[test]
    fn conanfile_txt_skips_comments_and_blanks() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.txt"),
            "# header comment\n\n[requires]\n# inline note\nzlib/1.2.13\n\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn malformed_token_in_txt_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.txt"),
            "[requires]\nzlib_no_slash\nopenssl/3.0.0\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].purl.as_str(), "pkg:conan/openssl@3.0.0");
    }

    #[test]
    fn source_mechanism_annotation_conan_recipe() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.txt"),
            "[requires]\nzlib/1.3.1\nopenssl/3.0.0\n",
        )
        .unwrap();
        let entries = read(tmp.path());
        assert_eq!(entries.len(), 2);
        for e in &entries {
            assert_eq!(
                e.extra_annotations
                    .get("mikebom:source-mechanism")
                    .and_then(|v| v.as_str()),
                Some("conan-recipe"),
                "every conan entry should carry source-mechanism: conan-recipe; got: {:?}",
                e.extra_annotations.get("mikebom:source-mechanism"),
            );
        }
    }
}
