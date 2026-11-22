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
//! - **The nearest enclosing project**, walking up from the file's
//!   directory and including the scan root itself (FR-002, FR-003). A
//!   project is a main module of any ecosystem or a git submodule. In one
//!   directory, a `pkg:pypi` main module wins, then any other main module
//!   (lowest PURL), then a submodule. CMake, Conan and vcpkg emit no project
//!   component, so their directories own nothing.
//! - **No project anywhere up to the scan root:** the document's subject.
//!   The root is chosen per format at emit time, so these become *declared
//!   root anchors*, installed with the reader anchors (`cli/scan_cmd.rs`).
//!
//! A scan-root main module owns a scan-root file. Sending that file to the
//! subject instead put every package a second time directly under a
//! synthetic `--root-name` root, a star beside the main module's own edges:
//! the quality corpus's `python-ansible` went from depth 2 to a flat graph.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};

use waybill_common::resolution::{
    EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent,
};

/// Owner rank within one directory (lower wins), research R3.
const RANK_PYPI_MAIN_MODULE: u8 = 0;
const RANK_OTHER_MAIN_MODULE: u8 = 1;
/// Spec 1618 (research R3): a CMake project owns its directory unless another
/// ecosystem's main module is there too, and outranks a submodule.
const RANK_CMAKE_PROJECT: u8 = 2;
const RANK_SUBMODULE: u8 = 3;

/// `p` as a path relative to the scan root, without `.` or `..`. `None`
/// when it is absolute outside the root or escapes it.
pub(crate) fn relative(p: &str, scan_root: &Path, canonical_root: &Path) -> Option<PathBuf> {
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

/// Spec 1642 (research R2): the source path a main module's project
/// directory comes from. A manifest-reading main module records its
/// project's directory (`path+file://<dir>`); when a lockfile entry of the
/// same PURL was merged into it, the lockfile's path is recorded too, and
/// spec 1542 sorts the merged list, so the first path may be the lockfile.
/// The first path that names a directory in the tree wins; otherwise the
/// first path, as before (CMake and Go record a manifest file).
fn project_path<'a>(c: &'a ResolvedComponent, scan_root: &Path, canonical_root: &Path) -> Option<&'a String> {
    let paths = &c.evidence.source_file_paths;
    paths
        .iter()
        .find(|p| relative(p, scan_root, canonical_root).is_some_and(|rel| canonical_root.join(rel).is_dir()))
        .or_else(|| paths.first())
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

/// Spec 1642 (FR-003): a directory whose owner the PURL decides, because the
/// best candidates tie on rank and scope.
fn log_purl_ties(all_by_dir: &BTreeMap<PathBuf, Vec<Candidate>>) {
    for (dir, cands) in all_by_dir {
        let mut sorted = cands.clone();
        sorted.sort();
        let tied: Vec<&str> = match sorted.first() {
            Some((r, n, _)) => sorted.iter().filter(|(r2, n2, _)| r2 == r && n2 == n).map(|(_, _, p)| p.as_str()).collect(),
            None => continue,
        };
        if tied.len() > 1 {
            tracing::debug!(target: "waybill::owner_index", dir = %dir.display(), candidates = ?tied, "owner decided by PURL");
        }
    }
}

/// Spec 1642 census (research R6): one `owner_candidates` event per
/// directory where more than one candidate shares the best rank, with each
/// candidate's scope and first source path and the step R3 would decide by.
fn census_owner_candidates(
    components: &[ResolvedComponent],
    all_by_dir: &BTreeMap<PathBuf, Vec<Candidate>>,
    by_dir: &BTreeMap<PathBuf, String>,
) {
    let facts: BTreeMap<&str, (bool, Option<&str>)> = components
        .iter()
        .map(|c| {
            let non_runtime = c.lifecycle_scope.is_some_and(|s| s.is_non_runtime());
            (c.purl.as_str(), (non_runtime, c.evidence.source_file_paths.first().map(String::as_str)))
        })
        .collect();
    for (dir, cands) in all_by_dir {
        let Some(best) = cands.first().map(|(r, _, _)| *r) else { continue };
        let top: Vec<&String> = cands.iter().filter(|(r, _, _)| *r == best).map(|(_, _, p)| p).collect();
        if top.len() < 2 {
            continue;
        }
        let rows: Vec<serde_json::Value> = top
            .iter()
            .map(|p| {
                let (non_runtime, first) = facts.get(p.as_str()).copied().unwrap_or((false, None));
                serde_json::json!({"purl": p, "rank": best, "non_runtime": non_runtime, "first_path": first})
            })
            .collect();
        let scopes: BTreeSet<bool> = top.iter().map(|p| facts.get(p.as_str()).is_some_and(|f| f.0)).collect();
        let step = if scopes.len() > 1 { "runtime" } else { "purl" };
        tracing::debug!(
            target: "waybill::owner_census",
            event = "owner_candidates",
            payload = %serde_json::json!({
                "dir": dir.to_string_lossy(),
                "candidates": rows,
                "step": step,
                "owner": by_dir.get(dir),
            }),
        );
    }
}

/// Spec 1642 (research R3): an owner candidate's key, best first: rank, then
/// a runtime candidate before a non-runtime one, then the PURL.
type Candidate = (u8, bool, String);

/// Directory (relative to the scan root) → the project that owns it.
#[derive(Debug, Default)]
pub(crate) struct OwnerIndex {
    by_dir: BTreeMap<PathBuf, String>,
    /// Every candidate per directory, best first (spec 1618 `enclosing`).
    all_by_dir: BTreeMap<PathBuf, Vec<Candidate>>,
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
        let mut best: BTreeMap<PathBuf, Candidate> = BTreeMap::new();
        let mut all_by_dir: BTreeMap<PathBuf, Vec<Candidate>> = BTreeMap::new();
        let mut offer = |dir: PathBuf, rank: u8, non_runtime: bool, purl: &str| {
            let cand = (rank, non_runtime, purl.to_string());
            all_by_dir.entry(dir.clone()).or_default().push(cand.clone());
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
            let Some(first) = project_path(c, scan_root, &canonical_root) else { continue };
            let Some(ws) = crate::scan_fs::workspace_root::derive_workspace_root(first, &canonical_root, mode) else { continue };
            let dir = if ws == "." { PathBuf::new() } else { PathBuf::from(ws) };
            let rank = if c.purl.ecosystem() == "pypi" {
                RANK_PYPI_MAIN_MODULE
            } else if crate::scan_fs::package_db::cmake::projects::is_cmake_project(c) {
                RANK_CMAKE_PROJECT
            } else {
                RANK_OTHER_MAIN_MODULE
            };
            offer(dir, rank, c.lifecycle_scope.is_some_and(|s| s.is_non_runtime()), c.purl.as_str());
        }
        for (dir, purl) in submodule_dirs {
            let Some(s) = dir.to_str() else { continue };
            let Some(rel) = relative(s, scan_root, &canonical_root) else { continue };
            offer(rel, RANK_SUBMODULE, false, purl);
        }
        let by_dir: BTreeMap<PathBuf, String> = best.into_iter().map(|(d, (_, _, p))| (d, p)).collect();
        log_purl_ties(&all_by_dir);
        for v in all_by_dir.values_mut() {
            v.sort();
            v.dedup();
        }
        tracing::debug!(owners = ?by_dir, "declared requirements: owner index");
        census_owner_candidates(components, &all_by_dir, &by_dir);
        Self { by_dir, all_by_dir }
    }

    /// The nearest owner of a file, walking up from its directory to the
    /// scan root inclusive (`""`).
    /// Spec 1618 (research R4): what encloses the project `own` declared in
    /// directory `dir`: another ecosystem's main module in `dir` itself, or
    /// the submodule whose root it is, else the nearest owner of an ancestor.
    /// Never `own`, and never another CMake project of the same directory:
    /// two `project()` calls in one directory are siblings, not nested.
    pub(crate) fn enclosing(&self, dir: &Path, own: &str) -> Option<&str> {
        if let Some(other) = self
            .all_by_dir
            .get(dir)
            .and_then(|v| v.iter().find(|(rank, _, p)| p != own && *rank != RANK_CMAKE_PROJECT))
        {
            return Some(other.2.as_str());
        }
        dir.ancestors()
            .skip(1)
            .find_map(|d| self.by_dir.get(d).map(String::as_str).filter(|p| *p != own))
    }

    pub(crate) fn owner_of(&self, file: &Path) -> Option<&str> {
        let dir = file.parent()?;
        dir.ancestors()
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
    /// Spec 1642 (research R5): the `root_anchors` that at least one runtime
    /// requirements file declares (`classify_requirements_scope_under`
    /// gives no Optional scope). Only these may protect what they anchor
    /// from transitive non-runtime scope.
    pub runtime_root_anchors: BTreeSet<String>,
}

/// The packages a requirements file declares (pip, non-main-module), by
/// file relative to the scan root.
fn members_by_file(
    components: &[ResolvedComponent],
    scan_root: &Path,
    canonical_root: &Path,
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut out: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for c in components {
        if c.purl.ecosystem() != "pypi" || is_main_module(c) {
            continue;
        }
        for p in &c.evidence.source_file_paths {
            let Some(rel) = relative(p, scan_root, canonical_root) else { continue };
            if is_requirements_file(&rel) {
                out.entry(rel).or_default().insert(c.purl.as_str().to_string());
            }
        }
    }
    out
}

/// The packages of one file that its project depends on directly.
///
/// A compiled file (pip-compile, uv) pins the whole closure and says which
/// pin pulls in which (`# via kombu`); the pip reader turns that into edges
/// between pins. A pin that another pin of the same file reaches is that
/// pin's dependency, not the project's: giving the project an edge to every
/// pin flattened `flask`'s celery example into 19 direct dependencies, 17 of
/// them transitive. The direct set is every pin reachable from the pins no
/// sibling points at, plus any pin left over in a sibling-only cycle, so the
/// rule can never strand a pin. A hand-written file has no such edges, and
/// every pin stays direct.
fn direct_members(members: &BTreeSet<String>, relationships: &[Relationship]) -> BTreeSet<String> {
    let mut inner: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut has_parent: BTreeSet<&str> = BTreeSet::new();
    for r in relationships {
        if r.from != r.to && members.contains(&r.from) && members.contains(&r.to) {
            inner.entry(r.from.as_str()).or_default().push(r.to.as_str());
            has_parent.insert(r.to.as_str());
        }
    }
    let tops: Vec<&str> =
        members.iter().map(String::as_str).filter(|m| !has_parent.contains(m)).collect();
    let mut reached: BTreeSet<&str> = BTreeSet::new();
    let mut stack = tops.clone();
    while let Some(n) = stack.pop() {
        if reached.insert(n) {
            stack.extend(inner.get(n).into_iter().flatten().copied());
        }
    }
    members
        .iter()
        .filter(|m| tops.contains(&m.as_str()) || !reached.contains(m.as_str()))
        .cloned()
        .collect()
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
    let direct: BTreeMap<PathBuf, BTreeSet<String>> = members_by_file(components, scan_root, &canonical_root)
        .into_iter()
        .map(|(file, members)| {
            let d = direct_members(&members, relationships);
            (file, d)
        })
        .collect();
    let mut out = DeclaredAttachment::default();
    for c in components {
        for p in &c.evidence.source_file_paths {
            let Some(rel) = relative(p, scan_root, &canonical_root) else { continue };
            // Only a pip, non-main-module package is ever a member
            // (`members_by_file`), and only a direct one is attached.
            if !direct.get(&rel).is_some_and(|d| d.contains(c.purl.as_str())) {
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
                    let file = canonical_root.join(&rel);
                    if crate::scan_fs::package_db::pip::requirements_file_is_runtime(&file, &canonical_root) {
                        out.runtime_root_anchors.insert(c.purl.as_str().to_string());
                    }
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

    fn cmake_project(purl: &str, lists: &str) -> ResolvedComponent {
        let mut c = main_module(purl, lists);
        c.extra_annotations.insert("waybill:source-mechanism".to_string(), serde_json::json!("cmake-project"));
        c
    }

    fn index(comps: &[ResolvedComponent], subs: &[(&str, &str)]) -> OwnerIndex {
        let dirs: BTreeMap<PathBuf, String> =
            subs.iter().map(|(d, p)| (Path::new(ROOT).join(d), p.to_string())).collect();
        OwnerIndex::build(comps, &dirs, Path::new(ROOT), crate::scan_fs::ScanMode::Path)
    }

    /// Spec 1618 (research R3): in one directory, another ecosystem's main
    /// module and a PyPI one outrank a CMake project, which outranks a
    /// submodule.
    #[test]
    fn s1618_cmake_project_rank() {
        let cm = "pkg:generic/waybill-fixture-cm@1.0.0";
        // Sorts after `pkg:generic`, so only the rank, not the PURL tie-break, lets it win.
        let golang = "pkg:golang/example.com/waybill-fixture-mod@v1.0.0";
        let py = "pkg:pypi/waybill-fixture-py@1.0.0";
        let sub = "pkg:github/example/waybill-fixture-sub@abc";
        let i = index(&[cmake_project(cm, "d/CMakeLists.txt"), main_module(golang, "d/go.mod")], &[]);
        assert_eq!(i.owner_of(Path::new("d/x.txt")), Some(golang));
        let i = index(&[cmake_project(cm, "d/CMakeLists.txt"), main_module(py, "d/pyproject.toml")], &[]);
        assert_eq!(i.owner_of(Path::new("d/x.txt")), Some(py));
        let i = index(&[cmake_project(cm, "d/CMakeLists.txt")], &[("d", sub)]);
        assert_eq!(i.owner_of(Path::new("d/x.txt")), Some(cm));
    }

    /// Spec 1618 (research R4): `enclosing` never returns the project itself.
    #[test]
    fn s1618_enclosing() {
        let outer = "pkg:generic/waybill-fixture-outer@1.0.0";
        let inner = "pkg:generic/waybill-fixture-inner@1.0.0";
        let sub = "pkg:github/example/waybill-fixture-sub@abc";
        let i = index(&[cmake_project(outer, "CMakeLists.txt"), cmake_project(inner, "a/b/CMakeLists.txt")], &[]);
        assert_eq!(i.enclosing(Path::new("a/b"), inner), Some(outer));
        assert_eq!(i.enclosing(Path::new(""), outer), None, "nothing encloses the scan root's project");
        // A project at a submodule's root is enclosed by that submodule.
        let i = index(&[cmake_project(inner, "s/CMakeLists.txt")], &[("s", sub)]);
        assert_eq!(i.enclosing(Path::new("s"), inner), Some(sub));
        // Two CMake projects in one directory do not enclose each other: the
        // nearest owner above encloses both.
        let twin = "pkg:generic/waybill-fixture-twin@1.0.0";
        let i = index(
            &[cmake_project(outer, "CMakeLists.txt"), cmake_project(inner, "a/CMakeLists.txt"), cmake_project(twin, "a/CMakeLists.txt")],
            &[],
        );
        assert_eq!(i.enclosing(Path::new("a"), inner), Some(outer));
        assert_eq!(i.enclosing(Path::new("a"), twin), Some(outer));
        // Another ecosystem's main module in the same directory encloses it.
        let py = "pkg:pypi/waybill-fixture-py@1.0.0";
        let i = index(&[cmake_project(inner, "CMakeLists.txt"), main_module(py, "pyproject.toml")], &[]);
        assert_eq!(i.enclosing(Path::new(""), inner), Some(py));
    }

    #[test]
    fn a_file_at_the_scan_root_goes_to_a_scan_root_main_module() {
        let app = "pkg:pypi/waybill-fixture-app@1.0.0";
        let comps = [main_module(app, "pyproject.toml"), declared(A, &["requirements.txt"])];
        let d = run(&comps, &[]);
        assert_eq!(edges(&d), vec![(app.to_string(), A.to_string())]);
        assert!(d.root_anchors.is_empty(), "a scan-root project owns its file; the subject does not");
    }

    #[test]
    fn a_file_at_the_scan_root_with_no_project_is_a_root_anchor() {
        let d = run(&[declared(A, &["requirements.txt"])], &[]);
        assert!(d.edges.is_empty());
        assert_eq!(d.root_anchors, BTreeSet::from([A.to_string()]));
    }

    #[test]
    fn a_nested_file_with_no_owner_of_its_own_walks_up_to_the_scan_root_project() {
        let app = "pkg:pypi/waybill-fixture-app@1.0.0";
        let comps = [main_module(app, "pyproject.toml"), declared(A, &["docs/requirements.txt"])];
        let d = run(&comps, &[]);
        assert_eq!(edges(&d), vec![(app.to_string(), A.to_string())]);
        assert!(d.root_anchors.is_empty());
    }

    /// Main-module readers record the project directory as a
    /// `path+file://<abs dir>` URI, not a manifest path (measured: npm and
    /// pip at the scan stage). That directory is the owner, and it is nearer
    /// than the scan root's own project.
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
        assert!(d.root_anchors.is_empty(), "the nearer project wins over the scan root's");
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

    const B: &str = "pkg:pypi/waybill-fixture-b@2.0.0";
    const C: &str = "pkg:pypi/waybill-fixture-c@3.0.0";
    const APP: &str = "pkg:pypi/waybill-fixture-app@1.0.0";

    fn run_with(comps: &[ResolvedComponent], rels: &[Relationship]) -> DeclaredAttachment {
        let index = OwnerIndex::build(comps, &BTreeMap::new(), Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        attach(comps, rels, &index, Path::new(ROOT))
    }

    /// pip-compile's `# via`: the pip reader writes `A -> B` and `B -> C`.
    /// The project gets A only; B and C are reached through it.
    #[test]
    fn a_compiled_file_gives_its_project_only_the_pins_no_sibling_reaches() {
        use crate::generate::graph_completeness::test_support::mk_rel;
        let comps = [
            main_module(APP, "pyproject.toml"),
            declared(A, &["requirements.txt"]),
            declared(B, &["requirements.txt"]),
            declared(C, &["requirements.txt"]),
        ];
        let d = run_with(&comps, &[mk_rel(A, B), mk_rel(B, C)]);
        assert_eq!(edges(&d), vec![(APP.to_string(), A.to_string())]);
    }

    #[test]
    fn a_pin_in_a_sibling_only_cycle_is_still_attached() {
        use crate::generate::graph_completeness::test_support::mk_rel;
        let comps = [
            main_module(APP, "pyproject.toml"),
            declared(A, &["requirements.txt"]),
            declared(B, &["requirements.txt"]),
        ];
        let mut got = edges(&run_with(&comps, &[mk_rel(A, B), mk_rel(B, A)]));
        got.sort();
        assert_eq!(got, vec![(APP.to_string(), A.to_string()), (APP.to_string(), B.to_string())]);
    }

    /// Only an edge from a pin of the SAME file demotes a pin. An edge from
    /// a package of another file, or from no file, leaves the declaration
    /// direct.
    #[test]
    fn an_edge_from_outside_the_file_does_not_demote_a_pin() {
        use crate::generate::graph_completeness::test_support::mk_rel;
        let comps = [
            main_module(APP, "pyproject.toml"),
            declared(A, &["requirements.txt"]),
            declared(B, &["docs/requirements.txt"]),
        ];
        let mut got = edges(&run_with(&comps, &[mk_rel(B, A)]));
        got.sort();
        assert_eq!(got, vec![(APP.to_string(), A.to_string()), (APP.to_string(), B.to_string())]);
    }

    /// A pin declared by two files is direct in the one whose siblings do
    /// not reach it, and transitive in the other.
    #[test]
    fn directness_is_decided_per_file() {
        use crate::generate::graph_completeness::test_support::mk_rel;
        let tools = "pkg:npm/waybill-fixture-tools@1.0.0";
        let comps = [
            main_module(APP, "pyproject.toml"),
            main_module(tools, "tools/package.json"),
            declared(A, &["requirements.txt"]),
            declared(B, &["requirements.txt", "tools/requirements.txt"]),
        ];
        let mut got = edges(&run_with(&comps, &[mk_rel(A, B)]));
        got.sort();
        assert_eq!(
            got,
            vec![(tools.to_string(), B.to_string()), (APP.to_string(), A.to_string())],
        );
    }

    // -----------------------------------------------------------------
    // Spec 1642 (#1640)
    // -----------------------------------------------------------------

    fn scoped_main_module(purl: &str, manifest: &str, scope: Option<waybill_common::resolution::LifecycleScope>) -> ResolvedComponent {
        let mut c = main_module(purl, manifest);
        c.lifecycle_scope = scope;
        c
    }

    fn owner(comps: &[ResolvedComponent], file: &str) -> Option<String> {
        let index = OwnerIndex::build(comps, &BTreeMap::new(), Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        index.owner_of(Path::new(file)).map(str::to_string)
    }

    /// FR-003 step 2 (research R3): a runtime candidate beats a non-runtime
    /// one of the same rank, whatever their PURLs. The tie the step decides
    /// is not produced by any scanned tree after R2, so this is its test
    /// (mutation M4).
    #[test]
    fn s1642_a_runtime_candidate_beats_a_non_runtime_one() {
        use waybill_common::resolution::LifecycleScope;
        let dev = "pkg:pypi/waybill-fixture-a-dev@0.1.0";
        let tool = "pkg:pypi/waybill-fixture-z-tool@1.0.0";
        let comps = [
            scoped_main_module(dev, "tool/pyproject.toml", Some(LifecycleScope::Optional)),
            scoped_main_module(tool, "tool/pyproject.toml", None),
        ];
        assert_eq!(owner(&comps, "tool/requirements.txt").as_deref(), Some(tool));
    }

    /// FR-003: rank still comes first, and the PURL decides last.
    #[test]
    fn s1642_rank_first_then_scope_then_purl() {
        use waybill_common::resolution::LifecycleScope;
        let py_dev = "pkg:pypi/waybill-fixture-pydev@1.0.0";
        let npm = "pkg:npm/waybill-fixture-web@1.0.0";
        let comps = [
            scoped_main_module(py_dev, "x/pyproject.toml", Some(LifecycleScope::Development)),
            scoped_main_module(npm, "x/package.json", None),
        ];
        assert_eq!(owner(&comps, "x/requirements.txt").as_deref(), Some(py_dev), "a pip main module outranks any other, runtime or not");
        let (b, c) = ("pkg:pypi/waybill-fixture-b@1.0.0", "pkg:pypi/waybill-fixture-c@1.0.0");
        let comps = [scoped_main_module(c, "y/pyproject.toml", None), scoped_main_module(b, "y/pyproject.toml", None)];
        assert_eq!(owner(&comps, "y/requirements.txt").as_deref(), Some(b));
    }

    /// Research R2: a main module whose source paths include its project's
    /// directory owns that directory, even when a lockfile path sorts first.
    #[test]
    fn s1642_a_main_module_owns_the_directory_its_paths_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::create_dir_all(root.join("tool/devtool")).unwrap();
        std::fs::write(root.join("tool/uv.lock"), "").unwrap();
        // As the readers record them: the lockfile's path, and the
        // project's directory in `path+file://` form (sorted after it).
        let dir_form = |rel: &str| format!("path+file://{}", root.join(rel).display());
        let mut dev = main_module("pkg:pypi/waybill-fixture-devtool@0.1.0", "tool/uv.lock");
        dev.evidence.source_file_paths.push(dir_form("tool/devtool"));
        let tool = main_module(TOOL_1642, &dir_form("tool"));
        let comps = [dev, tool];
        let index = OwnerIndex::build(&comps, &BTreeMap::new(), &root, crate::scan_fs::ScanMode::Path);
        assert_eq!(index.owner_of(Path::new("tool/requirements.txt")), Some(TOOL_1642));
        assert_eq!(index.owner_of(Path::new("tool/devtool/requirements.txt")), Some("pkg:pypi/waybill-fixture-devtool@0.1.0"));
    }

    const TOOL_1642: &str = "pkg:pypi/waybill-fixture-tool@1.0.0";

    /// Research R5: only a runtime requirements file's anchors may protect.
    #[test]
    fn s1642_runtime_root_anchors_exclude_dev_files() {
        let comps = [declared(A, &["requirements.txt"]), declared(B, &["requirements-dev.txt"])];
        let got = run(&comps, &[]);
        assert_eq!(got.root_anchors, [A, B].iter().map(|s| s.to_string()).collect());
        assert_eq!(got.runtime_root_anchors, [A].iter().map(|s| s.to_string()).collect());
    }
}
