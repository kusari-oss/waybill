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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::purl::{encode_purl_segment, Purl};
use regex::Regex;

use super::PackageDbEntry;

// Milestone 664 US2 T054: shared-walker marker-detect registration.
use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

const CONANFILE_TXT: &str = "conanfile.txt";
const CONANFILE_PY: &str = "conanfile.py";

/// Milestone 664 US2 T054: marker-detect state, recording every
/// `conanfile.txt` / `conanfile.py` the shared walker dispatched. Until
/// #1521 only the flag was kept and only the scan root's files were read.
#[derive(Default, Debug)]
pub(crate) struct ConanMarkerState {
    pub(crate) seen: bool,
    pub(crate) paths: BTreeSet<PathBuf>,
}

fn on_conan_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    if name != CONANFILE_TXT && name != CONANFILE_PY {
        return;
    }
    let Some(state) = ctx.state::<Mutex<ConanMarkerState>>(ReaderId::CONAN) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.seen = true;
    guard.paths.insert(path.to_path_buf());
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/conanfile.txt", "**/conanfile.py"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::CONAN,
        state: Some(Arc::new(Mutex::new(ConanMarkerState::default()))),
        patterns,
        on_file: Some(on_conan_file),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_marker(registration: &ReaderRegistration) -> ConanMarkerState {
    let Some(state_arc) = registration.state.as_ref() else {
        return ConanMarkerState::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<ConanMarkerState>>() else {
        return ConanMarkerState::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Post-walker entry — gates the O(1) fixed-root read on marker
/// presence + defensive fs-existence fallback (preserves FR-006
/// byte-identity when the walker missed the marker via exclusion or
/// symlink resolution).
pub(crate) fn finalize(
    marker: ConanMarkerState,
    scan_root: &Path,
) -> Vec<PackageDbEntry> {
    let mut paths = marker.paths;
    paths.extend(root_conanfiles(scan_root));
    read_conanfiles(paths)
}

fn root_conanfiles(scan_root: &Path) -> Vec<PathBuf> {
    [CONANFILE_TXT, CONANFILE_PY]
        .iter()
        .map(|n| scan_root.join(n))
        .filter(|p| p.is_file())
        .collect()
}

/// Read each conanfile once, in path order. Paths are canonicalized so a
/// root file reached both by the walker and by the root fallback is read
/// once.
fn read_conanfiles(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PackageDbEntry> {
    let unique: BTreeSet<PathBuf> = paths
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    let mut entries = Vec::new();
    for path in unique {
        match path.file_name().and_then(|s| s.to_str()) {
            Some(CONANFILE_TXT) => entries.extend(parse_conanfile_txt(&path)),
            Some(CONANFILE_PY) => entries.extend(parse_conanfile_py(&path)),
            _ => {}
        }
    }
    entries
}

/// Read only the scan root's conanfile.txt and conanfile.py (test helper;
/// a scan reads every conanfile the walker dispatched, via `finalize`).
/// `[tool_requires]` lines (txt) and `tool_requires =` (py) emit with
/// `LifecycleScope::Build` per FR-008.
#[cfg(test)]
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry> {
    read_conanfiles(root_conanfiles(scan_root))
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
    entries.extend(parse_requires_method_calls(&content, &source_path));
    entries
}

/// #1521: the Conan 2 method form, `self.requires("name/version")` and its
/// `tool_requires` / `build_requires` / `test_requires` siblings, with a
/// literal first argument. An f-string or variable argument is skipped.
/// A runtime `requires` nested under a conditional or loop depends on
/// options, so it is marked optional (C122 `conan-conditional-requires`).
fn parse_requires_method_calls(content: &str, source_path: &str) -> Vec<PackageDbEntry> {
    static RE: std::sync::OnceLock<Option<Regex>> = std::sync::OnceLock::new();
    let Some(re) = RE
        .get_or_init(|| {
            Regex::new(r#"^(\s*)self\.(requires|tool_requires|build_requires|test_requires)\(\s*["']([^"']+)["']"#).ok()
        })
        .as_ref()
    else {
        return Vec::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(c) = re.captures(line) else {
            continue;
        };
        let conditional = c[2] == *"requires" && under_conditional(&lines, i, c[1].len());
        let scope = match &c[2] {
            "tool_requires" | "build_requires" => Some(LifecycleScope::Build),
            "test_requires" => Some(LifecycleScope::Test),
            _ if conditional => Some(LifecycleScope::Optional),
            _ => None,
        };
        if let Some(mut entry) = parse_dep_token(&c[3], source_path, scope) {
            if conditional {
                entry.extra_annotations.insert(
                    "waybill:optional-derivation".to_string(),
                    serde_json::json!("conan-conditional-requires"),
                );
            }
            out.push(entry);
        }
    }
    out
}

/// Whether the line at `idx`, indented `indent` columns, sits inside an
/// `if` / `elif` / `else` / `for` / `while` block rather than directly in
/// its method body: the nearest less-indented line above opens the block.
fn under_conditional(lines: &[&str], idx: usize, indent: usize) -> bool {
    for line in lines[..idx].iter().rev() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.len() - trimmed.len() < indent {
            return ["if ", "elif ", "else", "for ", "while "]
                .iter()
                .any(|k| trimmed.starts_with(k));
        }
    }
    false
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
        lifecycle_scope: scope,
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
                "waybill:source-mechanism".to_string(),
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
                    .get("waybill:source-mechanism")
                    .and_then(|v| v.as_str()),
                Some("conan-recipe"),
                "every conan entry should carry source-mechanism: conan-recipe; got: {:?}",
                e.extra_annotations.get("waybill:source-mechanism"),
            );
        }
    }

    fn s1521_marker(paths: &[&Path]) -> ConanMarkerState {
        ConanMarkerState {
            seen: !paths.is_empty(),
            paths: paths.iter().map(|p| p.to_path_buf()).collect(),
        }
    }

    fn s1521_purls(entries: &[PackageDbEntry]) -> Vec<String> {
        let mut v: Vec<String> = entries.iter().map(|e| e.purl.as_str().to_string()).collect();
        v.sort();
        v
    }

    /// #1521: the issue's repro — a nested conanfile the walker dispatched
    /// is read, not only the one at the scan root.
    #[test]
    fn s1521_nested_conanfile_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("conanfile.py");
        let nested = tmp.path().join("libs/a/conanfile.py");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&root, "requires = [\"waybill-fixture-root/1.0.0\"]\n").unwrap();
        std::fs::write(&nested, "requires = [\"waybill-fixture-nested-literal/2.0.0\"]\n").unwrap();
        let out = finalize(s1521_marker(&[&root, &nested]), tmp.path());
        assert_eq!(
            s1521_purls(&out),
            ["pkg:conan/waybill-fixture-nested-literal@2.0.0", "pkg:conan/waybill-fixture-root@1.0.0"]
        );
    }

    /// #1521: the Conan 2 method form, with scopes, and a call under a
    /// conditional marked optional (C122 `conan-conditional-requires`).
    #[test]
    fn s1521_self_requires_method_calls() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("conanfile.py"),
            "from conan import ConanFile\n\n\
             class Recipe(ConanFile):\n    \
                 options = {\"with_x\": [True, False]}\n\n    \
                 def requirements(self):\n        \
                     self.requires(\"waybill-fixture-method/3.0.0\")\n        \
                     self.requires('waybill-fixture-single/1.1', transitive_headers=True)\n        \
                     if self.options.with_x:\n            \
                         self.requires(\"waybill-fixture-cond/4.0.0\")\n        \
                     self.requires(f\"waybill-fixture-fstring/{self.version}\")\n\n    \
                 def build_requirements(self):\n        \
                     self.tool_requires(\"waybill-fixture-tool/5.0.0\")\n        \
                     self.test_requires(\"waybill-fixture-test/6.0.0\")\n",
        )
        .unwrap();
        let out = read(tmp.path());
        let mut got: Vec<(String, Option<LifecycleScope>, Option<String>)> = out
            .iter()
            .map(|e| {
                (
                    e.purl.as_str().to_string(),
                    e.lifecycle_scope,
                    e.extra_annotations
                        .get("waybill:optional-derivation")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                )
            })
            .collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            got,
            vec![
                ("pkg:conan/waybill-fixture-cond@4.0.0".to_string(), Some(LifecycleScope::Optional), Some("conan-conditional-requires".to_string())),
                ("pkg:conan/waybill-fixture-method@3.0.0".to_string(), None, None),
                ("pkg:conan/waybill-fixture-single@1.1".to_string(), None, None),
                ("pkg:conan/waybill-fixture-test@6.0.0".to_string(), Some(LifecycleScope::Test), None),
                ("pkg:conan/waybill-fixture-tool@5.0.0".to_string(), Some(LifecycleScope::Build), None),
            ]
        );
    }
}
