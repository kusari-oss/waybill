//! Spec 1653 (#1535): read a CMake build tree after a build, to settle what
//! source scanning can only mark as possible.
//!
//! The operator names build trees with `--cmake-build-tree` (exported to
//! this reader as `WAYBILL_CMAKE_BUILD_TREES`). Each tree is walked once
//! ([`discover`]); every `CMakeCache.txt` is a build directory, unless its
//! `CMAKE_HOME_DIRECTORY` lies inside a named tree, which makes it a
//! sub-build of downloaded content (FetchContent's `-subbuild`, an
//! ExternalProject's `-build`, a super-build's fetched third-party source).
//! Contracts: `specs/1653-cmake-build-tree/contracts/`.

pub(crate) mod cache;
pub(crate) mod compile_db;
pub(crate) mod discover;
pub(crate) mod redact;
pub(crate) mod settle;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cache::CacheView;
use redact::{RedactedPath, Redactor};

/// C245 `waybill:cmake-build-settlement`.
pub(crate) const ANN_CMAKE_BUILD_SETTLEMENT: &str = "waybill:cmake-build-settlement";
/// C246 `waybill:cmake-build-tree-coverage`.
pub(crate) const ANN_CMAKE_BUILD_TREE_COVERAGE: &str = "waybill:cmake-build-tree-coverage";
/// The side channel `--cmake-build-tree` sets (contracts/cli.md).
pub(crate) const ENV_BUILD_TREES: &str = "WAYBILL_CMAKE_BUILD_TREES";

/// One configured CMake project of the scanned source.
#[derive(Debug)]
pub(crate) struct BuildDir {
    pub(crate) root: PathBuf,
    /// `CMAKE_HOME_DIRECTORY`, canonical. `None`: owns no declarations.
    pub(crate) home: Option<PathBuf>,
    pub(crate) cache: CacheView,
    /// FetchContent populations, lower-cased names (`_deps/<lc>-src`).
    pub(crate) populations: BTreeSet<String>,
    /// ExternalProject names with a `<name>-download` stamp.
    pub(crate) downloads: BTreeSet<String>,
    /// ExternalProject names with a `<name>-stamp` directory.
    pub(crate) stamp_dirs: BTreeSet<String>,
    /// US3: each ExternalProject's default source directory, the sibling of
    /// its stamp directory (`<prefix>/src/<name>`), canonical → name.
    pub(crate) ep_sources: BTreeMap<PathBuf, String>,
    /// US3: `<name>_DIR` cache entries that name a package configuration
    /// directory, name as cached → canonical value.
    pub(crate) package_dirs: BTreeMap<String, PathBuf>,
    /// US3: the translation units of `compile_commands.json`. Empty when
    /// there is none, or it is empty or malformed.
    pub(crate) units: Vec<compile_db::Unit>,
}

/// C246, filled while reading and settling.
#[derive(Debug, Default, Clone)]
pub(crate) struct BuildTreeCoverage {
    pub(crate) build_trees: usize,
    pub(crate) build_dirs: usize,
    pub(crate) declarations: BTreeMap<&'static str, usize>,
    pub(crate) build_only: usize,
    pub(crate) configured_values_applied: usize,
    pub(crate) compile_edges: usize,
    pub(crate) include_dirs_unresolved: usize,
    pub(crate) file_api_replies: usize,
    pub(crate) conflicts: usize,
    pub(crate) signal_disagreements: usize,
    pub(crate) source_mismatch: usize,
    pub(crate) unreadable_files: usize,
}

impl BuildTreeCoverage {
    /// Sorted-key JSON (contracts/annotations.md).
    pub(crate) fn wire(&self) -> String {
        let decl = |k: &str| self.declarations.get(k).copied().unwrap_or(0);
        serde_json::json!({
            "build-dirs": self.build_dirs,
            "build-only": self.build_only,
            "build-trees": self.build_trees,
            "compile-edges": self.compile_edges,
            "configured-values-applied": self.configured_values_applied,
            "conflicts": self.conflicts,
            "declarations": {
                "incomplete": decl("incomplete"),
                "no-evidence": decl("no-evidence"),
                "not-taken": decl("not-taken"),
                "taken": decl("taken"),
            },
            "file-api-replies": self.file_api_replies,
            "include-dirs-unresolved": self.include_dirs_unresolved,
            "signal-disagreements": self.signal_disagreements,
            "source-mismatch": self.source_mismatch,
            "unreadable-files": self.unreadable_files,
        })
        .to_string()
    }
}

pub(crate) struct BuildEvidence {
    pub(crate) dirs: Vec<BuildDir>,
    pub(crate) redactor: Redactor,
    pub(crate) coverage: BuildTreeCoverage,
    /// Git submodule directories (canonical) → PURL, for compile-edge
    /// resolution rule 4. Set by the caller; empty without submodules.
    pub(crate) submodules: BTreeMap<PathBuf, String>,
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn file_name(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

impl BuildEvidence {
    /// The trees named through [`ENV_BUILD_TREES`], or `None` without the flag.
    pub(crate) fn from_env(scan_root: &Path) -> Option<Self> {
        let raw = std::env::var_os(ENV_BUILD_TREES)?;
        let trees: Vec<PathBuf> = std::env::split_paths(&raw).filter(|p| !p.as_os_str().is_empty()).collect();
        (!trees.is_empty()).then(|| Self::load(&trees, scan_root))
    }

    pub(crate) fn load(trees: &[PathBuf], scan_root: &Path) -> Self {
        let mut coverage = BuildTreeCoverage { build_trees: trees.len(), ..Default::default() };
        let canonical_trees: Vec<PathBuf> = trees.iter().map(|t| canonical(t)).collect();
        let canonical_root = canonical(scan_root);
        let in_a_tree = |p: &Path| canonical_trees.iter().any(|t| p.starts_with(t));
        let mut dirs: Vec<BuildDir> = Vec::new();
        // Stamps and populations, attributed after the build directories are
        // known: to the deepest build directory of the scanned source that
        // contains them (contracts/settlement.md, matching rule 1).
        let mut pending: Vec<(PathBuf, Kind)> = Vec::new();
        for tree in trees {
            let found = discover::discover(tree);
            for c in &found.caches {
                let Some(root) = c.parent() else { continue };
                let cache = match std::fs::read_to_string(c).map_err(|_| ()).and_then(|t| CacheView::parse(&t).map_err(|_| ())) {
                    Ok(v) => v,
                    Err(()) => {
                        coverage.unreadable_files += 1;
                        continue;
                    }
                };
                let home = cache.home().map(|h| canonical(Path::new(h)));
                if home.as_deref().is_some_and(in_a_tree) {
                    continue; // a sub-build of downloaded content
                }
                if home.as_deref().is_some_and(|h| !h.starts_with(&canonical_root)) {
                    coverage.source_mismatch += 1;
                }
                let units = match read_units(&root.join("compile_commands.json")) {
                    Ok(u) => u,
                    Err(()) => {
                        coverage.unreadable_files += 1;
                        Vec::new()
                    }
                };
                let package_dirs = package_dirs(&cache);
                dirs.push(BuildDir {
                    root: canonical(root),
                    home,
                    cache,
                    populations: BTreeSet::new(),
                    downloads: BTreeSet::new(),
                    stamp_dirs: BTreeSet::new(),
                    ep_sources: BTreeMap::new(),
                    package_dirs,
                    units,
                });
            }
            pending.extend(found.populations.into_iter().map(|p| (p, Kind::Population)));
            pending.extend(found.downloads.into_iter().map(|p| (p, Kind::Download)));
            pending.extend(found.stamp_dirs.into_iter().map(|p| (p, Kind::StampDir)));
        }
        for (p, kind) in pending {
            let cp = canonical(&p);
            let Some(i) = dirs
                .iter()
                .enumerate()
                .filter(|(_, d)| cp.starts_with(&d.root))
                .max_by_key(|(_, d)| d.root.components().count())
                .map(|(i, _)| i)
            else {
                continue;
            };
            let n = file_name(&p);
            let name = match kind {
                Kind::Population => n.strip_suffix("-src").map(str::to_string),
                Kind::Download => n.strip_suffix("-download").map(str::to_string),
                Kind::StampDir => n.strip_suffix("-stamp").map(str::to_string),
            };
            // FetchContent populates through an ExternalProject named
            // `<lc>-populate` inside its `-subbuild`: not a declaration.
            let Some(name) = name.filter(|n| !n.is_empty() && !n.ends_with("-populate")) else { continue };
            let d = &mut dirs[i];
            if matches!(kind, Kind::StampDir) {
                if let Some(src) = cp.parent() {
                    d.ep_sources.insert(src.join(&name), name.clone());
                }
            }
            match kind {
                Kind::Population => d.populations.insert(name),
                Kind::Download => d.downloads.insert(name),
                Kind::StampDir => d.stamp_dirs.insert(name),
            };
        }
        dirs.sort_by(|a, b| a.root.cmp(&b.root));
        coverage.build_dirs = dirs.len();
        let redactor = Redactor::new(trees, scan_root);
        let ev = Self { dirs, redactor, coverage, submodules: BTreeMap::new() };
        tracing::info!(
            target: "waybill::cmake_build_tree",
            build_trees = ev.coverage.build_trees,
            build_dirs = ev.coverage.build_dirs,
            source_mismatch = ev.coverage.source_mismatch,
            unreadable_files = ev.coverage.unreadable_files,
            "cmake build tree: read (spec 1653)"
        );
        ev
    }

    /// The build directories that own a declaration in `decl_file`: those
    /// whose source directory is the deepest ancestor of it.
    pub(crate) fn owners(&self, decl_file: &Path) -> Vec<usize> {
        // The directory is canonicalised, not the file, so a symlinked root
        // (`/var` → `/private/var`) resolves even for a path not on disk.
        let f = match (decl_file.parent(), decl_file.file_name()) {
            (Some(dir), Some(name)) => canonical(dir).join(name),
            _ => canonical(decl_file),
        };
        let depth = self
            .dirs
            .iter()
            .filter_map(|d| d.home.as_ref().filter(|h| f.starts_with(h)).map(|h| h.components().count()))
            .max();
        let Some(depth) = depth else { return Vec::new() };
        self.dirs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.home.as_ref().is_some_and(|h| f.starts_with(h) && h.components().count() == depth))
            .map(|(i, _)| i)
            .collect()
    }

    /// Spec 1653 US2 (FR-005): the configured values in force for a file:
    /// its owning build directories' condition inputs, merged. Values that
    /// differ between owners conflict and read as unknown.
    pub(crate) fn overlay_for(&self, decl_file: &Path) -> super::conditions::ConfiguredOverlay {
        let mut o = super::conditions::ConfiguredOverlay::default();
        for i in self.owners(decl_file) {
            for (k, v) in self.dirs[i].cache.condition_inputs() {
                o.add(k, v);
            }
        }
        o
    }

    pub(crate) fn dir_id(&self, i: usize) -> RedactedPath {
        self.redactor.redact(&self.dirs[i].root)
    }

    #[cfg(test)]
    pub(crate) fn for_tests(dirs: Vec<BuildDir>) -> Self {
        Self {
            dirs,
            redactor: Redactor::new(&[PathBuf::from("/b")], Path::new("/src")),
            coverage: BuildTreeCoverage::default(),
            submodules: BTreeMap::new(),
        }
    }
}

/// `compile_commands.json`'s units. Absent or 0 bytes is none
/// (measurements/therock.md); unreadable or malformed is `Err`.
fn read_units(p: &Path) -> Result<Vec<compile_db::Unit>, ()> {
    match std::fs::read_to_string(p) {
        Ok(text) => compile_db::units(&text).map_err(|_| ()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err(()),
    }
}

/// `<name>_DIR` entries that hold a package configuration directory:
/// not FetchContent's `_SOURCE_DIR`/`_BINARY_DIR`, not CMake's own, not
/// unset, and not a FetchContent redirect (research R2).
fn package_dirs(cache: &CacheView) -> BTreeMap<String, PathBuf> {
    cache
        .entries_named(|k| {
            k.ends_with("_DIR") && !k.ends_with("_SOURCE_DIR") && !k.ends_with("_BINARY_DIR") && !k.starts_with("CMAKE_") && !k.starts_with("FETCHCONTENT_")
        })
        .filter(|(_, v)| !v.is_empty() && !v.ends_with("-NOTFOUND") && !v.ends_with("CMakeFiles/pkgRedirects"))
        .filter_map(|(k, v)| Some((k.strip_suffix("_DIR")?.to_string(), compile_db::canonical_lenient(Path::new(v)))))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

enum Kind {
    Population,
    Download,
    StampDir,
}

/// C245's value for a settled declaration (contracts/annotations.md).
pub(crate) fn settlement_value(ev: &BuildEvidence, s: &settle::Settlement, build_only: bool) -> serde_json::Value {
    let mut v = serde_json::json!({
        "basis": s.basis.iter().map(|b| b.as_str()).collect::<BTreeSet<_>>(),
        "build-only": build_only,
        "state": s.state.as_str(),
    });
    let builds: BTreeSet<String> = s.dirs.iter().map(|&i| ev.dir_id(i).as_str().to_string()).collect();
    if !builds.is_empty() {
        v["builds"] = serde_json::json!(builds);
    }
    v
}

/// Merge two C245 values of declarations that became one component:
/// the stronger state, the union of basis and builds.
pub(crate) fn merge_settlement(a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
    let state = |v: &serde_json::Value| v["state"].as_str().and_then(settle::SettlementState::parse).unwrap_or(settle::SettlementState::NoEvidence);
    let strs = |v: &serde_json::Value, k: &str| -> BTreeSet<String> {
        v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect()
    };
    let (sa, sb) = (state(a), state(b));
    let mut basis = BTreeSet::new();
    if sa >= sb {
        basis.extend(strs(a, "basis"));
    }
    if sb >= sa {
        basis.extend(strs(b, "basis"));
    }
    let mut builds = strs(a, "builds");
    builds.extend(strs(b, "builds"));
    let mut v = serde_json::json!({
        "basis": basis,
        "build-only": a["build-only"] == true && b["build-only"] == true,
        "state": sa.max(sb).as_str(),
    });
    if !builds.is_empty() {
        v["builds"] = serde_json::json!(builds);
    }
    v
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn touch(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn sub_builds_are_not_build_directories_but_their_evidence_counts() {
        let d = tempfile::tempdir().unwrap();
        let (src, b) = (d.path().join("src"), d.path().join("b"));
        std::fs::create_dir_all(&src).unwrap();
        let home = canonical(&src);
        touch(&b.join("CMakeCache.txt"), &format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", home.display()));
        touch(&b.join("_deps/zlib-subbuild/CMakeCache.txt"), &format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", canonical(&b).join("_deps/zlib-subbuild").display()));
        std::fs::create_dir_all(b.join("_deps/zlib-src")).unwrap();
        touch(&b.join("_deps/zlib-subbuild/zlib-populate-prefix/src/zlib-populate-stamp/zlib-populate-download"), "");
        touch(&b.join("ep-prefix/src/ep-stamp/ep-download"), "");
        touch(&b.join("ep-prefix/src/ep-build/CMakeCache.txt"), &format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", canonical(&b).join("ep-prefix/src/ep").display()));
        let ev = BuildEvidence::load(std::slice::from_ref(&b), &src);
        assert_eq!(ev.dirs.len(), 1, "sub-builds are not build directories");
        let bd = &ev.dirs[0];
        assert_eq!(bd.populations, BTreeSet::from(["zlib".to_string()]));
        assert_eq!(bd.downloads, BTreeSet::from(["ep".to_string()]), "a -populate stamp is FetchContent's own");
        assert_eq!(ev.owners(&src.join("CMakeLists.txt")), vec![0]);
        assert!(ev.owners(&d.path().join("elsewhere/CMakeLists.txt")).is_empty());
        assert_eq!(ev.dir_id(0).as_str(), "build:.");
        assert_eq!(ev.coverage.source_mismatch, 0);
    }

    #[test]
    fn an_unreadable_cache_is_counted() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("b/CMakeCache.txt"), "# nothing\n");
        let ev = BuildEvidence::load(&[d.path().join("b")], d.path());
        assert_eq!(ev.coverage.unreadable_files, 1);
        assert!(ev.dirs.is_empty());
    }

    #[test]
    fn merged_settlements_keep_the_stronger_state() {
        let a = serde_json::json!({"basis":["cache-package-dir"],"build-only":false,"builds":["build:."],"state":"not-taken"});
        let b = serde_json::json!({"basis":["download-stamp"],"build-only":false,"builds":["build:x"],"state":"taken"});
        let m = merge_settlement(&a, &b);
        assert_eq!(m["state"], "taken");
        assert_eq!(m["basis"], serde_json::json!(["download-stamp"]));
        assert_eq!(m["builds"], serde_json::json!(["build:.", "build:x"]));
    }
}
