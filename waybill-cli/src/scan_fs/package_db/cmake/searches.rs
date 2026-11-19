//! Spec 1614 (#1591, #1605): package searches — `find_package`,
//! `pkg_check_modules` and `pkg_search_module` — read from every CMake file
//! the shared walker hands the reader, each with the conditions it is made
//! under.
//!
//! Until this spec, package searches were matched by two regexes over a
//! narrow file set (spec 1538 FR-002), blind to `if()` blocks. They are now
//! read from the same parsed commands as fetches, so a search inside an
//! `if()` — or in a file reached only under one — is known to be
//! conditional. The rules the regexes encoded are kept, so a search the old
//! reader saw becomes the same component (FR-009):
//!
//! - `find_package`: the name is the first argument, in
//!   `[A-Za-z0-9_:.+-]`; a second argument starting with a digit is the
//!   version, with `EXACT` anywhere after it.
//! - `pkg_check_modules` / `pkg_search_module`: the first argument is the
//!   result prefix; every other argument that is not a modifier keyword is a
//!   module, stripped of a `>=`/`<=`/`>`/`<`/`=` version suffix.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use super::blocks::BlockStack;

/// C237 `waybill:cmake-find-coverage`.
pub(crate) const ANN_CMAKE_FIND_COVERAGE: &str = "waybill:cmake-find-coverage";

/// What C237 accounts for (data-model "FindCoverage"). Counts by
/// declaration: `read`, `unresolved`, `tools`, `paired_*`,
/// `in_tree_declarations`. By component: `components`, `conditional`,
/// `test`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct FindCoverage {
    pub(crate) read: usize,
    pub(crate) unresolved: usize,
    pub(crate) tools: usize,
    pub(crate) paired_fetch: usize,
    pub(crate) paired_installed: usize,
    pub(crate) paired_submodule: usize,
    pub(crate) in_tree: BTreeSet<String>,
    pub(crate) in_tree_declarations: usize,
    pub(crate) components: usize,
    pub(crate) conditional: usize,
    pub(crate) test: usize,
}

impl FindCoverage {
    /// Sorted-key JSON, or `None` when no package search was read.
    pub(crate) fn wire(&self) -> Option<String> {
        (self.read > 0).then(|| {
            serde_json::json!({
                "components": self.components,
                "conditional": self.conditional,
                "in-tree": self.in_tree,
                "in-tree-declarations": self.in_tree_declarations,
                "paired": {
                    "fetch": self.paired_fetch,
                    "installed": self.paired_installed,
                    "submodule": self.paired_submodule,
                },
                "read": self.read,
                "test": self.test,
                "tools": self.tools,
                "unresolved": self.unresolved,
            })
            .to_string()
        })
    }
}

/// The kind of package search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SearchKind {
    FindPackage,
    /// `pkg_check_modules` or `pkg_search_module`, one search per module.
    PkgConfig,
}

/// One package search: one `find_package`, or one module of a pkg-config
/// search (data-model "PackageSearch").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageSearch {
    pub(crate) kind: SearchKind,
    /// As declared (C103 keeps it when it differs from the lowercase name).
    pub(crate) name: String,
    /// `find_package`'s version requirement as a range (#1520).
    pub(crate) requirement: Option<String>,
    /// `find_package`'s requested components (research R6's Python rule).
    pub(crate) components: Vec<String>,
    pub(crate) source_path: String,
    /// What C236 records: the conditions reaching the file, then the
    /// enclosing blocks.
    pub(crate) conditions: Vec<String>,
    /// The `if` conditions alone, as `fallback::build_testing` reads them.
    pub(crate) if_conditions: Vec<String>,
    pub(crate) in_test_dir: bool,
}

/// What one file's commands hold.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FileSearches {
    pub(crate) searches: Vec<PackageSearch>,
    /// Searches whose name could not be read: a `${…}` reference, or
    /// characters the old reader never accepted. C237 `unresolved`.
    pub(crate) unresolved: usize,
}

const PKG_CONFIG_MODIFIERS: [&str; 6] =
    ["REQUIRED", "IMPORTED_TARGET", "GLOBAL", "QUIET", "NO_CMAKE_PATH", "NO_CMAKE_ENVIRONMENT_PATH"];

/// `find_package` keywords after which the following arguments are not
/// components. `REQUIRED`, `COMPONENTS` and `OPTIONAL_COMPONENTS` start a
/// component list.
const FIND_PACKAGE_KEYWORDS: [&str; 17] = [
    "EXACT",
    "QUIET",
    "MODULE",
    "CONFIG",
    "NO_MODULE",
    "NO_POLICY_SCOPE",
    "GLOBAL",
    "BYPASS_PROVIDER",
    "NAMES",
    "CONFIGS",
    "HINTS",
    "PATHS",
    "PATH_SUFFIXES",
    "REGISTRY_VIEW",
    "NO_DEFAULT_PATH",
    "CMAKE_FIND_ROOT_PATH_BOTH",
    "ONLY_CMAKE_FIND_ROOT_PATH",
];

fn re(cell: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    cell.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

fn valid_name(name: &str) -> bool {
    static NAME: OnceLock<Option<Regex>> = OnceLock::new();
    re(&NAME, r"^[A-Za-z0-9_:.+-]+$").is_some_and(|r| r.is_match(name))
}

fn valid_version(v: &str) -> bool {
    static VERSION: OnceLock<Option<Regex>> = OnceLock::new();
    re(&VERSION, r"^[0-9][A-Za-z0-9._-]*(?:<[0-9][A-Za-z0-9._-]*)?$").is_some_and(|r| r.is_match(v))
}

/// A pkg-config module token without its version suffix.
fn pkg_config_module(token: &str) -> Option<String> {
    static MODULE: OnceLock<Option<Regex>> = OnceLock::new();
    let caps = re(&MODULE, r"^([A-Za-z0-9_.+-]+)(?:[<>=]=?.*)?$")?.captures(token)?;
    Some(caps.get(1)?.as_str().to_string()).filter(|m| !m.is_empty())
}

/// The components a `find_package` call requests.
fn components(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut collecting = false;
    for a in args.iter().skip(1) {
        match a.as_str() {
            "REQUIRED" | "COMPONENTS" | "OPTIONAL_COMPONENTS" => collecting = true,
            k if FIND_PACKAGE_KEYWORDS.contains(&k) => collecting = false,
            other if collecting => out.push(other.to_string()),
            _ => {}
        }
    }
    out
}

/// The package searches one command makes: `None` when it is not a search;
/// otherwise the searches read and the number left unresolved. `name` is
/// lowercase.
fn parse(name: &str, args: &[String]) -> Option<(Vec<(SearchKind, String, Option<String>, Vec<String>)>, usize)> {
    match name {
        "find_package" => {
            let Some(pkg) = args.first() else { return Some((Vec::new(), 1)) };
            if pkg.contains("${") || !valid_name(pkg) {
                return Some((Vec::new(), 1));
            }
            let requirement = args.get(1).filter(|v| valid_version(v)).map(|v| {
                let exact = args.iter().skip(2).any(|a| a == "EXACT");
                super::find_package_requirement(v, exact)
            });
            Some((vec![(SearchKind::FindPackage, pkg.clone(), requirement, components(args))], 0))
        }
        "pkg_check_modules" | "pkg_search_module" => {
            let (mut found, mut unresolved) = (Vec::new(), 0);
            for token in args.iter().skip(1) {
                if PKG_CONFIG_MODIFIERS.contains(&token.to_ascii_uppercase().as_str()) {
                    continue;
                }
                if token.contains("${") {
                    unresolved += 1;
                    continue;
                }
                if let Some(module) = pkg_config_module(token) {
                    found.push((SearchKind::PkgConfig, module, None, Vec::new()));
                }
            }
            Some((found, unresolved))
        }
        _ => None,
    }
}

/// The searches one command makes under `blocks`, which the caller has
/// already advanced past every earlier command of the file (one
/// [`BlockStack`] per file, seeded with the conditions reaching it). A
/// command that opens or closes a block is not a search.
pub(crate) fn collect(
    blocks: &BlockStack,
    name: &str,
    args: &[String],
    source_path: &str,
    in_test_dir: bool,
    out: &mut FileSearches,
) {
    let Some((found, unresolved)) = parse(name, args) else { return };
    out.unresolved += unresolved;
    let (conditions, if_conditions) = (blocks.conditions(), blocks.if_conditions());
    out.searches.extend(found.into_iter().map(|(kind, name, requirement, components)| PackageSearch {
        kind,
        name,
        requirement,
        components,
        source_path: source_path.to_string(),
        conditions: conditions.clone(),
        if_conditions: if_conditions.clone(),
        in_test_dir,
    }));
}

/// The package searches in one file read on its own: no fetches, no
/// wrappers. For a narrow-set file the shared walker skipped (a `vendor/`
/// under `cmake/`), which milestone 156 read and FR-009 keeps.
pub(crate) fn read_file(path: &std::path::Path, seed: Vec<String>, in_test_dir: bool) -> FileSearches {
    use crate::scan_fs::package_db::installed_lib::cmake_version::{commands_checked, strip_comments};
    let mut out = FileSearches::default();
    let Ok(content) = std::fs::read_to_string(path) else { return out };
    let source_path = path.to_string_lossy();
    let mut blocks = BlockStack::seeded(seed);
    for (name, args) in commands_checked(&strip_comments(&content)).0 {
        let name = name.to_ascii_lowercase();
        let args: Vec<String> = args.into_iter().map(|a| a.text).collect();
        if !blocks.observe(&name, &args) {
            collect(&blocks, &name, &args, &source_path, in_test_dir, &mut out);
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::installed_lib::cmake_version::{commands_checked, strip_comments};

    fn read(text: &str, seed: &[&str]) -> FileSearches {
        let mut blocks = BlockStack::seeded(seed.iter().map(|s| s.to_string()).collect());
        let mut out = FileSearches::default();
        for (n, a) in commands_checked(&strip_comments(text)).0 {
            let args: Vec<String> = a.into_iter().map(|x| x.text).collect();
            let n = n.to_ascii_lowercase();
            if !blocks.observe(&n, &args) {
                collect(&blocks, &n, &args, "CMakeLists.txt", false, &mut out);
            }
        }
        out
    }

    fn one(text: &str) -> PackageSearch {
        let r = read(text, &[]);
        assert_eq!(r.searches.len(), 1, "{r:?}");
        r.searches.into_iter().next().unwrap()
    }

    #[test]
    fn version_exact_and_range() {
        assert_eq!(one("find_package(Foo)").requirement, None);
        assert_eq!(one("find_package(Foo 2.1)").requirement.as_deref(), Some(">=2.1"));
        assert_eq!(one("find_package(Foo 2.1 EXACT REQUIRED)").requirement.as_deref(), Some("==2.1"));
        assert_eq!(one("find_package(Foo 3.9...<4.0)").requirement.as_deref(), Some(">=3.9, <4.0"));
        assert_eq!(one("FIND_PACKAGE(Foo REQUIRED)").requirement, None, "a keyword is not a version");
        assert_eq!(one("find_package(Foo REQUIRED)").name, "Foo");
    }

    #[test]
    fn blocks_bodies_and_else() {
        let r = read(
            "if(A)\n find_package(a)\nelseif(B)\n find_package(b)\nelse()\n find_package(c)\nendif()\n\
             function(deps)\n find_package(d)\nendfunction()\nfind_package(e)\n",
            &["SEED"],
        );
        let got: Vec<(String, Vec<String>)> = r.searches.iter().map(|s| (s.name.clone(), s.conditions.clone())).collect();
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            got,
            vec![
                ("a".into(), v(&["SEED", "A"])),
                ("b".into(), v(&["SEED", "B"])),
                ("c".into(), v(&["SEED", "else: B"])),
                ("d".into(), v(&["SEED", "function: deps"])),
                ("e".into(), v(&["SEED"])),
            ]
        );
        assert_eq!(r.searches[3].if_conditions, v(&["SEED"]), "a function body is not an if");
    }

    #[test]
    fn components_after_required_and_components() {
        assert_eq!(one("find_package(Python3 COMPONENTS Interpreter)").components, vec!["Interpreter"]);
        assert_eq!(
            one("find_package(Python3 3.8 REQUIRED Interpreter Development.Module)").components,
            vec!["Interpreter", "Development.Module"]
        );
        assert_eq!(
            one("find_package(Boost REQUIRED COMPONENTS system OPTIONAL_COMPONENTS thread CONFIG HINTS /opt)").components,
            vec!["system", "thread"]
        );
    }

    #[test]
    fn unresolved_names_comments_and_multiline_calls() {
        let r = read("find_package(${PKG})\nfind_package(\"bad name\")\n# find_package(commented)\nfind_package(\n  Multi\n  1.0\n)\n", &[]);
        assert_eq!(r.unresolved, 2);
        assert_eq!(r.searches.len(), 1);
        assert_eq!((r.searches[0].name.as_str(), r.searches[0].requirement.as_deref()), ("Multi", Some(">=1.0")));
    }

    #[test]
    fn pkg_config_modules_modifiers_and_suffixes() {
        let r = read("pkg_check_modules(X REQUIRED IMPORTED_TARGET libfoo>=1.0 bar)\npkg_search_module(Y QUIET baz<2 ${M})\n", &[]);
        let names: Vec<&str> = r.searches.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["libfoo", "bar", "baz"]);
        assert!(r.searches.iter().all(|s| s.kind == SearchKind::PkgConfig));
        assert_eq!(r.unresolved, 1);
    }
}
