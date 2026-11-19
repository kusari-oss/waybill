//! Spec 1543 (#1602), research R3 and R4: libraries a CMake file links by
//! link directory and plain name.
//!
//! A vendored prebuilt SDK is often consumed without `find_package`:
//!
//! ```cmake
//! target_link_directories(tgt PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/vendor/sdk/lib)
//! target_link_libraries(tgt PRIVATE sdk)
//! ```
//!
//! This module only reads those declarations, with the conditions they
//! are made under. `scan_fs::cmake_link_attach` matches them against
//! installed-library records. Nothing is evaluated: a directory that still
//! holds `${` after the common source-directory variables are substituted
//! is skipped.
//!
//! `include(<file>)` is followed, because a declaration in an included
//! file is a declaration of the including project: TheRock links its
//! vendored `wkmi` from `CMakeLists_wsl.txt`, which the reader's file
//! patterns never match and which is included only under `if (WIN_SDK)`.
//! The blocks open at the `include()` site are the included file's first
//! conditions.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::scan_fs::package_db::installed_lib::cmake_version::{commands_checked, strip_comments};

/// The deepest `include()` chain followed from one walked file.
const MAX_INCLUDE_DEPTH: usize = 8;

/// One library name a CMake file links from one directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkDeclaration {
    /// The file holding the link (an included file names itself).
    pub file: PathBuf,
    /// The link directory, absolute and lexically normalised.
    pub directory: PathBuf,
    /// The plain library name, as written.
    pub name: String,
    /// The open blocks at the link, include sites first, as C232 writes
    /// them. Empty for an unconditional link.
    pub conditions: Vec<String>,
}

/// Commands that link or set link directories. Everything else is ignored.
const KEYWORDS: [&str; 10] = [
    "PUBLIC",
    "PRIVATE",
    "INTERFACE",
    "LINK_PUBLIC",
    "LINK_PRIVATE",
    "LINK_INTERFACE_LIBRARIES",
    "debug",
    "optimized",
    "general",
    "BEFORE",
];

/// Whether `item` is a plain library name: not a target (`a::b`), path,
/// generator expression, variable, flag or keyword.
pub(crate) fn is_plain_name(item: &str) -> bool {
    !item.is_empty()
        && !item.contains("::")
        && !item.contains(['/', '\\'])
        && !item.contains("$<")
        && !item.contains("${")
        && !item.starts_with('-')
        && !KEYWORDS.contains(&item)
        && item != "AFTER"
}

/// A link before pairing: its target (None for `link_libraries`), name,
/// conditions, and the directory-scope link directories open at it.
struct PendingLink {
    target: Option<String>,
    name: String,
    conditions: Vec<String>,
    scoped: Vec<PathBuf>,
}

struct Ctx<'a> {
    scan_root: &'a Path,
    out: Vec<LinkDeclaration>,
}

/// Every link declaration in `files` and the files they include.
///
/// A walked file that another walked file includes is read only through
/// that include, so it is not read a second time without the include
/// site's conditions.
pub(crate) fn read_links(scan_root: &Path, files: &[PathBuf]) -> Vec<LinkDeclaration> {
    let root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut ctx = Ctx { scan_root: &root, out: Vec::new() };
    let walked: Vec<PathBuf> = files.iter().map(|f| canon(f)).collect();
    let included: HashSet<PathBuf> = walked
        .iter()
        .flat_map(|f| {
            let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
            included_files(f, &dir, &root)
        })
        .collect();
    for f in &walked {
        if included.contains(f) {
            continue;
        }
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut visited = HashSet::from([f.clone()]);
        read_file(&mut ctx, f, &dir, &[], &[], 0, &mut visited);
    }
    ctx.out.sort();
    ctx.out.dedup();
    ctx.out
}

pub(super) fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn commands(file: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(content) = std::fs::read_to_string(file) else { return Vec::new() };
    commands_checked(&strip_comments(&content))
        .0
        .into_iter()
        .map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect()))
        .collect()
}

/// The files `file` includes directly, resolved. `source_dir` is the
/// directory of the `CMakeLists.txt` being processed.
fn included_files(file: &Path, source_dir: &Path, root: &Path) -> Vec<PathBuf> {
    let list_dir = file.parent().unwrap_or(source_dir);
    commands(file)
        .into_iter()
        .filter(|(n, _)| n == "include")
        .filter_map(|(_, a)| resolve_include(a.first()?, source_dir, list_dir, root))
        .collect()
}

/// Substitute the source-directory variables CMake defines for a file.
/// `None` when anything else remains to expand.
pub(super) fn substitute(raw: &str, source_dir: &Path, list_dir: &Path, root: &Path) -> Option<PathBuf> {
    let (src, list, top) = (source_dir.to_string_lossy(), list_dir.to_string_lossy(), root.to_string_lossy());
    let v = raw
        .replace("${CMAKE_CURRENT_SOURCE_DIR}", &src)
        .replace("${CMAKE_CURRENT_LIST_DIR}", &list)
        .replace("${PROJECT_SOURCE_DIR}", &top)
        .replace("${CMAKE_SOURCE_DIR}", &top);
    if v.is_empty() || v.contains("${") {
        return None;
    }
    let p = Path::new(&v);
    // A relative path is relative to the current source directory.
    Some(lexical_normalize(&if p.is_absolute() { p.to_path_buf() } else { source_dir.join(p) }))
}

/// An `include()` argument naming an existing file inside the scan root.
/// A bare module name (`include(FetchContent)`) is not a path.
pub(super) fn resolve_include(arg: &str, source_dir: &Path, list_dir: &Path, root: &Path) -> Option<PathBuf> {
    if !arg.contains(['/', '\\', '.']) {
        return None;
    }
    let p = substitute(arg, source_dir, list_dir, root)?;
    (p.starts_with(root) && p.is_file()).then(|| canon(&p)).filter(|c| c.starts_with(root))
}

#[allow(clippy::too_many_arguments)]
fn read_file(
    ctx: &mut Ctx<'_>,
    file: &Path,
    source_dir: &Path,
    seed: &[String],
    inherited_dirs: &[PathBuf],
    depth: usize,
    visited: &mut HashSet<PathBuf>,
) {
    let list_dir = file.parent().unwrap_or(source_dir).to_path_buf();
    let root = ctx.scan_root.to_path_buf();
    let mut blocks = super::blocks::BlockStack::default();
    let mut dir_scope: Vec<PathBuf> = inherited_dirs.to_vec();
    let mut target_dirs: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
    let mut links: Vec<PendingLink> = Vec::new();
    let conditions = |blocks: &super::blocks::BlockStack| -> Vec<String> {
        seed.iter().cloned().chain(blocks.conditions()).collect()
    };
    for (name, args) in commands(file) {
        if blocks.observe(&name, &args) {
            continue;
        }
        let resolve = |a: &String| substitute(a, source_dir, &list_dir, &root);
        match name.as_str() {
            "link_directories" => {
                dir_scope.extend(args.iter().filter(|a| *a != "AFTER" && *a != "BEFORE").filter_map(resolve));
            }
            "target_link_directories" => {
                let Some((tgt, rest)) = args.split_first() else { continue };
                let dirs = rest.iter().filter(|a| !KEYWORDS.contains(&a.as_str())).filter_map(resolve);
                target_dirs.entry(tgt.clone()).or_default().extend(dirs);
            }
            "target_link_libraries" => {
                let Some((tgt, rest)) = args.split_first() else { continue };
                for item in rest.iter().filter(|a| is_plain_name(a)) {
                    links.push(PendingLink {
                        target: Some(tgt.clone()),
                        name: item.clone(),
                        conditions: conditions(&blocks),
                        scoped: dir_scope.clone(),
                    });
                }
            }
            "link_libraries" => {
                for item in args.iter().filter(|a| is_plain_name(a)) {
                    links.push(PendingLink {
                        target: None,
                        name: item.clone(),
                        conditions: conditions(&blocks),
                        scoped: dir_scope.clone(),
                    });
                }
            }
            "include" if depth < MAX_INCLUDE_DEPTH => {
                let Some(target) = args.first().and_then(|a| resolve_include(a, source_dir, &list_dir, &root)) else {
                    continue;
                };
                if visited.insert(target.clone()) {
                    let seed_here = conditions(&blocks);
                    read_file(ctx, &target, source_dir, &seed_here, &dir_scope, depth + 1, visited);
                    visited.remove(&target);
                }
            }
            _ => {}
        }
    }
    // A target's link directories apply to all its links, wherever in the
    // file they are set: they are a target property.
    for link in links {
        let mut dirs: BTreeSet<PathBuf> = link.scoped.into_iter().collect();
        if let Some(t) = link.target.as_ref().and_then(|t| target_dirs.get(t)) {
            dirs.extend(t.iter().cloned());
        }
        for directory in dirs {
            ctx.out.push(LinkDeclaration {
                file: file.to_path_buf(),
                directory,
                name: link.name.clone(),
                conditions: link.conditions.clone(),
            });
        }
    }
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        for (rel, body) in files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let root = canon(dir.path());
        (dir, root)
    }

    fn links(root: &Path, walked: &[&str]) -> Vec<(String, String, Vec<String>)> {
        let files: Vec<PathBuf> = walked.iter().map(|w| root.join(w)).collect();
        read_links(root, &files)
            .into_iter()
            .map(|l| (l.directory.strip_prefix(root).unwrap().to_string_lossy().into_owned(), l.name, l.conditions))
            .collect()
    }

    #[test]
    fn only_plain_names_are_links() {
        for n in ["wkmi", "m", "foo_bar"] {
            assert!(is_plain_name(n), "{n}");
        }
        for n in ["a::b", "lib/x.a", "${V}", "$<TARGET_FILE:x>", "-lfoo", "PRIVATE", "debug", "", "C:\\x.lib"] {
            assert!(!is_plain_name(n), "{n}");
        }
    }

    #[test]
    fn a_target_link_pairs_with_its_own_directories_only() {
        let (_d, root) = tree(&[(
            "CMakeLists.txt",
            "target_link_directories(t BEFORE PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/sdk/lib)\n\
             target_link_directories(other PRIVATE elsewhere)\n\
             target_link_libraries(t PRIVATE sdk m::m ${X})\n",
        )]);
        assert_eq!(links(&root, &["CMakeLists.txt"]), [("sdk/lib".into(), "sdk".into(), vec![])]);
    }

    #[test]
    fn link_directories_apply_to_later_links_of_any_target() {
        let (_d, root) = tree(&[(
            "CMakeLists.txt",
            "target_link_libraries(early PRIVATE before)\nlink_directories(AFTER sdk/lib)\ntarget_link_libraries(late PRIVATE after)\n",
        )]);
        assert_eq!(links(&root, &["CMakeLists.txt"]), [("sdk/lib".into(), "after".into(), vec![])]);
    }

    #[test]
    fn an_include_carries_its_site_conditions_and_its_own_directories() {
        let (_d, root) = tree(&[
            ("p/CMakeLists.txt", "if (WIN_SDK)\n  include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt)\nendif()\n"),
            (
                "p/CMakeLists_wsl.txt",
                "target_link_directories(t PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../sdk/lnx/lib)\ntarget_link_libraries(t PRIVATE sdk)\n",
            ),
        ]);
        assert_eq!(links(&root, &["p/CMakeLists.txt"]), [("sdk/lnx/lib".into(), "sdk".into(), vec!["WIN_SDK".into()])]);
    }

    #[test]
    fn list_dir_is_the_included_files_own_directory() {
        let (_d, root) = tree(&[
            ("CMakeLists.txt", "include(cmake/links.cmake)\n"),
            ("cmake/links.cmake", "link_directories(${CMAKE_CURRENT_LIST_DIR}/lib)\nlink_libraries(z)\n"),
        ]);
        // The included `.cmake` file is walked too; it is read once, through
        // the include, not again on its own.
        assert_eq!(links(&root, &["CMakeLists.txt", "cmake/links.cmake"]), [("cmake/lib".into(), "z".into(), vec![])]);
    }

    #[test]
    fn module_names_unresolved_variables_and_cycles_are_skipped() {
        let (_d, root) = tree(&[
            ("CMakeLists.txt", "include(FetchContent)\ninclude(${SOMEWHERE}/x.cmake)\ninclude(a.cmake)\n"),
            ("a.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/b.cmake)\nlink_directories(d)\nlink_libraries(fromA)\n"),
            ("b.cmake", "include(${CMAKE_CURRENT_LIST_DIR}/a.cmake)\n"),
        ]);
        assert_eq!(links(&root, &["CMakeLists.txt"]), [("d".into(), "fromA".into(), vec![])]);
    }

    #[test]
    fn an_include_chain_stops_at_the_depth_bound() {
        let mut files: Vec<(String, String)> = vec![("CMakeLists.txt".into(), "include(i0.cmake)\n".into())];
        for i in 0..12 {
            files.push((format!("i{i}.cmake"), format!("include(i{}.cmake)\nlink_directories(d{i})\nlink_libraries(n{i})\n", i + 1)));
        }
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let (_d, root) = tree(&refs);
        let got = links(&root, &["CMakeLists.txt"]);
        assert_eq!(got.len(), MAX_INCLUDE_DEPTH, "{got:?}");
    }
}
