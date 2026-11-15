//! Test scope for an installed tree's test payload (spec 1533, #1564).
//!
//! A post-pass over the finished component list. It runs in `scan_cmd`
//! after file-tier emission and before `--exclude-scope`, which is after
//! scope propagation, so a scope it sets can never seed propagation (FR-007,
//! research R2). A component is scoped only by a rule matching every one of
//! its own paths (research R6).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use waybill_common::resolution::{LifecycleScope, ResolvedComponent};

/// C226: the document-scope coverage record.
pub(crate) const ANN_TEST_SCOPE_COVERAGE: &str = "waybill:test-scope-coverage";

/// What made a file test payload. Ordered strongest first, so the minimum
/// of several is the strongest and the maximum the weakest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TestRule {
    ExportedTargetsTestPackage,
    CtestDeclaration,
    TestDirectory,
}

impl TestRule {
    pub(crate) const ALL: [TestRule; 3] =
        [TestRule::CtestDeclaration, TestRule::ExportedTargetsTestPackage, TestRule::TestDirectory];

    /// C62 `waybill:lifecycle-scope-derivation` value.
    pub(crate) fn c62(self) -> &'static str {
        match self {
            Self::ExportedTargetsTestPackage => "exported-targets-test-package",
            Self::CtestDeclaration => "ctest-declaration",
            Self::TestDirectory => "test-directory",
        }
    }
}

/// FR-005: directory names matched exactly.
pub(crate) const TEST_DIRECTORIES: [&str; 5] = ["test", "tests", "unittests", "test_plugins", ".upstream-tests"];

/// FR-015: whether path rules run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstalledTree {
    Image,
    Recognised,
    NotRecognised,
}

impl InstalledTree {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Recognised => "recognised",
            Self::NotRecognised => "not-recognised",
        }
    }

    fn path_rules(self) -> bool {
        !matches!(self, Self::NotRecognised)
    }
}

/// Declared locations by outcome, and declaring files by kind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DeclarationCoverage {
    pub(crate) ctest: usize,
    pub(crate) exported_targets: usize,
    pub(crate) owned_by_os_package: usize,
    pub(crate) resolved: usize,
    pub(crate) not_shipped: usize,
    pub(crate) unresolved: usize,
    pub(crate) outside_root: usize,
}

/// What the declarations name (data-model "Declared locations").
#[derive(Debug, Default, Clone)]
pub(crate) struct TestDeclarations {
    /// Root-relative path → the strongest declaring rule.
    pub(crate) paths: BTreeMap<PathBuf, TestRule>,
    /// Installed-library components of declared test packages.
    pub(crate) package_purls: BTreeSet<String>,
    pub(crate) coverage: DeclarationCoverage,
}

impl TestDeclarations {
    /// Record `rule` for `rel`, keeping the strongest.
    pub(crate) fn declare(&mut self, rel: PathBuf, rule: TestRule) {
        self.paths.entry(rel).and_modify(|r| *r = (*r).min(rule)).or_insert(rule);
    }

    fn read_any(&self) -> bool {
        self.coverage.ctest + self.coverage.exported_targets + self.coverage.owned_by_os_package > 0
    }
}

/// Everything the pass needs from `scan_path`.
#[derive(Debug, Clone)]
pub(crate) struct Inputs {
    pub(crate) installed_tree: InstalledTree,
    pub(crate) declarations: TestDeclarations,
    /// The OS package databases' claims (FR-013).
    pub(crate) os_claimed_paths: HashSet<PathBuf>,
    #[cfg(unix)]
    pub(crate) os_claimed_inodes: HashSet<(u64, u64)>,
    /// `--include-test-binaries` (FR-012).
    pub(crate) include_test_binaries: bool,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            installed_tree: InstalledTree::NotRecognised,
            declarations: TestDeclarations::default(),
            os_claimed_paths: HashSet::new(),
            #[cfg(unix)]
            os_claimed_inodes: HashSet::new(),
            include_test_binaries: false,
        }
    }
}

/// C226 (contracts/wire.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestScopeCoverage {
    pub(crate) installed_tree: InstalledTree,
    pub(crate) declarations: DeclarationCoverage,
    pub(crate) scoped: BTreeMap<TestRule, usize>,
}

impl TestScopeCoverage {
    /// Sorted-key JSON, or `None` when nothing was read and nothing scoped
    /// (FR-011).
    pub(crate) fn wire(&self, declarations_read: bool) -> Option<String> {
        if !declarations_read && self.scoped.values().all(|n| *n == 0) {
            return None;
        }
        let d = &self.declarations;
        let scoped: serde_json::Map<String, serde_json::Value> = TestRule::ALL
            .iter()
            .map(|r| (r.c62().to_string(), serde_json::json!(self.scoped.get(r).copied().unwrap_or(0))))
            .collect();
        let v = serde_json::json!({
            "declarations": {
                "ctest": d.ctest,
                "exported-targets": d.exported_targets,
                "owned-by-os-package": d.owned_by_os_package,
            },
            "installed-tree": self.installed_tree.as_str(),
            "locations": {
                "not-shipped": d.not_shipped,
                "outside-root": d.outside_root,
                "resolved": d.resolved,
                "unresolved": d.unresolved,
            },
            "scoped": scoped,
        });
        // serde_json's map is a BTreeMap here (no `preserve_order`), so keys
        // serialise sorted.
        Some(v.to_string())
    }
}

/// Which components this feature may scope (FR-008).
enum Kind {
    /// A binary file component: its own file.
    BinaryFile,
    /// A file-tier component: every path its content was seen at.
    FileTier,
    /// A #1531 installed-library component.
    InstalledLibrary,
}

fn kind(c: &ResolvedComponent) -> Option<Kind> {
    if c.binary_class.is_some() && c.binary_role.is_some() {
        return Some(Kind::BinaryFile);
    }
    let ann = |k: &str| c.extra_annotations.get(k).and_then(|v| v.as_str());
    if ann(crate::scan_fs::file_tier::COMPONENT_TIER_KEY) == Some(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE) {
        return Some(Kind::FileTier);
    }
    if matches!(ann("waybill:source-mechanism"), Some("pkg-config" | "cmake-package-version")) {
        return Some(Kind::InstalledLibrary);
    }
    None
}

/// `path` relative to whichever of `roots` it lies under.
fn root_relative(path: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(path);
    if p.is_relative() {
        return Some(p.to_path_buf());
    }
    roots.iter().find_map(|r| p.strip_prefix(r).ok().map(Path::to_path_buf))
}

fn under_directory(rel: &Path, names: &[&str]) -> bool {
    let Some(parent) = rel.parent() else { return false };
    parent.components().any(|c| c.as_os_str().to_str().is_some_and(|s| names.contains(&s)))
}

/// The rule `rel` matches, if any: a declaration first, then a directory.
fn rule_for(rel: &Path, inputs: &Inputs, binary: bool) -> Option<TestRule> {
    if let Some(r) = inputs.declarations.paths.get(rel) {
        return Some(*r);
    }
    if inputs.installed_tree.path_rules() && under_directory(rel, &TEST_DIRECTORIES) {
        return Some(TestRule::TestDirectory);
    }
    // FR-012: #1524's own set, only for the binaries it would have skipped.
    if binary && inputs.include_test_binaries && under_directory(rel, &crate::scan_fs::binary::TEST_INPUT_DIRS) {
        return Some(TestRule::TestDirectory);
    }
    None
}

/// FR-013. File-tier paths are root-relative, binary paths absolute; a
/// claim is checked against the absolute form under the canonical root.
fn os_owned(path: &str, inputs: &Inputs, roots: &[PathBuf]) -> bool {
    #[cfg(unix)]
    let none = inputs.os_claimed_paths.is_empty() && inputs.os_claimed_inodes.is_empty();
    #[cfg(not(unix))]
    let none = inputs.os_claimed_paths.is_empty();
    if none {
        return false;
    }
    let p = Path::new(path);
    let abs = match roots.first() {
        Some(root) if p.is_relative() => root.join(p),
        _ => p.to_path_buf(),
    };
    crate::scan_fs::binary::is_path_claimed(
        &abs,
        &inputs.os_claimed_paths,
        #[cfg(unix)]
        &inputs.os_claimed_inodes,
    )
}

/// The pass. `roots` are the scan root canonicalised and as given (#1084).
/// Returns C226's wire value, `None` when there is nothing to report.
///
/// The document subject is never one of the kinds this pass reads: the root
/// selector picks main modules, and `--root-name` synthesises its own root.
pub(crate) fn apply(components: &mut [ResolvedComponent], inputs: &Inputs, roots: &[PathBuf]) -> Option<String> {
    let mut scoped: BTreeMap<TestRule, usize> = BTreeMap::new();
    for c in components.iter_mut() {
        if c.lifecycle_scope.is_some() {
            continue;
        }
        let Some(kind) = kind(c) else { continue };
        let rule = match kind {
            Kind::InstalledLibrary => inputs
                .declarations
                .package_purls
                .contains(c.purl.as_str())
                .then_some(TestRule::ExportedTargetsTestPackage),
            Kind::BinaryFile | Kind::FileTier => {
                let truncated = c.extra_annotations.contains_key(crate::scan_fs::file_tier::FILE_PATHS_TRUNCATED_KEY);
                let paths = &c.evidence.source_file_paths;
                if truncated || paths.is_empty() {
                    None
                } else {
                    // Every path must match; the weakest witness is recorded.
                    // Ownership is checked last: it costs a stat per path.
                    paths
                        .iter()
                        .map(|p| {
                            root_relative(p, roots)
                                .and_then(|rel| rule_for(&rel, inputs, matches!(kind, Kind::BinaryFile)))
                        })
                        .collect::<Option<Vec<TestRule>>>()
                        .and_then(|rules| rules.into_iter().max())
                        .filter(|_| !paths.iter().any(|p| os_owned(p, inputs, roots)))
                }
            }
        };
        let Some(rule) = rule else { continue };
        c.lifecycle_scope = Some(LifecycleScope::Test);
        c.extra_annotations.insert(
            crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION.to_string(),
            serde_json::Value::String(rule.c62().to_string()),
        );
        *scoped.entry(rule).or_default() += 1;
    }
    let total: usize = scoped.values().sum();
    if total > 0 {
        tracing::info!(
            scoped = total,
            installed_tree = inputs.installed_tree.as_str(),
            "test scope: installed-tree test payload scoped (spec 1533)"
        );
    }
    TestScopeCoverage {
        installed_tree: inputs.installed_tree,
        declarations: inputs.declarations.coverage.clone(),
        scoped,
    }
    .wire(inputs.declarations.read_any())
}

/// Whether `c` was scoped by this feature (SPDX 3 purpose, R8).
pub(crate) fn scoped_by_this_feature(c: &ResolvedComponent) -> bool {
    c.lifecycle_scope == Some(LifecycleScope::Test)
        && c
            .extra_annotations
            .get(crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION)
            .and_then(|v| v.as_str())
            .is_some_and(|v| TestRule::ALL.iter().any(|r| r.c62() == v))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::openvex::tests::mk_component;
    use waybill_common::resolution::BinaryRole;

    const ROOT: &str = "/scan-root";

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from(ROOT)]
    }

    fn binary(purl: &str, rel: &str) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.binary_class = Some("elf".into());
        c.binary_role = Some(BinaryRole::Application);
        c.evidence.source_file_paths = vec![format!("{ROOT}/{rel}")];
        c
    }

    fn file_tier(purl: &str, rels: &[&str]) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.extra_annotations.insert(
            crate::scan_fs::file_tier::COMPONENT_TIER_KEY.into(),
            serde_json::json!(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE),
        );
        c.evidence.source_file_paths = rels.iter().map(|r| format!("{ROOT}/{r}")).collect();
        c
    }

    fn installed(tree: InstalledTree) -> Inputs {
        Inputs { installed_tree: tree, ..Inputs::default() }
    }

    fn c62(c: &ResolvedComponent) -> Option<&str> {
        c.extra_annotations.get(crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION).and_then(|v| v.as_str())
    }

    #[test]
    fn strength_order_and_c62_values() {
        assert!(TestRule::ExportedTargetsTestPackage < TestRule::CtestDeclaration);
        assert!(TestRule::CtestDeclaration < TestRule::TestDirectory);
        let mut d = TestDeclarations::default();
        d.declare("bin/t".into(), TestRule::CtestDeclaration);
        d.declare("bin/t".into(), TestRule::ExportedTargetsTestPackage);
        d.declare("bin/t".into(), TestRule::CtestDeclaration);
        assert_eq!(d.paths[Path::new("bin/t")], TestRule::ExportedTargetsTestPackage);
        let values: Vec<&str> = TestRule::ALL.iter().map(|r| r.c62()).collect();
        assert_eq!(values, ["ctest-declaration", "exported-targets-test-package", "test-directory"]);
    }

    /// FR-011: nothing read and nothing scoped → no C226.
    #[test]
    fn the_wire_record_and_its_absence() {
        let mut cs = vec![binary("pkg:generic/a?file-sha256=1", "bin/a")];
        assert_eq!(apply(&mut cs, &installed(InstalledTree::NotRecognised), &roots()), None);
        let mut inputs = installed(InstalledTree::Recognised);
        inputs.declarations.coverage.ctest = 1;
        let wire = apply(&mut cs, &inputs, &roots()).unwrap();
        assert_eq!(
            wire,
            r#"{"declarations":{"ctest":1,"exported-targets":0,"owned-by-os-package":0},"installed-tree":"recognised","locations":{"not-shipped":0,"outside-root":0,"resolved":0,"unresolved":0},"scoped":{"ctest-declaration":0,"exported-targets-test-package":0,"test-directory":0}}"#
        );
    }

    #[test]
    fn a_declaration_scopes_in_any_tree() {
        let mut inputs = installed(InstalledTree::NotRecognised);
        inputs.declarations.declare("bin/t".into(), TestRule::CtestDeclaration);
        let mut cs = vec![binary("pkg:generic/t?file-sha256=1", "bin/t"), binary("pkg:generic/p?file-sha256=2", "bin/p")];
        apply(&mut cs, &inputs, &roots());
        assert_eq!(cs[0].lifecycle_scope, Some(LifecycleScope::Test));
        assert_eq!(c62(&cs[0]), Some("ctest-declaration"));
        assert_eq!(cs[1].lifecycle_scope, None);
    }

    /// FR-005 and FR-015: exact names, and only in an installed tree.
    #[test]
    fn test_directories_match_exactly_and_only_when_installed() {
        let mk = || {
            vec![
                file_tier("pkg:generic/f1", &["share/p/tests/in.dat"]),
                file_tier("pkg:generic/f2", &["lib/test_plugins/x.so"]),
                file_tier("pkg:generic/f3", &["lib/x/.upstream-tests/t.cpp"]),
                file_tier("pkg:generic/f4", &["share/p/testing/in.dat"]),
                file_tier("pkg:generic/f5", &["share/attestation/x"]),
                file_tier("pkg:generic/f6", &["share/contest/x"]),
                file_tier("pkg:generic/f7", &["tests"]),
            ]
        };
        let mut cs = mk();
        apply(&mut cs, &installed(InstalledTree::Recognised), &roots());
        let scoped: Vec<bool> = cs.iter().map(|c| c.lifecycle_scope.is_some()).collect();
        assert_eq!(scoped, [true, true, true, false, false, false, false]);
        assert_eq!(c62(&cs[0]), Some("test-directory"));
        let mut cs = mk();
        assert_eq!(apply(&mut cs, &installed(InstalledTree::NotRecognised), &roots()), None);
        assert!(cs.iter().all(|c| c.lifecycle_scope.is_none()));
    }

    /// Research R6: every path must match, and the weakest witness is recorded.
    #[test]
    fn every_path_must_match() {
        let mut inputs = installed(InstalledTree::Image);
        inputs.declarations.declare("bin/t".into(), TestRule::CtestDeclaration);
        let mut cs = vec![
            file_tier("pkg:generic/both", &["lib/x.dat", "share/p/tests/x.dat"]),
            file_tier("pkg:generic/weakest", &["bin/t", "share/p/tests/t"]),
        ];
        apply(&mut cs, &inputs, &roots());
        assert_eq!(cs[0].lifecycle_scope, None);
        assert_eq!(c62(&cs[1]), Some("test-directory"));
    }

    #[test]
    fn what_is_never_scoped() {
        let mut inputs = installed(InstalledTree::Image);
        inputs.declarations.declare("bin/t".into(), TestRule::CtestDeclaration);
        // Already scoped by its reader: untouched.
        let mut pre = binary("pkg:generic/t?file-sha256=1", "bin/t");
        pre.lifecycle_scope = Some(LifecycleScope::Development);
        // A truncated file-tier path list: not every path is known.
        let mut trunc = file_tier("pkg:generic/f", &["share/p/tests/x"]);
        trunc.extra_annotations.insert(crate::scan_fs::file_tier::FILE_PATHS_TRUNCATED_KEY.into(), serde_json::json!("true"));
        // An identity found inside a binary, and a package component.
        let mut embedded = mk_component("pkg:generic/zlib@1.3");
        embedded.evidence.source_file_paths = vec![format!("{ROOT}/bin/t")];
        let mut npm = mk_component("pkg:npm/x@1");
        npm.evidence.source_file_paths = vec![format!("{ROOT}/share/p/tests/x")];
        let mut cs = vec![pre, trunc, embedded, npm];
        apply(&mut cs, &inputs, &roots());
        assert_eq!(cs[0].lifecycle_scope, Some(LifecycleScope::Development));
        assert!(c62(&cs[0]).is_none());
        assert!(cs[1..].iter().all(|c| c.lifecycle_scope.is_none()));
    }

    #[test]
    fn a_declared_test_package_component_is_scoped_by_purl() {
        let mut inputs = installed(InstalledTree::NotRecognised);
        inputs.declarations.package_purls.insert("pkg:generic/p-tests@1".into());
        let mk = |purl: &str| {
            let mut c = mk_component(purl);
            c.extra_annotations.insert("waybill:source-mechanism".into(), serde_json::json!("cmake-package-version"));
            c
        };
        let mut cs = vec![mk("pkg:generic/p-tests@1"), mk("pkg:generic/p@1")];
        apply(&mut cs, &inputs, &roots());
        assert_eq!(c62(&cs[0]), Some("exported-targets-test-package"));
        assert_eq!(cs[1].lifecycle_scope, None);
    }

    /// FR-012: #1524's set, binaries only, recognition not required.
    #[test]
    fn include_test_binaries_scopes_what_1524_skips() {
        let mut inputs = installed(InstalledTree::NotRecognised);
        inputs.include_test_binaries = true;
        let mut cs = vec![
            binary("pkg:generic/a?file-sha256=1", "llvm/test/Inputs/core"),
            binary("pkg:generic/b?file-sha256=2", "src/Inputs/x.o"),
            file_tier("pkg:generic/f", &["llvm/test/x.ll"]),
        ];
        apply(&mut cs, &inputs, &roots());
        assert_eq!(c62(&cs[0]), Some("test-directory"));
        assert_eq!(c62(&cs[1]), Some("test-directory"));
        assert_eq!(cs[2].lifecycle_scope, None);
    }

    /// FR-013.
    #[test]
    fn an_os_owned_file_is_not_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("usr/share/p/tests")).unwrap();
        let owned = root.join("usr/share/p/tests/a");
        let free = root.join("usr/share/p/tests/b");
        std::fs::write(&owned, b"a").unwrap();
        std::fs::write(&free, b"b").unwrap();
        let mut inputs = installed(InstalledTree::Image);
        inputs.os_claimed_paths.insert(owned.clone());
        // File-tier paths are root-relative, as the orphan walker writes them.
        let mk = |p: &Path, purl: &str| {
            let mut c = file_tier(purl, &[]);
            c.evidence.source_file_paths = vec![p.strip_prefix(&root).unwrap().to_string_lossy().into_owned()];
            c
        };
        let mut cs = vec![mk(&owned, "pkg:generic/a"), mk(&free, "pkg:generic/b")];
        apply(&mut cs, &inputs, &[root]);
        assert_eq!(cs[0].lifecycle_scope, None);
        assert_eq!(cs[1].lifecycle_scope, Some(LifecycleScope::Test));
    }
}
