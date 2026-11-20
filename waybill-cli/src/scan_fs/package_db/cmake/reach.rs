//! Spec 1614 (#1605), research R1–R2: the conditions under which each CMake
//! file is reached.
//!
//! The reader reads every CMake file on its own, so a file pulled in by
//! `include()` or `add_subdirectory()` inside an `if()` used to look
//! unconditional. This pre-pass records, per file, what reaches it:
//!
//! - **Edges.** Each walked `CMakeLists.txt` is traversed with its own
//!   directory as the current source directory. An `include()` (resolved as
//!   `links.rs` resolves it) is an edge to the included file, which is then
//!   traversed with the same source directory, as CMake processes it. An
//!   `add_subdirectory(<dir>)` is an edge to `<dir>/CMakeLists.txt`. Every
//!   edge carries the blocks open at its call site, in C232's form. A walked
//!   `*.cmake` file nothing reached is traversed with its own directory.
//! - **Modules by name.** `include(<name>)` with no path is looked up as
//!   `<dir>/<name>.cmake` along `CMAKE_MODULE_PATH`, tracked from literal
//!   `list(APPEND|PREPEND CMAKE_MODULE_PATH …)` and `set(CMAKE_MODULE_PATH
//!   …)`. An included file shares its includer's scope; a subdirectory
//!   inherits the module path its parent had at `add_subdirectory()`, so
//!   `CMakeLists.txt` files are traversed parents first. A name not found
//!   on the path (CMake's own modules) contributes nothing.
//! - **Reach.** A file is [`Reach::Unconditional`] when nothing reaches it,
//!   or when some edge into it has no condition and comes from an
//!   unconditional file. Otherwise it is [`Reach::Conditional`]: the union,
//!   over every edge, of the edge's conditions and its source's. A cycle is
//!   unconditional.
//!
//! Nothing is evaluated. A path holding a variable other than the source-
//! and list-directory ones resolves to nothing and contributes nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::blocks::BlockStack;
use super::links::{canon, resolve_include, substitute};
use crate::scan_fs::package_db::installed_lib::cmake_version::{commands_checked, strip_comments};

/// The deepest `include()` chain followed from one traversal, as
/// `links.rs` bounds it.
const MAX_INCLUDE_DEPTH: usize = 8;

/// How a CMake file is reached (data-model "Reach").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    Unconditional,
    Conditional(BTreeSet<String>),
}

impl Reach {
    /// The conditions to seed the file's `BlockStack` with.
    pub(crate) fn seed(&self) -> Vec<String> {
        match self {
            Reach::Unconditional => Vec::new(),
            Reach::Conditional(c) => c.iter().cloned().collect(),
        }
    }
}

/// Every file's [`Reach`], by canonical path, and the files `include()`
/// reached that the walker did not hand the reader (research R2).
#[derive(Debug, Default)]
pub(crate) struct ReachMap {
    by_file: BTreeMap<PathBuf, Reach>,
    pub(crate) extra_files: Vec<PathBuf>,
    scan_root: PathBuf,
    canonical_root: PathBuf,
    /// Spec 1618 (research R9): the files the scan-root `CMakeLists.txt`
    /// reaches through add_subdirectory() or include(), itself included;
    /// `None` when there is no scan-root `CMakeLists.txt`.
    from_root: Option<BTreeSet<PathBuf>>,
}

impl ReachMap {
    /// The conditions reaching `path`, a walked file or an extra file.
    pub(crate) fn seed(&self, path: &Path) -> Vec<String> {
        self.by_file.get(&rebase(path, &self.scan_root, &self.canonical_root)).map(Reach::seed).unwrap_or_default()
    }

    /// Whether the scan-root `CMakeLists.txt` reaches `path`; `None` when the
    /// scan root has no `CMakeLists.txt`, where the question is undefined.
    pub(crate) fn reached_from_root(&self, path: &Path) -> Option<bool> {
        let set = self.from_root.as_ref()?;
        Some(set.contains(&rebase(path, &self.scan_root, &self.canonical_root)))
    }
}

/// A file's commands, lowercased names, shared with the fetch reader's parse.
pub(crate) type Commands = Rc<Vec<(String, Vec<String>)>>;

/// `p` under the canonical scan root, by swapping the root prefix: one
/// `canonicalize` for the root instead of one per file (6,000+ on TheRock).
/// A path outside the given root is canonicalised.
fn rebase(p: &Path, scan_root: &Path, canonical_root: &Path) -> PathBuf {
    match p.strip_prefix(scan_root) {
        Ok(rel) => canonical_root.join(rel),
        Err(_) => canon(p),
    }
}

/// A file read and tokenized as the fetch reader tokenizes it.
pub(crate) fn parse_commands(file: &Path) -> Option<(Commands, String, bool)> {
    let content = std::fs::read_to_string(file).ok()?;
    let (cmds, unterminated) = commands_checked(&strip_comments(&content));
    let cmds = cmds
        .into_iter()
        .map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect()))
        .collect();
    Some((Rc::new(cmds), content, unterminated))
}

struct Graph<'a> {
    root: &'a Path,
    parsed: HashMap<PathBuf, Commands>,
    /// Edges into each file: (source file, conditions at the call site).
    into: BTreeMap<PathBuf, BTreeSet<(PathBuf, Vec<String>)>>,
    /// `CMAKE_MODULE_PATH` each `CMakeLists.txt` inherits from the parent
    /// that adds it.
    inherited: HashMap<PathBuf, Vec<PathBuf>>,
}

/// `CMAKE_MODULE_PATH` after one `list()` / `set()` command, or `None` when
/// the command does not touch it.
fn module_path_after(name: &str, args: &[String], current: &[PathBuf], resolve: impl Fn(&str) -> Option<PathBuf>) -> Option<Vec<PathBuf>> {
    match name {
        "list" if args.get(1).map(String::as_str) == Some("CMAKE_MODULE_PATH") => {
            let added: Vec<PathBuf> = args.iter().skip(2).filter_map(|a| resolve(a)).collect();
            match args.first().map(String::as_str) {
                Some("APPEND") => Some(current.iter().cloned().chain(added).collect()),
                Some("PREPEND") => Some(added.into_iter().chain(current.iter().cloned()).collect()),
                _ => None,
            }
        }
        "set" if args.first().map(String::as_str) == Some("CMAKE_MODULE_PATH") => {
            let mut out = Vec::new();
            for a in args.iter().skip(1) {
                if a == "CACHE" || a == "PARENT_SCOPE" {
                    break;
                }
                if a == "${CMAKE_MODULE_PATH}" {
                    out.extend(current.iter().cloned());
                } else if let Some(p) = resolve(a) {
                    out.push(p);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

impl Graph<'_> {
    fn commands(&mut self, file: &Path) -> Commands {
        self.parsed
            .entry(file.to_path_buf())
            .or_insert_with(|| parse_commands(file).map(|(c, _, _)| c).unwrap_or_default())
            .clone()
    }

    /// Traverse `file` as CMake processes it with `source_dir` current and
    /// `module_path` the scope's `CMAKE_MODULE_PATH`.
    fn traverse(
        &mut self,
        file: &Path,
        source_dir: &Path,
        depth: usize,
        on_stack: &mut HashSet<PathBuf>,
        module_path: &mut Vec<PathBuf>,
    ) {
        let list_dir = file.parent().unwrap_or(source_dir).to_path_buf();
        let root = self.root.to_path_buf();
        let mut blocks = BlockStack::default();
        for (name, args) in self.commands(file).iter() {
            if blocks.observe(name, args) {
                continue;
            }
            if let Some(mp) = module_path_after(name, args, module_path, |a| substitute(a, source_dir, &list_dir, &root)) {
                *module_path = mp;
                continue;
            }
            let Some(arg) = args.first() else { continue };
            match name.as_str() {
                "include" => {
                    let by_name = || {
                        if arg.contains(['/', '\\', '.', '$']) {
                            return None;
                        }
                        module_path
                            .iter()
                            .map(|d| d.join(format!("{arg}.cmake")))
                            .find(|c| c.is_file())
                            .map(|c| canon(&c))
                            .filter(|c| c.starts_with(&root))
                    };
                    let Some(target) = resolve_include(arg, source_dir, &list_dir, &root).or_else(by_name) else { continue };
                    self.into.entry(target.clone()).or_default().insert((file.to_path_buf(), blocks.conditions()));
                    if depth < MAX_INCLUDE_DEPTH && on_stack.insert(target.clone()) {
                        self.traverse(&target, source_dir, depth + 1, on_stack, module_path);
                        on_stack.remove(&target);
                    }
                }
                "add_subdirectory" => {
                    let Some(dir) = substitute(arg, source_dir, &list_dir, &root) else { continue };
                    let target = dir.join("CMakeLists.txt");
                    if target.is_file() {
                        let target = canon(&target);
                        if target.starts_with(&root) && target != file {
                            self.inherited.entry(target.clone()).or_insert_with(|| module_path.clone());
                            self.into.entry(target).or_default().insert((file.to_path_buf(), blocks.conditions()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Compute every file's [`Reach`] for the walked CMake files of `scan_root`,
/// given their commands as the fetch reader parsed them (spec 1614 research
/// R12: each file is tokenized once). Files `include()` reaches that are not
/// walked are parsed here.
pub(crate) fn compute(scan_root: &Path, walked: &[(PathBuf, Commands)]) -> ReachMap {
    let root = canon(scan_root);
    let mut parsed = HashMap::new();
    let mut walked_set = BTreeSet::new();
    for (p, cmds) in walked {
        let c = rebase(p, scan_root, &root);
        parsed.insert(c.clone(), cmds.clone());
        walked_set.insert(c);
    }
    let walked = walked_set;
    let mut g = Graph { root: &root, parsed, into: BTreeMap::new(), inherited: HashMap::new() };
    let lists: BTreeSet<PathBuf> =
        walked.iter().filter(|f| f.file_name().is_some_and(|n| n == "CMakeLists.txt")).cloned().collect();
    // Parents first, so a subdirectory inherits its parent's module path.
    let mut ordered: Vec<&PathBuf> = lists.iter().collect();
    ordered.sort_by_key(|f| (f.components().count(), (*f).clone()));
    for f in ordered {
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut module_path = g.inherited.get(f).cloned().unwrap_or_default();
        g.traverse(f, &dir, 0, &mut HashSet::from([f.clone()]), &mut module_path);
    }
    let unreached: Vec<PathBuf> =
        walked.iter().filter(|f| !lists.contains(*f) && !g.into.contains_key(*f)).cloned().collect();
    for f in &unreached {
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        g.traverse(f, &dir, 0, &mut HashSet::from([f.clone()]), &mut Vec::new());
    }

    let nodes: BTreeSet<PathBuf> = walked.iter().cloned().chain(g.into.keys().cloned()).collect();
    let mut by_file = fixed_point(&nodes, &g.into);
    // A file only a cycle with no way in reaches has no known source.
    for n in &nodes {
        by_file.entry(n.clone()).or_insert(Reach::Unconditional);
    }
    let root_lists = root.join("CMakeLists.txt");
    let from_root = nodes.contains(&root_lists).then(|| {
        let mut out: BTreeMap<&PathBuf, Vec<&PathBuf>> = BTreeMap::new();
        for (file, edges) in &g.into {
            for (src, _) in edges {
                out.entry(src).or_default().push(file);
            }
        }
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        let mut stack = vec![&root_lists];
        while let Some(f) = stack.pop() {
            if seen.insert(f.clone()) {
                stack.extend(out.get(f).into_iter().flatten().copied());
            }
        }
        seen
    });
    let extra_files = nodes.into_iter().filter(|n| !walked.contains(n)).collect();
    ReachMap { by_file, extra_files, scan_root: scan_root.to_path_buf(), canonical_root: root, from_root }
}

/// Every file's reach, as a fixed point: a file nothing reaches is
/// unconditional; any other file takes, over its edges from sources whose
/// reach is known, the union of the edge's conditions and the source's,
/// and is unconditional as soon as one such path has none. `Unconditional`
/// absorbs. A set can shrink when a source turns unconditional, so the
/// iteration is also capped at one pass per file plus one; files are
/// visited in path order, so the result is deterministic. A file still
/// unknown at the end is absent from the result.
fn fixed_point(
    nodes: &BTreeSet<PathBuf>,
    into: &BTreeMap<PathBuf, BTreeSet<(PathBuf, Vec<String>)>>,
) -> BTreeMap<PathBuf, Reach> {
    let mut known: BTreeMap<PathBuf, Reach> =
        nodes.iter().filter(|n| !into.contains_key(*n)).map(|n| (n.clone(), Reach::Unconditional)).collect();
    for _ in 0..=nodes.len() {
        let mut changed = false;
        for (file, edges) in into {
            if known.get(file) == Some(&Reach::Unconditional) {
                continue;
            }
            let mut union: Option<BTreeSet<String>> = None;
            let mut unconditional = false;
            for (src, site) in edges {
                let Some(src_reach) = known.get(src) else { continue };
                let mut path: BTreeSet<String> = site.iter().cloned().collect();
                if let Reach::Conditional(c) = src_reach {
                    path.extend(c.iter().cloned());
                }
                if path.is_empty() {
                    unconditional = true;
                    break;
                }
                union.get_or_insert_with(BTreeSet::new).extend(path);
            }
            let next = if unconditional {
                Some(Reach::Unconditional)
            } else {
                union.map(Reach::Conditional)
            };
            if let Some(next) = next {
                if known.get(file) != Some(&next) {
                    known.insert(file.clone(), next);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    known
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<(PathBuf, Commands)>) {
        let d = tempfile::tempdir().unwrap();
        let mut walked = Vec::new();
        for (rel, body) in files {
            let p = d.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            if rel.ends_with("CMakeLists.txt") || rel.ends_with(".cmake") {
                walked.push(p);
            }
        }
        let walked = walked.into_iter().map(|p| {
            let c = parse_commands(&p).unwrap().0;
            (p, c)
        }).collect();
        (d, walked)
    }

    fn seed(m: &ReachMap, root: &Path, rel: &str) -> Vec<String> {
        m.seed(&root.join(rel))
    }

    /// Spec 1618 (research R9): reached from the scan-root `CMakeLists.txt`
    /// through add_subdirectory() or include(), conditionally or not.
    #[test]
    fn s1618_reached_from_root() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "add_subdirectory(a)
if(X)
 add_subdirectory(c)
endif()
"),
            ("a/CMakeLists.txt", "include(../cmake/m.cmake)
"),
            ("c/CMakeLists.txt", ""),
            ("cmake/m.cmake", ""),
            ("other/CMakeLists.txt", "add_subdirectory(inner)
"),
            ("other/inner/CMakeLists.txt", ""),
        ]);
        let m = compute(d.path(), &w);
        let r = |rel: &str| m.reached_from_root(&d.path().join(rel));
        assert_eq!(r("CMakeLists.txt"), Some(true));
        assert_eq!(r("a/CMakeLists.txt"), Some(true));
        assert_eq!(r("c/CMakeLists.txt"), Some(true), "a conditional add_subdirectory still reaches");
        assert_eq!(r("cmake/m.cmake"), Some(true));
        assert_eq!(r("other/CMakeLists.txt"), Some(false));
        assert_eq!(r("other/inner/CMakeLists.txt"), Some(false), "reached only from an unreached file");
    }

    #[test]
    fn s1618_no_root_listfile_means_undefined() {
        let (d, w) = tree(&[("a/CMakeLists.txt", ""), ("b/CMakeLists.txt", "")]);
        let m = compute(d.path(), &w);
        assert_eq!(m.reached_from_root(&d.path().join("a/CMakeLists.txt")), None);
    }

    #[test]
    fn an_unconditional_edge_wins_and_conditions_union() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "if(A)\n include(cmake/x.cmake)\nendif()\ninclude(cmake/y.cmake)\nif(B)\n include(cmake/y.cmake)\nendif()\nif(C)\n include(cmake/z.cmake)\nendif()\nif(D)\n include(cmake/z.cmake)\nendif()\n"),
            ("cmake/x.cmake", ""),
            ("cmake/y.cmake", ""),
            ("cmake/z.cmake", ""),
        ]);
        let m = compute(d.path(), &w);
        assert_eq!(seed(&m, d.path(), "cmake/x.cmake"), vec!["A"]);
        assert!(seed(&m, d.path(), "cmake/y.cmake").is_empty(), "one unconditional include wins");
        assert_eq!(seed(&m, d.path(), "cmake/z.cmake"), vec!["C", "D"]);
        assert!(seed(&m, d.path(), "CMakeLists.txt").is_empty());
    }

    #[test]
    fn a_chain_accumulates_and_includes_resolve_against_the_source_directory() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "if(A)\n add_subdirectory(x)\nendif()\n"),
            ("x/CMakeLists.txt", "include(cmake/y.cmake)\n"),
            ("x/cmake/y.cmake", "if(B)\n include(cmake/z.cmake)\nendif()\n"),
            // Relative to CMAKE_CURRENT_SOURCE_DIR (x/), not to y.cmake's directory.
            ("x/cmake/z.cmake", ""),
            ("x/cmake/cmake/z.cmake", ""),
        ]);
        let m = compute(d.path(), &w);
        assert_eq!(seed(&m, d.path(), "x/CMakeLists.txt"), vec!["A"]);
        assert_eq!(seed(&m, d.path(), "x/cmake/y.cmake"), vec!["A"]);
        assert_eq!(seed(&m, d.path(), "x/cmake/z.cmake"), vec!["A", "B"]);
        assert!(seed(&m, d.path(), "x/cmake/cmake/z.cmake").is_empty(), "not reached, so unconditional");
    }

    #[test]
    fn a_cycle_terminates_and_an_unresolved_path_contributes_nothing() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "if(A)\n include(cmake/a.cmake)\nendif()\nif(B)\n include(${SOMEWHERE}/c.cmake)\nendif()\n"),
            ("cmake/a.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/b.cmake)\n"),
            ("cmake/b.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/a.cmake)\n"),
            ("cmake/c.cmake", ""),
        ]);
        let m = compute(d.path(), &w);
        assert_eq!(seed(&m, d.path(), "cmake/a.cmake"), vec!["A"]);
        assert_eq!(seed(&m, d.path(), "cmake/b.cmake"), vec!["A"]);
        assert!(seed(&m, d.path(), "cmake/c.cmake").is_empty());
    }

    #[test]
    fn modules_resolve_along_cmake_module_path() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "set(CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/late)\nlist(PREPEND CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/early)\nset(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} ${CMAKE_CURRENT_SOURCE_DIR}/last)\nif(A)\n include(m)\nendif()\nif(B)\n include(only_last)\nendif()\ninclude(GNUInstallDirs)\n"),
            ("early/m.cmake", ""),
            ("late/m.cmake", ""),
            ("last/only_last.cmake", ""),
        ]);
        let m = compute(d.path(), &w);
        assert_eq!(seed(&m, d.path(), "early/m.cmake"), vec!["A"], "PREPEND puts early/ first");
        assert!(seed(&m, d.path(), "late/m.cmake").is_empty(), "shadowed, so not reached");
        assert_eq!(seed(&m, d.path(), "last/only_last.cmake"), vec!["B"]);
    }

    #[test]
    fn a_cycle_nothing_enters_is_unconditional() {
        let (d, w) = tree(&[
            ("cmake/a.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/b.cmake)\n"),
            ("cmake/b.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/a.cmake)\n"),
        ]);
        let m = compute(d.path(), &w);
        assert!(seed(&m, d.path(), "cmake/a.cmake").is_empty());
        assert!(seed(&m, d.path(), "cmake/b.cmake").is_empty());
    }

    #[test]
    fn a_function_body_is_a_condition_and_a_non_cmake_include_is_an_extra_file() {
        let (d, w) = tree(&[
            ("CMakeLists.txt", "if(WIN_SDK)\n include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt)\nendif()\nfunction(f)\n include(cmake/q.cmake)\nendfunction()\n"),
            ("CMakeLists_wsl.txt", ""),
            ("cmake/q.cmake", ""),
        ]);
        let m = compute(d.path(), &w);
        assert_eq!(seed(&m, d.path(), "CMakeLists_wsl.txt"), vec!["WIN_SDK"]);
        assert_eq!(seed(&m, d.path(), "cmake/q.cmake"), vec!["function: f"]);
        assert_eq!(m.extra_files, vec![canon(&d.path().join("CMakeLists_wsl.txt"))]);
    }
}
