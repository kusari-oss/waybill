//! Spec 1618 (#1616): a component for each CMake project the scanned tree
//! builds.
//!
//! A literal `project(<name> …)` at the top level of a `CMakeLists.txt` is a
//! project declaration (research R1). Its version is the `VERSION` argument:
//! literal, resolved through the file's own single-valued literal `set()`, an
//! unresolved expression (C238), or absent (C197 `not-declared`, research R7).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use waybill_common::resolution::{LifecycleScope, ResolvedComponent};

use crate::scan_fs::package_db::PackageDbEntry;

/// C55 `waybill:source-mechanism` of a project component.
pub(crate) const MECHANISM_CMAKE_PROJECT: &str = "cmake-project";

/// The annotations a CMake project keeps when CycloneDX promotes it to
/// `metadata.component` (`generate/cyclonedx/metadata.rs`).
pub(crate) const ROOT_PROPAGATED_KEYS: &[&str] = &[
    "waybill:source-mechanism",
    "waybill:version-absence",
    ANN_DECLARED_VERSION_EXPRESSION,
    ANN_CMAKE_BUILD_REACH,
    ANN_VENDORED,
    ANN_VENDORED_SIGNALS,
];

/// Whether `c` is an in-tree CMake project component.
pub(crate) fn is_cmake_project(c: &ResolvedComponent) -> bool {
    c.extra_annotations.get("waybill:source-mechanism").and_then(|v| v.as_str()) == Some(MECHANISM_CMAKE_PROJECT)
}

/// One literal top-level `project()` in a `CMakeLists.txt` (data model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDeclaration {
    /// Lowercased.
    pub(crate) name: String,
    pub(crate) version: DeclaredVersion,
    /// The declaring `CMakeLists.txt`, relative to the scan root.
    pub(crate) rel_file: PathBuf,
    /// The same file as the reader names it in evidence.
    pub(crate) source_path: String,
    /// Whether the scan-root `CMakeLists.txt` reaches the file (research R9);
    /// `None` when there is no scan-root `CMakeLists.txt`.
    pub(crate) reached: Option<bool>,
}

/// C238: an unresolved `VERSION` expression, verbatim.
pub(crate) const ANN_DECLARED_VERSION_EXPRESSION: &str = "waybill:declared-version-expression";
/// C239: `not-reached` when the root build does not reach the project.
pub(crate) const ANN_CMAKE_BUILD_REACH: &str = "waybill:cmake-build-reach";
/// C241: the candidate vendored signals that hold (research R10).
pub(crate) const ANN_VENDORED_SIGNALS: &str = "waybill:vendored-signals";
/// C240: the chosen vendored rule holds (T002: path-segment + submodule-boundary).
pub(crate) const ANN_VENDORED: &str = "waybill:vendored";
const ANN_VERSION_ABSENCE: &str = "waybill:version-absence";
const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";
/// C62 for a project under an example, sample, benchmark or documentation
/// directory (research R8, census (c)).
pub(crate) const DERIVATION_SOURCE_EXAMPLE_DIRECTORY: &str = "source-example-directory";
const DERIVATION_SOURCE_TEST_DIRECTORY: &str = "source-test-directory";
/// Directories whose projects are Development (census (c)).
pub(crate) const EXAMPLE_DIRS: [&str; 10] =
    ["example", "examples", "sample", "samples", "benchmark", "benchmarks", "bench", "doc", "docs", "documentation"];
/// R10's `path-segment` signal.
pub(crate) const VENDOR_SEGMENTS: [&str; 8] =
    ["third_party", "third-party", "thirdparty", "3rdparty", "external", "extern", "vendor", "deps"];
pub(crate) const SIGNAL_PATH_SEGMENT: &str = "path-segment";
pub(crate) const SIGNAL_SUBMODULE_BOUNDARY: &str = "submodule-boundary";
pub(crate) const SIGNAL_LICENCE_FILE: &str = "licence-file";
pub(crate) const SIGNAL_NOT_REACHED: &str = "not-reached";

/// One component's declarations (research R5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub(crate) name: String,
    /// The effective version: a versioned declaration's, or for a group with
    /// none, the first expression, else `NotDeclared`.
    pub(crate) version: DeclaredVersion,
    /// Sorted by declaring file.
    pub(crate) decls: Vec<ProjectDeclaration>,
}

/// Group declarations into components (research R5): one per (name,
/// version); a declaration with no literal or resolved version joins the
/// name's lowest versioned group, or with none, one versionless group.
pub(crate) fn group(decls: &[ProjectDeclaration]) -> Vec<Group> {
    let mut by_name: BTreeMap<&str, Vec<&ProjectDeclaration>> = BTreeMap::new();
    for d in decls {
        by_name.entry(d.name.as_str()).or_default().push(d);
    }
    let mut out = Vec::new();
    for (name, ds) in by_name {
        let mut versioned: BTreeMap<&str, Vec<ProjectDeclaration>> = BTreeMap::new();
        let mut rest: Vec<ProjectDeclaration> = Vec::new();
        for d in ds {
            match d.version.value() {
                Some(v) => versioned.entry(v).or_default().push(d.clone()),
                None => rest.push(d.clone()),
            }
        }
        if let Some(first) = versioned.values_mut().next() {
            first.append(&mut rest);
            for (v, mut ds) in versioned {
                ds.sort_by(|a, b| a.rel_file.cmp(&b.rel_file));
                out.push(Group { name: name.to_string(), version: DeclaredVersion::Literal(v.to_string()), decls: ds });
            }
        } else {
            rest.sort_by(|a, b| a.rel_file.cmp(&b.rel_file));
            let expression = rest
                .iter()
                .filter_map(|d| match &d.version {
                    DeclaredVersion::Expression(e) => Some(e.clone()),
                    _ => None,
                })
                .min();
            let version = expression.map(DeclaredVersion::Expression).unwrap_or(DeclaredVersion::NotDeclared);
            out.push(Group { name: name.to_string(), version, decls: rest });
        }
    }
    out
}

/// Scope and C62 from the declaring directories (FR-015): runtime when any
/// declaration is outside a test or example directory; else Development
/// when any is in an example directory; else Test.
pub(crate) fn dir_scope(dirs: &[&Path]) -> Option<(LifecycleScope, &'static str)> {
    // Test directories compare exactly, as the package-search reader's C62
    // does (`fetches::TEST_DIRS`), so a project and a search in one directory
    // agree; example directories case-insensitively, as the census counted.
    let class = |d: &Path| {
        let segs: Vec<&str> = d.components().filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        }).collect();
        if segs.iter().any(|s| super::fetches::TEST_DIRS.contains(s)) {
            Some(LifecycleScope::Test)
        } else if segs.iter().any(|s| EXAMPLE_DIRS.contains(&s.to_ascii_lowercase().as_str())) {
            Some(LifecycleScope::Development)
        } else {
            None
        }
    };
    let classes: Vec<Option<LifecycleScope>> = dirs.iter().map(|d| class(d)).collect();
    if classes.is_empty() || classes.iter().any(Option::is_none) {
        return None;
    }
    if classes.contains(&Some(LifecycleScope::Development)) {
        Some((LifecycleScope::Development, DERIVATION_SOURCE_EXAMPLE_DIRECTORY))
    } else {
        Some((LifecycleScope::Test, DERIVATION_SOURCE_TEST_DIRECTORY))
    }
}

/// Whether a path below the scan root passes through a vendored directory
/// name (R10 `path-segment`), compared case-insensitively.
pub(crate) fn has_vendor_segment(dir: &Path) -> bool {
    dir.components().any(|c| match c {
        Component::Normal(s) => s.to_str().is_some_and(|s| VENDOR_SEGMENTS.contains(&s.to_ascii_lowercase().as_str())),
        _ => false,
    })
}

/// R10 `licence-file`: a licence, copying or notice file in `dir`.
fn has_licence_file(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_ascii_lowercase();
        let stem = n.split('.').next().unwrap_or("");
        matches!(stem, "license" | "licence" | "copying" | "notice") && e.file_type().is_ok_and(|t| t.is_file())
    })
}

/// Spec 1618 (research R10, T002): add `submodule-boundary` to C241 for a
/// project at or below a git submodule's root (`submodule_dirs` holds
/// canonical directories), and set C240 when the chosen rule holds:
/// `path-segment` and `submodule-boundary` together. Both sides are compared
/// relative to the scan root, as `declared_attach` compares them.
pub(crate) fn mark_vendored(
    components: &mut [ResolvedComponent],
    submodule_dirs: &BTreeMap<PathBuf, String>,
    scan_root: &Path,
) {
    use crate::scan_fs::declared_attach::relative;
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let subs: Vec<PathBuf> = submodule_dirs
        .keys()
        .filter_map(|d| d.to_str().and_then(|s| relative(s, scan_root, &canonical_root)))
        .collect();
    for c in components.iter_mut().filter(|c| is_cmake_project(c)) {
        let in_submodule = !subs.is_empty()
            && c.evidence
                .source_file_paths
                .iter()
                .filter(|f| Path::new(f).file_name().is_some_and(|n| n == "CMakeLists.txt"))
                .filter_map(|f| relative(f, scan_root, &canonical_root))
                .filter_map(|r| r.parent().map(Path::to_path_buf))
                .any(|d| subs.iter().any(|s| d.starts_with(s)));
        let mut signals: BTreeSet<String> = c
            .extra_annotations
            .get(ANN_VENDORED_SIGNALS)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if in_submodule {
            signals.insert(SIGNAL_SUBMODULE_BOUNDARY.to_string());
            c.extra_annotations.insert(ANN_VENDORED_SIGNALS.to_string(), serde_json::json!(signals));
        }
        if signals.contains(SIGNAL_PATH_SEGMENT) && signals.contains(SIGNAL_SUBMODULE_BOUNDARY) {
            c.extra_annotations.insert(ANN_VENDORED.to_string(), serde_json::json!(true));
        }
    }
}

/// One entry per group, with the role, tier, mechanism, evidence, scope and
/// markers of the data model. `submodule-boundary` and C240 need the
/// submodule directories and are added in `scan_fs::cmake_search_pair`.
pub(crate) fn entries(decls: &[ProjectDeclaration], scan_root: &Path) -> Vec<PackageDbEntry> {
    use waybill_common::types::purl::{encode_purl_segment, Purl};
    let mut out = Vec::new();
    for g in group(decls) {
        let version = g.version.value().unwrap_or("");
        let purl_str = if version.is_empty() {
            format!("pkg:generic/{}", encode_purl_segment(&g.name))
        } else {
            format!("pkg:generic/{}@{}", encode_purl_segment(&g.name), encode_purl_segment(version))
        };
        let Ok(purl) = Purl::new(&purl_str) else {
            tracing::debug!(name = %g.name, version, "cmake: a project's identity is not a valid PURL; skipped");
            continue;
        };
        let Some(first) = g.decls.first() else { continue };
        let mut e = super::build_cmake_entry(&g.name, version, &first.source_path, purl, None, None, false, MECHANISM_CMAKE_PROJECT);
        e.extra_source_paths = g.decls.iter().skip(1).map(|d| d.source_path.clone()).collect();
        e.extra_annotations.insert("waybill:component-role".to_string(), serde_json::json!("main-module"));
        let dirs: Vec<&Path> = g.decls.iter().map(|d| d.rel_file.parent().unwrap_or(Path::new(""))).collect();
        if let Some((scope, derivation)) = dir_scope(&dirs) {
            e.lifecycle_scope = Some(scope);
            e.extra_annotations.insert(ANN_SCOPE_DERIVATION.to_string(), serde_json::json!(derivation));
        }
        match &g.version {
            DeclaredVersion::NotDeclared => {
                e.extra_annotations.insert(ANN_VERSION_ABSENCE.to_string(), serde_json::json!("not-declared"));
            }
            DeclaredVersion::Expression(x) => {
                e.extra_annotations.insert(ANN_DECLARED_VERSION_EXPRESSION.to_string(), serde_json::json!(x));
            }
            DeclaredVersion::Literal(_) | DeclaredVersion::Resolved(_) => {}
        }
        let mut signals: BTreeSet<&str> = BTreeSet::new();
        if g.decls.iter().all(|d| d.reached == Some(false)) {
            e.extra_annotations.insert(ANN_CMAKE_BUILD_REACH.to_string(), serde_json::json!(SIGNAL_NOT_REACHED));
            signals.insert(SIGNAL_NOT_REACHED);
        }
        if dirs.iter().any(|d| has_vendor_segment(d)) {
            signals.insert(SIGNAL_PATH_SEGMENT);
        }
        if dirs.iter().any(|d| has_licence_file(&scan_root.join(d))) {
            signals.insert(SIGNAL_LICENCE_FILE);
        }
        if !signals.is_empty() {
            e.extra_annotations.insert(ANN_VENDORED_SIGNALS.to_string(), serde_json::json!(signals));
        }
        out.push(e);
    }
    out
}

/// A project's `VERSION`, as declared (data model `DeclaredVersion`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeclaredVersion {
    /// `VERSION 1.2.3`.
    Literal(String),
    /// `VERSION ${X}`, with a same-file literal `set(X …)`.
    Resolved(String),
    /// `VERSION ${X}`, unresolved; kept verbatim (C238).
    Expression(String),
    /// No `VERSION` argument (C197 `not-declared`).
    NotDeclared,
}

impl DeclaredVersion {
    /// The version a PURL carries, if any.
    pub(crate) fn value(&self) -> Option<&str> {
        match self {
            DeclaredVersion::Literal(v) | DeclaredVersion::Resolved(v) => Some(v),
            DeclaredVersion::Expression(_) | DeclaredVersion::NotDeclared => None,
        }
    }
}

/// The name (lowercased) and version of a `project()` call, or `None` when
/// the name is built from a variable (it yields nothing, as before).
pub(crate) fn parse_project(
    args: &[String],
    vars: &HashMap<String, Option<String>>,
) -> Option<(String, DeclaredVersion)> {
    let name = args.first().filter(|n| !n.is_empty() && !n.contains("${"))?;
    let version = match args.iter().position(|a| a == "VERSION").and_then(|i| args.get(i + 1)) {
        None => DeclaredVersion::NotDeclared,
        Some(v) if !v.contains("${") => DeclaredVersion::Literal(v.clone()),
        Some(v) => {
            let expanded = super::expand_known_vars(v, vars);
            if expanded.contains("${") || expanded.is_empty() {
                DeclaredVersion::Expression(v.clone())
            } else {
                DeclaredVersion::Resolved(expanded)
            }
        }
    };
    Some((name.to_lowercase(), version))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    fn vars(pairs: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.map(str::to_string))).collect()
    }

    #[test]
    fn s1618_literal_version() {
        assert_eq!(
            parse_project(&args("Waybill-Fixture VERSION 1.2.3 LANGUAGES CXX"), &vars(&[])),
            Some(("waybill-fixture".to_string(), DeclaredVersion::Literal("1.2.3".to_string())))
        );
    }

    #[test]
    fn s1618_version_after_languages() {
        assert_eq!(
            parse_project(&args("n LANGUAGES CXX VERSION 2"), &vars(&[])),
            Some(("n".to_string(), DeclaredVersion::Literal("2".to_string())))
        );
    }

    #[test]
    fn s1618_no_version_and_languages_only() {
        assert_eq!(parse_project(&args("n"), &vars(&[])), Some(("n".to_string(), DeclaredVersion::NotDeclared)));
        assert_eq!(
            parse_project(&args("n C CXX"), &vars(&[])),
            Some(("n".to_string(), DeclaredVersion::NotDeclared))
        );
    }

    #[test]
    fn s1618_variable_version_resolved_by_a_same_file_set() {
        assert_eq!(
            parse_project(&args("n VERSION ${V}"), &vars(&[("V", Some("3.1.4"))])),
            Some(("n".to_string(), DeclaredVersion::Resolved("3.1.4".to_string())))
        );
    }

    #[test]
    fn s1618_unresolved_and_ambiguous_variables_stay_expressions() {
        assert_eq!(
            parse_project(&args("n VERSION ${FROM_ELSEWHERE}"), &vars(&[])),
            Some(("n".to_string(), DeclaredVersion::Expression("${FROM_ELSEWHERE}".to_string())))
        );
        // `set()` twice with different values: ambiguous, so unresolved.
        assert_eq!(
            parse_project(&args("n VERSION ${V}"), &vars(&[("V", None)])),
            Some(("n".to_string(), DeclaredVersion::Expression("${V}".to_string())))
        );
        // Partly resolved is still an expression, kept as written.
        assert_eq!(
            parse_project(&args("n VERSION ${A}.${B}"), &vars(&[("A", Some("1"))])),
            Some(("n".to_string(), DeclaredVersion::Expression("${A}.${B}".to_string())))
        );
    }

    #[test]
    fn s1618_a_name_from_a_variable_yields_nothing() {
        assert_eq!(parse_project(&args("${PROJECT} VERSION 1"), &vars(&[])), None);
        assert_eq!(parse_project(&[], &vars(&[])), None);
    }

    fn decl(name: &str, version: DeclaredVersion, rel: &str) -> ProjectDeclaration {
        ProjectDeclaration {
            name: name.into(),
            version,
            rel_file: PathBuf::from(rel),
            source_path: format!("/root/{rel}"),
            reached: None,
        }
    }

    fn lit(v: &str) -> DeclaredVersion {
        DeclaredVersion::Literal(v.into())
    }

    fn summary(g: &[Group]) -> Vec<(String, DeclaredVersion, Vec<String>)> {
        g.iter()
            .map(|g| (g.name.clone(), g.version.clone(), g.decls.iter().map(|d| d.rel_file.display().to_string()).collect()))
            .collect()
    }

    #[test]
    fn s1618_group_merge_rules() {
        let g = group(&[
            decl("a", lit("1"), "y/CMakeLists.txt"),
            decl("a", lit("1"), "x/CMakeLists.txt"),
            decl("b", lit("1"), "b1/CMakeLists.txt"),
            decl("b", lit("2"), "b2/CMakeLists.txt"),
            decl("b", DeclaredVersion::NotDeclared, "b3/CMakeLists.txt"),
            decl("c", DeclaredVersion::NotDeclared, "c1/CMakeLists.txt"),
            decl("c", DeclaredVersion::Expression("${Z}".into()), "c2/CMakeLists.txt"),
            decl("c", DeclaredVersion::Expression("${Y}".into()), "c3/CMakeLists.txt"),
            decl("d", DeclaredVersion::NotDeclared, "d/CMakeLists.txt"),
        ]);
        assert_eq!(
            summary(&g),
            vec![
                ("a".into(), lit("1"), vec!["x/CMakeLists.txt".into(), "y/CMakeLists.txt".into()]),
                ("b".into(), lit("1"), vec!["b1/CMakeLists.txt".into(), "b3/CMakeLists.txt".into()]),
                ("b".into(), lit("2"), vec!["b2/CMakeLists.txt".into()]),
                ("c".into(), DeclaredVersion::Expression("${Y}".into()), vec!["c1/CMakeLists.txt".into(), "c2/CMakeLists.txt".into(), "c3/CMakeLists.txt".into()]),
                ("d".into(), DeclaredVersion::NotDeclared, vec!["d/CMakeLists.txt".into()]),
            ]
        );
        // A resolved and a literal declaration of one version are one group.
        let g = group(&[decl("e", lit("3"), "e1/CMakeLists.txt"), decl("e", DeclaredVersion::Resolved("3".into()), "e2/CMakeLists.txt")]);
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn s1618_dir_scope() {
        let p = |s: &'static str| Path::new(s);
        assert_eq!(dir_scope(&[p("tests/t")]), Some((LifecycleScope::Test, "source-test-directory")));
        assert_eq!(dir_scope(&[p("a/unittests")]), Some((LifecycleScope::Test, "source-test-directory")));
        assert_eq!(dir_scope(&[p("examples/e")]), Some((LifecycleScope::Development, "source-example-directory")));
        assert_eq!(dir_scope(&[p("x/docs/integration/y")]), Some((LifecycleScope::Development, "source-example-directory")));
        assert_eq!(dir_scope(&[p("samples/s"), p("tests/t")]), Some((LifecycleScope::Development, "source-example-directory")));
        assert_eq!(dir_scope(&[p("lib"), p("tests/t")]), None, "one runtime declaration keeps it runtime");
        assert_eq!(dir_scope(&[p("")]), None);
        assert_eq!(dir_scope(&[p("contest/x")]), None, "whole segments only");
    }

    #[test]
    fn s1618_vendor_segment() {
        assert!(has_vendor_segment(Path::new("third_party/zlib")));
        assert!(has_vendor_segment(Path::new("a/External/b")));
        assert!(!has_vendor_segment(Path::new("a/externals/b")));
        assert!(!has_vendor_segment(Path::new("src")));
    }

    #[test]
    fn s1618_entries_markers() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("third_party/v")).unwrap();
        std::fs::write(root.path().join("third_party/v/LICENSE"), "x").unwrap();
        let mut unreached = decl("u", lit("1"), "u/CMakeLists.txt");
        unreached.reached = Some(false);
        let es = entries(
            &[
                decl("n", DeclaredVersion::NotDeclared, "n/CMakeLists.txt"),
                decl("x", DeclaredVersion::Expression("${V}".into()), "x/CMakeLists.txt"),
                decl("v", lit("2.0"), "third_party/v/CMakeLists.txt"),
                unreached,
                decl("t", lit("1"), "tests/t/CMakeLists.txt"),
            ],
            root.path(),
        );
        let by = |n: &str| es.iter().find(|e| e.name == n).unwrap();
        let ann = |n: &str, k: &str| by(n).extra_annotations.get(k).cloned();
        assert_eq!(by("n").purl.as_str(), "pkg:generic/n");
        assert_eq!(ann("n", "waybill:version-absence"), Some(serde_json::json!("not-declared")));
        assert_eq!(ann("x", ANN_DECLARED_VERSION_EXPRESSION), Some(serde_json::json!("${V}")));
        assert_eq!(ann("x", "waybill:version-absence"), None);
        assert_eq!(by("v").purl.as_str(), "pkg:generic/v@2.0");
        assert_eq!(ann("v", ANN_VENDORED_SIGNALS), Some(serde_json::json!(["licence-file", "path-segment"])));
        assert_eq!(ann("v", ANN_VENDORED), None, "C240 needs a submodule boundary too");
        assert_eq!(ann("u", ANN_CMAKE_BUILD_REACH), Some(serde_json::json!("not-reached")));
        assert_eq!(ann("u", ANN_VENDORED_SIGNALS), Some(serde_json::json!(["not-reached"])));
        assert_eq!(ann("n", ANN_CMAKE_BUILD_REACH), None, "reach undefined without a root listfile");
        assert_eq!(by("t").lifecycle_scope, Some(LifecycleScope::Test));
        for e in &es {
            assert_eq!(e.extra_annotations.get("waybill:component-role"), Some(&serde_json::json!("main-module")));
            assert_eq!(e.extra_annotations.get("waybill:source-mechanism"), Some(&serde_json::json!("cmake-project")));
            assert_eq!(e.sbom_tier.as_deref(), Some("source"));
        }
    }

    #[test]
    fn s1618_version_keyword_without_a_value() {
        assert_eq!(
            parse_project(&args("n VERSION"), &vars(&[])),
            Some(("n".to_string(), DeclaredVersion::NotDeclared))
        );
    }
}
