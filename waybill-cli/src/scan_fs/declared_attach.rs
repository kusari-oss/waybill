//! Spec 1537 (#1566, #1538), research R3–R4: a package a pip requirements
//! file declares is a direct dependency of the project that declares it.
//!
//! Until now such a package had no declared edge at all. The pip reader
//! builds it with an empty `depends` and nothing names it, so it reached the
//! root only through the "attach graph-tops to a root with no edges"
//! fallback. Anything that gave the root an edge stranded it: a git
//! submodule (#1566), or a `pyproject.toml` next to the file with a
//! dependency of its own (research R1, arm c3).
//!
//! The declaring project of a file is found by its location:
//!
//! - **At the scan root:** the document's subject (FR-002). The root is
//!   chosen per format at emit time, so these become *declared root
//!   anchors*, installed with the reader anchors (`cli/scan_cmd.rs`).
//! - **In a nested directory:** the nearest enclosing project below the
//!   scan root (FR-003). A project is a main module of any ecosystem or a
//!   git submodule. In one directory, a `pkg:pypi` main module wins, then
//!   any other main module (lowest PURL), then a submodule. CMake, Conan
//!   and vcpkg emit no project component, so their directories own nothing.
//! - **No project below the scan root:** the root again.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};

use waybill_common::resolution::{
    EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent,
};

/// Owner rank within one directory (lower wins), research R3.
const RANK_PYPI_MAIN_MODULE: u8 = 0;
const RANK_OTHER_MAIN_MODULE: u8 = 1;
const RANK_SUBMODULE: u8 = 2;

/// `p` as a path relative to the scan root, without `.` or `..`. `None`
/// when it is absolute outside the root or escapes it.
fn relative(p: &str, scan_root: &Path, canonical_root: &Path) -> Option<PathBuf> {
    // Main-module readers write `path+file://<absolute path>` (the
    // workspace-member tagger reads the same form).
    let path = Path::new(p.strip_prefix("path+file://").unwrap_or(p));
    let rel = if path.is_absolute() {
        path.strip_prefix(canonical_root)
            .or_else(|_| path.strip_prefix(scan_root))
            .ok()?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };
    let mut out = PathBuf::new();
    for c in rel.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn is_main_module(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get("waybill:component-role")
        .and_then(|v| v.as_str())
        == Some("main-module")
}

/// The pip reader's own file test (`pip/mod.rs`: `requirements*.txt`).
fn is_requirements_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("requirements") && n.ends_with(".txt"))
}

/// Directory (relative to the scan root) → the project that owns it.
#[derive(Debug, Default)]
pub(crate) struct OwnerIndex {
    by_dir: BTreeMap<PathBuf, String>,
}

impl OwnerIndex {
    /// `submodule_dirs` maps each submodule's absolute, canonical directory
    /// to its PURL (`git_submodule::SubmoduleDirs`).
    pub(crate) fn build(
        components: &[ResolvedComponent],
        submodule_dirs: &BTreeMap<PathBuf, String>,
        scan_root: &Path,
        mode: crate::scan_fs::ScanMode,
    ) -> Self {
        let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
        let mut best: BTreeMap<PathBuf, (u8, String)> = BTreeMap::new();
        let mut offer = |dir: PathBuf, rank: u8, purl: &str| {
            if dir.as_os_str().is_empty() {
                return; // the scan root is FR-002's, not an owner
            }
            let cand = (rank, purl.to_string());
            match best.get(&dir) {
                Some(cur) if *cur <= cand => {}
                _ => {
                    best.insert(dir, cand);
                }
            }
        };
        for c in components.iter().filter(|c| is_main_module(c)) {
            // A main module's project directory is what the
            // `waybill:workspace-member` annotation reports, from the same
            // function: `path+file://<abs dir>` (pip, npm, cargo) or a
            // manifest path's directory. `.` is the scan root.
            let Some(first) = c.evidence.source_file_paths.first() else { continue };
            let Some(ws) = crate::scan_fs::workspace_root::derive_workspace_root(first, &canonical_root, mode) else { continue };
            let dir = if ws == "." { PathBuf::new() } else { PathBuf::from(ws) };
            let rank = if c.purl.ecosystem() == "pypi" { RANK_PYPI_MAIN_MODULE } else { RANK_OTHER_MAIN_MODULE };
            offer(dir, rank, c.purl.as_str());
        }
        for (dir, purl) in submodule_dirs {
            let Some(s) = dir.to_str() else { continue };
            let Some(rel) = relative(s, scan_root, &canonical_root) else { continue };
            offer(rel, RANK_SUBMODULE, purl);
        }
        let by_dir: BTreeMap<PathBuf, String> = best.into_iter().map(|(d, (_, p))| (d, p)).collect();
        tracing::debug!(owners = ?by_dir, "declared requirements: owner index");
        Self { by_dir }
    }

    /// The nearest owner of a file, walking up from its directory to, but
    /// not including, the scan root.
    fn owner_of(&self, file: &Path) -> Option<&str> {
        let dir = file.parent()?;
        dir.ancestors()
            .take_while(|d| !d.as_os_str().is_empty())
            .find_map(|d| self.by_dir.get(d).map(String::as_str))
    }
}

/// What the pass decided.
#[derive(Debug, Default)]
pub(crate) struct DeclaredAttachment {
    /// `owner DependsOn package`, typed later by the lifecycle-scope pass.
    pub edges: Vec<Relationship>,
    /// Packages declared at the scan root, or with no owner below it.
    pub root_anchors: BTreeSet<String>,
}

/// One candidate per declaring file (data-model V2–V4).
pub(crate) fn attach(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    index: &OwnerIndex,
    scan_root: &Path,
) -> DeclaredAttachment {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut seen: HashSet<(String, String)> = relationships
        .iter()
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();
    let mut out = DeclaredAttachment::default();
    for c in components {
        if c.purl.ecosystem() != "pypi" || is_main_module(c) {
            continue;
        }
        for p in &c.evidence.source_file_paths {
            let Some(rel) = relative(p, scan_root, &canonical_root) else { continue };
            if !is_requirements_file(&rel) {
                continue;
            }
            match index.owner_of(&rel) {
                Some(owner) if owner == c.purl.as_str() => {}
                Some(owner) => {
                    if seen.insert((owner.to_string(), c.purl.as_str().to_string())) {
                        out.edges.push(Relationship {
                            from: owner.to_string(),
                            to: c.purl.as_str().to_string(),
                            relationship_type: RelationshipType::DependsOn,
                            provenance: EnrichmentProvenance {
                                source: rel.to_string_lossy().into_owned(),
                                data_type: "declared-requirement".to_string(),
                            },
                        });
                    }
                }
                None => {
                    out.root_anchors.insert(c.purl.as_str().to_string());
                }
            }
        }
    }
    tracing::debug!(
        edges = out.edges.len(),
        root_anchors = out.root_anchors.len(),
        "declared requirements: attached"
    );
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::mk_component;

    fn declared(purl: &str, files: &[&str]) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.evidence.source_file_paths = files.iter().map(|f| f.to_string()).collect();
        c
    }

    fn main_module(purl: &str, manifest: &str) -> ResolvedComponent {
        let mut c = declared(purl, &[manifest]);
        c.extra_annotations
            .insert("waybill:component-role".to_string(), serde_json::json!("main-module"));
        c
    }

    const A: &str = "pkg:pypi/waybill-fixture-a@1.0.0";
    const ROOT: &str = "/nonexistent-waybill-fixture-root";

    fn run(comps: &[ResolvedComponent], subs: &[(&str, &str)]) -> DeclaredAttachment {
        let dirs: BTreeMap<PathBuf, String> =
            subs.iter().map(|(d, p)| (Path::new(ROOT).join(d), p.to_string())).collect();
        let index = OwnerIndex::build(comps, &dirs, Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        attach(comps, &[], &index, Path::new(ROOT))
    }

    fn edges(d: &DeclaredAttachment) -> Vec<(String, String)> {
        d.edges.iter().map(|r| (r.from.clone(), r.to.clone())).collect()
    }

    #[test]
    fn a_file_at_the_scan_root_is_a_root_anchor_even_beside_a_main_module() {
        let comps = [main_module("pkg:pypi/waybill-fixture-app@1.0.0", "pyproject.toml"), declared(A, &["requirements.txt"])];
        let d = run(&comps, &[]);
        assert!(d.edges.is_empty());
        assert_eq!(d.root_anchors, BTreeSet::from([A.to_string()]));
    }

    /// Main-module readers record the project directory as a
    /// `path+file://<abs dir>` URI, not a manifest path (measured: npm and
    /// pip at the scan stage). That directory is the owner; the scan root's
    /// own project is never one (FR-002).
    #[test]
    fn a_main_module_recorded_by_its_directory_owns_that_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tools")).unwrap();
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let app_src = format!("path+file://{}", root.display());
        let tools_src = format!("path+file://{}", root.join("tools").display());
        let comps = [
            main_module("pkg:pypi/waybill-fixture-app@1.0.0", &app_src),
            main_module(tools, &tools_src),
            declared(A, &["tools/requirements.txt"]),
        ];
        let index = OwnerIndex::build(&comps, &BTreeMap::new(), tmp.path(), crate::scan_fs::ScanMode::Path);
        let d = attach(&comps, &[], &index, tmp.path());
        assert_eq!(edges(&d), vec![(tools.to_string(), A.to_string())]);
        assert!(d.root_anchors.is_empty(), "`.` is the scan root, never an owner");
    }

    #[test]
    fn a_nested_file_with_no_owner_is_a_root_anchor() {
        let d = run(&[declared(A, &["tools/requirements.txt"])], &[]);
        assert_eq!(d.root_anchors, BTreeSet::from([A.to_string()]));
    }

    #[test]
    fn a_nested_file_goes_to_the_main_module_of_its_directory() {
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let comps = [main_module(tools, "tools/package.json"), declared(A, &["tools/requirements.txt"])];
        let d = run(&comps, &[]);
        assert_eq!(edges(&d), vec![(tools.to_string(), A.to_string())]);
        assert!(d.root_anchors.is_empty());
    }

    #[test]
    fn the_walk_continues_to_the_nearest_enclosing_owner() {
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let comps = [main_module(tools, "tools/package.json"), declared(A, &["tools/scripts/ci/requirements.txt"])];
        assert_eq!(edges(&run(&comps, &[])), vec![(tools.to_string(), A.to_string())]);
    }

    #[test]
    fn a_file_under_a_submodule_goes_to_the_submodule() {
        let sub = "pkg:github/waybill-fixture/x@abc";
        let d = run(&[declared(A, &["third_party/x/requirements.txt"])], &[("third_party/x", sub)]);
        assert_eq!(edges(&d), vec![(sub.to_string(), A.to_string())]);
    }

    #[test]
    fn a_submodules_own_python_project_outranks_the_submodule() {
        let sub = "pkg:github/waybill-fixture/x@abc";
        let xpy = "pkg:pypi/waybill-fixture-xpy@1.0.0";
        let comps = [main_module(xpy, "third_party/x/pyproject.toml"), declared(A, &["third_party/x/requirements.txt"])];
        assert_eq!(edges(&run(&comps, &[("third_party/x", sub)])), vec![(xpy.to_string(), A.to_string())]);
    }

    #[test]
    fn a_pypi_main_module_outranks_another_ecosystems_in_the_same_directory() {
        let npm = "pkg:npm/aaa-waybill-fixture@1.0.0";
        let py = "pkg:pypi/zzz-waybill-fixture@1.0.0";
        let comps = [main_module(npm, "tools/package.json"), main_module(py, "tools/pyproject.toml"), declared(A, &["tools/requirements.txt"])];
        assert_eq!(edges(&run(&comps, &[])), vec![(py.to_string(), A.to_string())]);
    }

    #[test]
    fn among_other_ecosystems_the_lowest_purl_wins() {
        let cargo = "pkg:cargo/waybill-fixture-b@1.0.0";
        let npm = "pkg:npm/waybill-fixture-a@1.0.0";
        let comps = [main_module(npm, "tools/package.json"), main_module(cargo, "tools/Cargo.toml"), declared(A, &["tools/requirements.txt"])];
        assert_eq!(edges(&run(&comps, &[])), vec![(cargo.to_string(), A.to_string())]);
    }

    #[test]
    fn two_declaring_files_give_an_edge_and_an_anchor() {
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let comps = [main_module(tools, "tools/package.json"), declared(A, &["requirements.txt", "tools/requirements.txt"])];
        let d = run(&comps, &[]);
        assert_eq!(edges(&d), vec![(tools.to_string(), A.to_string())]);
        assert_eq!(d.root_anchors, BTreeSet::from([A.to_string()]));
    }

    #[test]
    fn only_pypi_requirements_declarations_count() {
        let cget = declared("pkg:generic/waybill-fixture-cget@1.0.0", &["requirements.txt"]);
        let lock = declared(A, &["poetry.lock"]);
        let d = run(&[cget, lock], &[]);
        assert!(d.edges.is_empty() && d.root_anchors.is_empty());
    }

    #[test]
    fn a_directory_with_no_project_component_owns_nothing() {
        // CMake emits no main module (research R3): `native/` has only a
        // CMakeLists.txt, so the walk continues to the root.
        let d = run(&[declared(A, &["native/requirements.txt"])], &[]);
        assert_eq!(d.root_anchors, BTreeSet::from([A.to_string()]));
    }

    #[test]
    fn no_self_edge_and_no_duplicate_of_an_existing_edge() {
        let xpy = "pkg:pypi/waybill-fixture-xpy@1.0.0";
        let mut c = main_module(xpy, "x/pyproject.toml");
        c.evidence.source_file_paths.push("x/requirements.txt".to_string());
        let d = run(&[c], &[]);
        assert!(d.edges.is_empty(), "a main module is never attached to itself");
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let comps = [main_module(tools, "tools/package.json"), declared(A, &["tools/requirements.txt"])];
        let index = OwnerIndex::build(&comps, &BTreeMap::new(), Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        let existing = vec![crate::generate::graph_completeness::test_support::mk_rel(tools, A)];
        assert!(attach(&comps, &existing, &index, Path::new(ROOT)).edges.is_empty());
    }
}
