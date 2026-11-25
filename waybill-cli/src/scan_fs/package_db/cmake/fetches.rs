//! Spec 1538 (#1516, #1519): every `FetchContent_Declare` and
//! `ExternalProject_Add` in the CMake files the shared walker found, each
//! marked with why it might not be taken, and accounted for in C231.
//!
//! - Discovery (research R1): every file, because literal
//!   `add_subdirectory()` reachability reaches 0 of TheRock's 191 calls.
//! - Marking (R2, R3, R7, R13): `LifecycleScope::Optional` with C122 naming
//!   the [`FallbackShape`], C232 the conditions; `LifecycleScope::Test` for a
//!   test directory or `BUILD_TESTING`.
//! - First declaration wins (R5): a `FetchContent_Declare` of a name an
//!   ancestor directory also declares is recorded on the ancestor's
//!   component (C18, C233), never emitted.
//! - One component per PURL (R4): Optional only if every declaration is.
//!
//! Spec 1539 (#1518): a call to a user-defined wrapper (`wrappers.rs`) is
//! bound to the built-in it makes, and becomes the same `Declaration` a
//! direct call is, with the call site's file, conditions and directory, so
//! every rule above applies to it unchanged. The built-in call inside a
//! wrapper's body is a template, not a declaration (FR-008).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

use waybill_common::resolution::LifecycleScope;
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::super::PackageDbEntry;
use super::fallback::{self, Context, FallbackShape, FetchKind, TryMode};
use super::wrappers::{Bound, Definitions, Unbound};

/// C231 `waybill:cmake-fetch-coverage`.
pub(crate) const ANN_CMAKE_FETCH_COVERAGE: &str = "waybill:cmake-fetch-coverage";
/// C232 `waybill:cmake-fetch-conditions`.
pub(crate) const ANN_CMAKE_FETCH_CONDITIONS: &str = "waybill:cmake-fetch-conditions";
/// C233 `waybill:superseded-declarations`.
pub(crate) const ANN_SUPERSEDED: &str = "waybill:superseded-declarations";
/// C234 `waybill:cmake-fetch-wrapper` (spec 1539).
pub(crate) const ANN_CMAKE_FETCH_WRAPPER: &str = "waybill:cmake-fetch-wrapper";
const ANN_OPTIONAL_DERIVATION: &str = "waybill:optional-derivation";
const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";
pub(crate) const TEST_DIRS: [&str; 3] = ["test", "tests", "unittests"];

/// What the fetch pass read (C231), plus what the cross-reader merge
/// (research R9) needs to keep the accounting exact.
#[derive(Debug, Default, Clone)]
pub(crate) struct FetchCoverage {
    pub(crate) read: usize,
    /// Declarations an emitted component stands for.
    pub(crate) emitted: usize,
    /// Emitted fetch components.
    pub(crate) components: usize,
    pub(crate) conditional: usize,
    pub(crate) test: usize,
    pub(crate) merged_first_declaration_wins: usize,
    pub(crate) merged_existing_component: usize,
    /// A name built from a variable, or no `URL` / `GIT_REPOSITORY` at all.
    pub(crate) unresolved_identity: usize,
    pub(crate) unparseable_files: usize,
    /// Declarations each emitted component stands for, by PURL.
    pub(crate) declarations: BTreeMap<String, usize>,
    /// Spec 1539 R9, outside V1: wrapper definitions (their body calls are
    /// templates, not declarations), wrapper call sites (each one
    /// declaration in `read`), and calls whose definitions disagree (each
    /// also in `unresolved_identity`).
    pub(crate) wrapper_definitions: usize,
    pub(crate) wrapper_calls: usize,
    pub(crate) wrapper_ambiguous_calls: usize,
    /// Spec 1543 (#1602): link declarations read from the same files, for
    /// `scan_fs::cmake_link_attach`. Not part of C231.
    pub(crate) links: Vec<super::links::LinkDeclaration>,
    /// Spec 1614 (C237): the package-search accounting, finished by
    /// `scan_fs::cmake_search_pair` once every reader has run.
    pub(crate) finds: super::searches::FindCoverage,
    /// Spec 1614 research R7.1: (fetch PURL, search file) for each search a
    /// fetch answers. Added to the fetch's evidence only after
    /// [`merge_into_existing`]: that merge reads a fetch's source files as
    /// where it is declared, and a search file is not a fetch declaration.
    pub(crate) search_evidence: Vec<(String, String)>,
    /// Spec 1653 (C246): build-tree evidence counts, when `--cmake-build-tree`
    /// named a tree. Not part of C231.
    pub(crate) build_tree: Option<super::build_tree::BuildTreeCoverage>,
}

impl FetchCoverage {
    /// Sorted-key JSON, or `None` when no declaration was read (I-4).
    /// `wrappers` is present only when a wrapper was defined, so a tree
    /// without one keeps spec 1538's value byte for byte (spec 1539 I-7).
    pub(crate) fn wire(&self) -> Option<String> {
        (self.read > 0).then(|| {
            let mut v = serde_json::json!({
                "components": self.components,
                "conditional": self.conditional,
                "emitted": self.emitted,
                "merged": {
                    "existing-component": self.merged_existing_component,
                    "first-declaration-wins": self.merged_first_declaration_wins,
                },
                "read": self.read,
                "test": self.test,
                "unparseable-files": self.unparseable_files,
                "unresolved": { "identity": self.unresolved_identity },
            });
            if self.wrapper_definitions > 0 {
                v["wrappers"] = serde_json::json!({
                    "ambiguous-calls": self.wrapper_ambiguous_calls,
                    "calls": self.wrapper_calls,
                    "definitions": self.wrapper_definitions,
                });
            }
            v.to_string()
        })
    }
}

struct Declaration {
    kind: FetchKind,
    name: String,
    rel: PathBuf,
    entry: Option<PackageDbEntry>,
}

/// One parsed CMake file: absolute path, path relative to the scan root,
/// raw text, and its commands (lowercased name, argument texts).
type ParsedFile = (PathBuf, PathBuf, String, super::reach::Commands);

/// An entry and the number of declarations it stands for. Counted, never
/// inferred from its paths: a first-declaration-wins merge adds the loser's
/// file to the winner's paths without the loser being emitted.
type Counted = (PackageDbEntry, usize);

/// `p` relative to the scan root, without `.`.
fn relative(p: &Path, scan_root: &Path, canonical_root: &Path) -> PathBuf {
    let rel = p.strip_prefix(scan_root).or_else(|_| p.strip_prefix(canonical_root)).unwrap_or(p);
    rel.components().filter(|c| matches!(c, Component::Normal(_))).collect()
}

/// The mode set by the nearest of `dir` and its ancestors.
fn mode_for(dir: &Path, modes: &BTreeMap<PathBuf, TryMode>) -> TryMode {
    dir.ancestors().find_map(|d| modes.get(d).copied()).unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn read_fetches(scan_root: &Path, files: &[PathBuf]) -> (Vec<PackageDbEntry>, FetchCoverage) {
    let (entries, cov, _) = read_declarations(scan_root, files, None);
    (entries, cov)
}

/// Spec 1614 (#1591, #1605): what [`read_declarations`] reads besides fetches.
#[derive(Debug, Default)]
pub(crate) struct Declarations {
    /// Every package search in the files, in file order (`searches.rs`).
    pub(crate) searches: Vec<super::searches::PackageSearch>,
    /// Package searches whose name could not be read (C237 `unresolved`).
    pub(crate) unresolved_searches: usize,
    /// Each fetch's declared name (lowercased) and the PURL of the component
    /// it is (research R7.1).
    pub(crate) fetch_purls: BTreeMap<String, String>,
    /// `project()` names, lowercased (research R9).
    pub(crate) projects: BTreeSet<String>,
    /// Spec 1618 (#1616): each top-level `project()` in a `CMakeLists.txt`,
    /// with its version (research R1).
    pub(crate) project_decls: Vec<super::projects::ProjectDeclaration>,
    /// Spec 1653 (FR-007): (build directory index, settlement key) of every
    /// declaration a build directory owns, so a population or stamp none of
    /// them accounts for is known to be build-only.
    pub(crate) build_matched: BTreeSet<(usize, String)>,
}

/// [`read_fetches`], plus the package searches in the same files, read from
/// the same parsed commands under the same blocks.
pub(crate) fn read_declarations(
    scan_root: &Path,
    files: &[PathBuf],
    build_tree: Option<&super::build_tree::BuildEvidence>,
) -> (Vec<PackageDbEntry>, FetchCoverage, Declarations) {
    let mut found = super::searches::FileSearches::default();
    let mut projects: BTreeSet<String> = BTreeSet::new();
    let mut project_decls: Vec<super::projects::ProjectDeclaration> = Vec::new();
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut cov = FetchCoverage { build_tree: build_tree.map(|ev| ev.coverage.clone()), ..Default::default() };
    let mut build_matched: BTreeSet<(usize, String)> = BTreeSet::new();
    // Pre-pass: every file's commands, the mode each directory sets, and
    // whether any file defines a FetchContent-capable provider (R3).
    let mut parsed: Vec<ParsedFile> = Vec::new();
    let mut modes: BTreeMap<PathBuf, TryMode> = BTreeMap::new();
    let mut provider = false;
    let mut parse_one = |path: &PathBuf, parsed: &mut Vec<ParsedFile>| {
        let Some((cmds, content, unterminated)) = super::reach::parse_commands(path) else {
            tracing::warn!(path = %path.display(), "cmake: a CMake file could not be read (spec 1538 C231)");
            cov.unparseable_files += 1;
            return;
        };
        if unterminated {
            tracing::warn!(path = %path.display(), "cmake: a CMake file ends inside an open command (spec 1538 C231)");
            cov.unparseable_files += 1;
        }
        let rel = relative(path, scan_root, &canonical_root);
        let dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        for (name, args) in cmds.iter() {
            match name.as_str() {
                "set" => {
                    if let Some(m) = fallback::mode_set(args) {
                        modes.insert(dir.clone(), m);
                    }
                }
                "cmake_language" => provider |= fallback::fetchcontent_provider(args),
                _ => {}
            }
        }
        parsed.push((path.clone(), rel, content, cmds));
    };
    for path in files {
        parse_one(path, &mut parsed);
    }
    // Spec 1614 (#1605): the conditions under which each file is reached
    // through include() / add_subdirectory() enclose all its declarations.
    // Computed from the commands just parsed (research R12).
    let walked: Vec<(PathBuf, super::reach::Commands)> = parsed.iter().map(|(p, _, _, c)| (p.clone(), c.clone())).collect();
    let reach = super::reach::compute(scan_root, &walked);
    // FR-004 (research R2): a file `include()` reaches under a name the
    // walker does not match (`CMakeLists_wsl.txt`) is read too.
    for path in &reach.extra_files {
        parse_one(path, &mut parsed);
    }

    // Spec 1539: every function and macro, and which are wrappers. A body
    // call that names a package on its own is spec 1538's function-body
    // fetch, never a wrapper (research R2).
    let vars_of: HashMap<&Path, Vars> =
        parsed.iter().map(|(_, rel, content, _)| (rel.as_path(), super::collect_set_vars(content))).collect();
    let no_vars = Vars::new();
    // Spec 1638 research R2: a GitHub repository with an unresolved reference
    // names a declaration whose name is unexpanded (FR-008), but it is not an
    // identity of its own for spec 1539 R2. On `main` no such declaration had
    // an identity, so every wrapper stays one and keeps its call sites (C234).
    let defs = Definitions::collect(parsed.iter().map(|(_, rel, _, cmds)| (rel.as_path(), cmds.as_slice())), |file, kind, args| {
        declared(kind, args, "", vars_of.get(file).unwrap_or(&no_vars))
            .is_some_and(|d| !(d.name.is_empty() && d.entry.extra_annotations.contains_key("waybill:unresolved-reason")))
    });
    cov.wrapper_definitions = defs.wrapper_count();

    // Spec 1618 FR-009: only files the walker handed over declare projects,
    // so its skip set and `--exclude-path` hold; an include()-reached extra
    // file does not.
    let walked_files: std::collections::HashSet<&PathBuf> = files.iter().collect();
    let mut decls: Vec<Declaration> = Vec::new();
    for (path, rel, _, cmds) in &parsed {
        let vars = vars_of.get(rel.as_path()).unwrap_or(&no_vars);
        let source_path = path.to_string_lossy().into_owned();
        let dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        let in_test_dir = rel.parent().is_some_and(|d| {
            d.components().any(|c| matches!(c, Component::Normal(s) if TEST_DIRS.iter().any(|t| s == *t)))
        });
        let mut blocks = super::blocks::BlockStack::seeded(reach.seed(path));
        // Spec 1641 research R4, R5: the defaults in reach of this file and
        // whether the file itself is reached by default.
        let base = reach.defaults_for(path);
        // Spec 1653 US2 (FR-005): a build's configured values replace the
        // source defaults. `base` stays for counting what they changed (C246).
        let overlay = build_tree.map(|ev| ev.overlay_for(path)).unwrap_or_default();
        let defaults = if overlay.is_empty() { base.clone() } else { base.clone().with_overlay(&overlay) };
        let mut reach_atoms = BTreeSet::new();
        let reach_eval = reach.reach_eval(path, &defaults, &mut reach_atoms);
        let base_reach = if defaults.has_overlay() { reach.reach_eval(path, &base, &mut BTreeSet::new()) } else { reach_eval };
        let mut configured_changes = 0usize;
        let mut changed = |with: super::conditions::Eval, without: super::conditions::Eval| {
            if with != without {
                configured_changes += 1;
            }
        };
        let mut ordinal = 0usize;
        let mut open: Vec<usize> = Vec::new();
        for (name, args) in cmds.iter() {
            match name.as_str() {
                "function" | "macro" => {
                    open.push(ordinal);
                    ordinal += 1;
                }
                "endfunction" | "endmacro" => {
                    open.pop();
                }
                _ => {}
            }
            if blocks.observe(name, args) {
                continue;
            }
            let before = found.searches.len();
            super::searches::collect(&blocks, name, args, &source_path, in_test_dir, &mut found);
            for s in &mut found.searches[before..] {
                let mut atoms = reach_atoms.clone();
                s.eval = defaults.chain(reach_eval, &s.branches, &[], &mut atoms);
                if defaults.has_overlay() {
                    changed(s.eval, base.chain(base_reach, &s.branches, &[], &mut BTreeSet::new()));
                }
                if !s.conditions.is_empty() && !s.in_test_dir && !super::fallback::build_testing(&s.if_conditions) {
                    census("search", &s.name, false, &atoms, s.eval, &s.conditions);
                }
            }
            if name == "project" {
                if let Some(p) = args.first().filter(|p| !p.contains("${")) {
                    projects.insert(p.to_lowercase());
                }
                // Spec 1618: a project component needs a directory, so only a
                // `CMakeLists.txt`'s top-level call declares one.
                let is_lists = rel.file_name().is_some_and(|n| n == "CMakeLists.txt");
                if is_lists && open.is_empty() && walked_files.contains(path) {
                    if let Some((pname, version)) = super::projects::parse_project(args, vars) {
                        project_decls.push(super::projects::ProjectDeclaration {
                            name: pname,
                            version,
                            rel_file: rel.clone(),
                            source_path: source_path.clone(),
                            reached: reach.reached_from_root(path),
                        });
                    }
                }
            }
            let builtin = match name.as_str() {
                "fetchcontent_declare" => Some(FetchKind::FetchContent),
                "externalproject_add" => Some(FetchKind::ExternalProject),
                _ => None,
            };
            if builtin.is_none() && !defs.is_wrapper(name) {
                continue;
            }
            // FR-008: the call inside a wrapper's body is a template.
            if open.iter().any(|&o| defs.is_template(rel, o)) {
                continue;
            }
            cov.read += 1;
            let (kind, decl_args, body_conditions, chain) = match builtin {
                Some(kind) => (kind, args.clone(), Vec::new(), Vec::new()),
                None => {
                    cov.wrapper_calls += 1;
                    let call: Vec<String> = args.iter().map(|a| super::expand_known_vars(a, vars)).collect();
                    let identity = |b: &Bound| declared(b.kind, &b.args, &source_path, vars).map(|d| identity_key(&d.entry));
                    match defs.bind(name, &call, &identity) {
                        Ok(b) => (b.kind, b.args, b.conditions, b.chain),
                        Err(e) => {
                            cov.unresolved_identity += 1;
                            if e == Unbound::Ambiguous {
                                cov.wrapper_ambiguous_calls += 1;
                            }
                            continue;
                        }
                    }
                }
            };
            let Some(Declared { mut entry, name: decl_name }) = declared(kind, &decl_args, &source_path, vars) else {
                cov.unresolved_identity += 1;
                continue;
            };
            if !chain.is_empty() {
                name_from_archive(&mut entry, &decl_args, vars);
                entry.extra_annotations.insert(ANN_CMAKE_FETCH_WRAPPER.to_string(), serde_json::json!([chain]));
            }
            let mut if_conds = blocks.if_conditions();
            if_conds.extend(body_conditions.iter().cloned());
            let mut conditions = blocks.conditions();
            conditions.extend(body_conditions.iter().cloned());
            let ctx = Context { mode: mode_for(&dir, &modes), fetchcontent_provider: provider };
            let mut shape = fallback::classify(kind, &decl_args, &if_conds, blocks.function().is_some(), ctx);
            // Spec 1641 (FR-001, FR-004): the catch-all is re-evaluated; a
            // chain that does not hold by default stays Optional, anything
            // else is in scope with C232 as the record (FR-005).
            let mut kept = false;
            if shape == Some(FallbackShape::Conditional) {
                let mut atoms = reach_atoms.clone();
                let eval = defaults.chain(reach_eval, &blocks.branches(), &body_conditions, &mut atoms);
                if defaults.has_overlay() {
                    changed(eval, base.chain(base_reach, &blocks.branches(), &body_conditions, &mut BTreeSet::new()));
                }
                let versioned = !entry.version.is_empty() && entry.version != "unknown";
                census("fetch", entry.purl.as_str(), versioned, &atoms, eval, &conditions);
                if eval.in_scope() {
                    shape = None;
                    kept = true;
                }
            }
            let test = if in_test_dir {
                Some("source-test-directory")
            } else if fallback::build_testing(&if_conds) {
                Some("cmake-build-testing")
            } else {
                None
            };
            mark(&mut entry, shape, test, &conditions, kept);
            if let (Some(ev), Some(bt)) = (build_tree, cov.build_tree.as_mut()) {
                let raw = decl_args.first().map(|a| super::expand_known_vars(a, vars)).unwrap_or_default();
                if !raw.is_empty() && !raw.contains("${") {
                    settle_declaration(ev, bt, &mut build_matched, kind, &raw, path, &mut entry);
                }
            }
            decls.push(Declaration { kind, name: decl_name, rel: rel.clone(), entry: Some(entry) });
        }
        if let Some(bt) = cov.build_tree.as_mut() {
            bt.configured_values_applied += configured_changes;
        }
    }

    first_declaration_wins(&mut decls, &mut cov);
    let fetch_purls: BTreeMap<String, String> = decls
        .iter()
        .filter(|d| !d.name.is_empty())
        .filter_map(|d| d.entry.as_ref().map(|e| (d.name.clone(), e.purl.as_str().to_string())))
        .collect();
    let entries = one_per_purl(decls.into_iter().filter_map(|d| d.entry).map(|e| (e, 1)).collect(), &mut cov);
    cov.components = entries.len();
    cov.conditional = entries.iter().filter(|e| e.lifecycle_scope == Some(LifecycleScope::Optional)).count();
    cov.test = entries.iter().filter(|e| e.lifecycle_scope == Some(LifecycleScope::Test)).count();
    if cov.read > 0 {
        tracing::info!(
            read = cov.read,
            components = cov.components,
            conditional = cov.conditional,
            test = cov.test,
            first_declaration_wins = cov.merged_first_declaration_wins,
            unresolved = cov.unresolved_identity,
            wrapper_definitions = cov.wrapper_definitions,
            wrapper_calls = cov.wrapper_calls,
            wrapper_ambiguous_calls = cov.wrapper_ambiguous_calls,
            "cmake: fetch declarations (spec 1538, 1539)"
        );
    }
    let decls = Declarations {
        searches: found.searches,
        unresolved_searches: found.unresolved,
        fetch_purls,
        projects,
        project_decls,
        build_matched,
    };
    (entries, cov, decls)
}

/// Spec 1653 (contracts/settlement.md, effect on the component): settle one
/// declaration against the build tree, record C245, and bring a *taken* one
/// into scope. Test scope (spec 1538 R7) outranks it and is kept.
fn settle_declaration(
    ev: &super::build_tree::BuildEvidence,
    bt: &mut super::build_tree::BuildTreeCoverage,
    matched: &mut BTreeSet<(usize, String)>,
    kind: FetchKind,
    raw_name: &str,
    decl_file: &Path,
    entry: &mut PackageDbEntry,
) {
    use super::build_tree::settle::{self, SettlementState};
    let key = settle::key(kind, raw_name);
    for i in ev.owners(decl_file) {
        matched.insert((i, key.clone()));
    }
    let s = settle::settle(ev, kind, raw_name, decl_file);
    *bt.declarations.entry(s.state.as_str()).or_default() += 1;
    bt.conflicts += usize::from(s.conflict);
    bt.signal_disagreements += usize::from(s.signal_disagreement);
    if s.state == SettlementState::Taken && entry.lifecycle_scope == Some(LifecycleScope::Optional) {
        entry.lifecycle_scope = None;
        entry.extra_annotations.remove(ANN_OPTIONAL_DERIVATION);
    }
    entry
        .extra_annotations
        .insert(super::build_tree::ANN_CMAKE_BUILD_SETTLEMENT.to_string(), super::build_tree::settlement_value(ev, &s, false));
}

/// A component the build tree says was taken (C245).
fn taken(e: &PackageDbEntry) -> bool {
    e.extra_annotations
        .get(super::build_tree::ANN_CMAKE_BUILD_SETTLEMENT)
        .is_some_and(|v| v["state"] == "taken")
}

type Vars = HashMap<String, Option<String>>;

/// One declaration read as spec 1538 reads it: the component, and the
/// lowercased name first-declaration-wins keys on (empty when the name is
/// unexpanded, so it never matches). `None` when it has no identity.
struct Declared {
    entry: PackageDbEntry,
    name: String,
}

fn rule_of(kind: FetchKind) -> &'static str {
    match kind {
        FetchKind::FetchContent => "FetchContent_Declare",
        FetchKind::ExternalProject => "ExternalProject_Add",
    }
}

fn declared(kind: FetchKind, args: &[String], source_path: &str, vars: &Vars) -> Option<Declared> {
    let raw_name = args.first().cloned().unwrap_or_default();
    let decl_name = super::expand_known_vars(&raw_name, vars);
    if decl_name.is_empty() {
        return None;
    }
    // A name the reader cannot expand is unresolved only when the identity
    // depends on it: a GitHub `GIT_REPOSITORY` names the package by itself,
    // and the component takes that name.
    let unexpanded = |s: &str| s.contains("${") || s.contains("$<") || s.contains("%24");
    let name_unexpanded = unexpanded(&decl_name);
    let entry = super::fetch_entry(&decl_name, &args.get(1..).unwrap_or_default().join(" "), source_path, rule_of(kind), vars)
        .filter(|e| !unexpanded(e.purl.as_str()))
        .map(|mut e| {
            if name_unexpanded {
                e.name = e.purl.name().to_string();
            }
            e
        })?;
    // An unexpanded name is no name: it never matches under R5.
    let name = if name_unexpanded { String::new() } else { decl_name.to_ascii_lowercase() };
    Some(Declared { entry, name })
}

/// What two definitions of one wrapper must agree on (spec 1539 R5).
fn identity_key(e: &PackageDbEntry) -> String {
    format!("{} {:?}", e.purl.as_str(), e.hashes)
}

/// Spec 1539 FR-005: a wrapped URL fetch is named from its archive when the
/// filename is `<name>-<version>` for the version the URL gave.
fn name_from_archive(entry: &mut PackageDbEntry, args: &[String], vars: &Vars) {
    let body = super::expand_known_vars(&args.get(1..).unwrap_or_default().join(" "), vars);
    if super::extract_keyword(&body, "GIT_REPOSITORY").is_some() || entry.purl.ecosystem() != "generic" {
        return;
    }
    let Some(url) = super::extract_keyword(&body, "URL") else { return };
    if entry.version.is_empty() || entry.version == "unknown" {
        return;
    }
    let Some(n) = super::archive_name(&url, &entry.version) else { return };
    let Ok(purl) = Purl::new(&format!("pkg:generic/{}@{}", encode_purl_segment(&n), encode_purl_segment(&entry.version))) else {
        return;
    };
    entry.purl = purl;
    entry.name = n;
}

fn wrapper_chains(e: &PackageDbEntry) -> BTreeSet<Vec<String>> {
    e.extra_annotations
        .get(ANN_CMAKE_FETCH_WRAPPER)
        .and_then(|v| serde_json::from_value::<Vec<Vec<String>>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Spec 1641 research R6: one census event per catch-all conditional fetch
/// or search, read by the fixture repository's `census/conditions.py`.
fn census(
    kind: &str,
    subject: &str,
    versioned: bool,
    atoms: &BTreeSet<super::conditions::Atom>,
    eval: super::conditions::Eval,
    conditions: &[String],
) {
    tracing::debug!(
        target: "waybill::cmake_conditions",
        kind,
        subject,
        versioned,
        category = super::conditions::category(atoms),
        eval = eval.as_str(),
        conditions = %serde_json::json!(conditions),
        "cmake: a catch-all conditional (spec 1641)"
    );
}

/// Test outranks Optional: a component's scope is one value (R7).
fn mark(entry: &mut PackageDbEntry, shape: Option<FallbackShape>, test: Option<&str>, conditions: &[String], kept: bool) {
    if let Some(derivation) = test {
        entry.lifecycle_scope = Some(LifecycleScope::Test);
        entry.extra_annotations.insert(ANN_SCOPE_DERIVATION.to_string(), serde_json::json!(derivation));
    } else if let Some(shape) = shape {
        entry.lifecycle_scope = Some(LifecycleScope::Optional);
        entry.extra_annotations.insert(ANN_OPTIONAL_DERIVATION.to_string(), serde_json::json!(shape.as_str()));
    }
    if (entry.lifecycle_scope.is_some() || kept) && !conditions.is_empty() {
        entry.extra_annotations.insert(ANN_CMAKE_FETCH_CONDITIONS.to_string(), serde_json::json!(conditions));
    }
}

fn superseded(entry: &mut PackageDbEntry, file: &Path, version: &str) {
    let item = serde_json::json!({ "file": file.to_string_lossy(), "version": version });
    let mut list: Vec<serde_json::Value> = entry
        .extra_annotations
        .get(ANN_SUPERSEDED)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if !list.contains(&item) {
        list.push(item);
    }
    list.sort_by_key(|v| (v["file"].as_str().unwrap_or("").to_string(), v["version"].as_str().unwrap_or("").to_string()));
    entry.extra_annotations.insert(ANN_SUPERSEDED.to_string(), serde_json::Value::Array(list));
}

/// R5: a `FetchContent_Declare` whose name an ancestor directory also
/// declares never runs (probe P1). The shallowest ancestor declaration is
/// the one CMake keeps.
fn first_declaration_wins(decls: &mut [Declaration], cov: &mut FetchCoverage) {
    let mut moves: Vec<(usize, usize)> = Vec::new();
    for (i, d) in decls.iter().enumerate() {
        if d.kind != FetchKind::FetchContent || d.name.is_empty() {
            continue;
        }
        let dir = d.rel.parent().unwrap_or(Path::new(""));
        let winner = decls
            .iter()
            .enumerate()
            .filter(|(j, o)| {
                *j != i && o.kind == FetchKind::FetchContent && o.name == d.name && o.entry.is_some() && {
                    let od = o.rel.parent().unwrap_or(Path::new(""));
                    od != dir && dir.starts_with(od)
                }
            })
            .min_by_key(|(_, o)| o.rel.components().count())
            .map(|(j, _)| j);
        if let Some(j) = winner {
            moves.push((i, j));
        }
    }
    for (loser, winner) in moves {
        let (Some(lost), rel) = (decls[loser].entry.take(), decls[loser].rel.clone()) else { continue };
        if let Some(w) = decls[winner].entry.as_mut() {
            crate::scan_fs::also_detected::record("S4", w.purl.as_str(), &mut w.extra_annotations, &lost.extra_annotations);
            w.extra_source_paths.push(lost.source_path.clone());
            superseded(w, &rel, &lost.version);
        }
        cov.merged_first_declaration_wins += 1;
    }
}

fn conditions_of(e: &PackageDbEntry) -> Vec<String> {
    e.extra_annotations
        .get(ANN_CMAKE_FETCH_CONDITIONS)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// R4: one component per PURL. Its scope is the strongest of its
/// declarations' (feature 1269): required if any is, Optional only if every
/// one is, otherwise Test. Conditions are the sorted union.
fn one_per_purl(entries: Vec<Counted>, cov: &mut FetchCoverage) -> Vec<PackageDbEntry> {
    let mut groups: BTreeMap<String, Vec<PackageDbEntry>> = BTreeMap::new();
    for (e, decls) in entries {
        *cov.declarations.entry(e.purl.as_str().to_string()).or_default() += decls;
        groups.entry(e.purl.as_str().to_string()).or_default().push(e);
    }
    cov.emitted = cov.declarations.values().sum();
    let mut out = Vec::new();
    for (_, mut group) in groups {
        // Spec 1641: a declaration with no scope and no C232 is unconditional;
        // one with no scope and C232 is a catch-all conditional kept in scope.
        // The unconditional one ranks first, so it stays the survivor (and its
        // C55) as on `main`; a kept one still outranks an Optional one (R5).
        let unconditional = |e: &PackageDbEntry| e.lifecycle_scope.is_none() && !e.extra_annotations.contains_key(ANN_CMAKE_FETCH_CONDITIONS);
        let rank = |e: &PackageDbEntry| match e.lifecycle_scope {
            None if unconditional(e) => 0,
            None => 1,
            Some(LifecycleScope::Optional) => 2,
            _ => 3,
        };
        group.sort_by_key(|e| (rank(e), e.source_path.clone()));
        let any_unconditional = group.iter().any(unconditional);
        let mut first = group.remove(0);
        let mut conds: BTreeSet<String> = conditions_of(&first).into_iter().collect();
        let mut chains = wrapper_chains(&first);
        let mut sup: Vec<serde_json::Value> =
            first.extra_annotations.get(ANN_SUPERSEDED).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let mut settled = first.extra_annotations.get(super::build_tree::ANN_CMAKE_BUILD_SETTLEMENT).cloned();
        for other in group {
            // Spec 1653 (T019): a component is taken if any declaration is.
            if let Some(o) = other.extra_annotations.get(super::build_tree::ANN_CMAKE_BUILD_SETTLEMENT) {
                settled = Some(match settled {
                    Some(f) => super::build_tree::merge_settlement(&f, o),
                    None => o.clone(),
                });
            }
            crate::scan_fs::also_detected::record("S5", first.purl.as_str(), &mut first.extra_annotations, &other.extra_annotations);
            conds.extend(conditions_of(&other));
            chains.extend(wrapper_chains(&other));
            first.extra_source_paths.push(other.source_path.clone());
            first.extra_source_paths.extend(other.extra_source_paths.iter().cloned());
            if let Some(a) = other.extra_annotations.get(ANN_SUPERSEDED).and_then(|v| v.as_array()) {
                sup.extend(a.iter().cloned());
            }
        }
        first.extra_source_paths.sort();
        first.extra_source_paths.dedup();
        if (first.lifecycle_scope.is_some() || !any_unconditional) && !conds.is_empty() {
            first.extra_annotations.insert(ANN_CMAKE_FETCH_CONDITIONS.to_string(), serde_json::json!(conds));
        } else {
            first.extra_annotations.remove(ANN_CMAKE_FETCH_CONDITIONS);
        }
        if !chains.is_empty() {
            first.extra_annotations.insert(ANN_CMAKE_FETCH_WRAPPER.to_string(), serde_json::json!(chains));
        }
        if let Some(v) = settled {
            // A taken declaration merged into an Optional survivor brings it
            // into scope, as it would have alone (FR-004).
            if v["state"] == "taken" && first.lifecycle_scope == Some(LifecycleScope::Optional) {
                first.lifecycle_scope = None;
                first.extra_annotations.remove(ANN_OPTIONAL_DERIVATION);
            }
            first.extra_annotations.insert(super::build_tree::ANN_CMAKE_BUILD_SETTLEMENT.to_string(), v);
        }
        if !sup.is_empty() {
            sup.sort_by_key(|v| (v["file"].as_str().unwrap_or("").to_string(), v["version"].as_str().unwrap_or("").to_string()));
            sup.dedup();
            first.extra_annotations.insert(ANN_SUPERSEDED.to_string(), serde_json::Value::Array(sup));
        }
        out.push(first);
    }
    out
}

/// R9: the repository a component stands for, without its version:
/// `github.com/<owner>/<repo>`, or the normalised git URL of a
/// `GIT_REPOSITORY` fetch or a submodule's `vcs_url`. `None` for a URL
/// tarball, whose version cannot be removed reliably.
pub(crate) fn repository_identity(e: &PackageDbEntry) -> Option<String> {
    let purl = e.purl.as_str();
    if e.purl.ecosystem() == "github" {
        return Some(format!("github.com/{}/{}", e.purl.namespace()?, e.purl.name()).to_ascii_lowercase());
    }
    let mechanism = e.extra_annotations.get("waybill:source-mechanism").and_then(|v| v.as_str());
    let url = if mechanism == Some("cmake-fetchcontent-git") {
        e.extra_annotations.get("waybill:download-url").and_then(|v| v.as_str()).map(str::to_string)
    } else {
        purl.split_once('?').and_then(|(_, q)| {
            q.split('&').find_map(|kv| kv.strip_prefix("vcs_url=")).map(percent_decode)
        })
    }?;
    Some(normalise_repository(&url))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn normalise_repository(url: &str) -> String {
    let mut u = url.trim().strip_prefix("git+").unwrap_or(url.trim()).to_string();
    for scheme in ["https://", "http://", "ssh://", "git://", "file://"] {
        if let Some(rest) = u.strip_prefix(scheme) {
            u = rest.to_string();
        }
    }
    if let Some(rest) = u.strip_prefix("git@") {
        u = rest.replacen(':', "/", 1);
    }
    let u = u.trim_end_matches('/');
    let u = u.strip_suffix(".git").unwrap_or(u);
    if u.to_ascii_lowercase().starts_with("github.com/") { u.to_ascii_lowercase() } else { u.to_string() }
}

/// R9: a **conditional** fetch of a repository that an enclosing project
/// already supplies (a git submodule, another reader's component, an
/// unconditional fetch) is recorded on that component, not emitted. The
/// supplier is the one declared in the nearest ancestor-or-self directory;
/// a sibling's component is never a supplier.
pub(crate) fn merge_into_existing(entries: &mut Vec<PackageDbEntry>, cov: &mut FetchCoverage, scan_root: &Path) {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let is_fetch = |e: &PackageDbEntry| {
        e.extra_annotations
            .get("waybill:source-mechanism")
            .and_then(|v| v.as_str())
            .is_some_and(|m| m.starts_with("cmake-fetchcontent") || m == "cmake-externalproject")
    };
    // Spec 1641 (FR-006): conditional is the presence of a condition, not
    // the scope. A fetch spec 1641 keeps in scope carries C232 and no scope.
    // Spec 1653 (T019): a fetch the build took is not a fallback to fold away.
    let conditional = |e: &PackageDbEntry| {
        is_fetch(e)
            && !taken(e)
            && ((e.lifecycle_scope == Some(LifecycleScope::Optional)
                && e.extra_annotations.get(ANN_OPTIONAL_DERIVATION).and_then(|v| v.as_str()).is_some_and(|d| d.starts_with("cmake-")))
                || (e.lifecycle_scope.is_none() && e.extra_annotations.contains_key(ANN_CMAKE_FETCH_CONDITIONS)))
    };
    let files_of = |e: &PackageDbEntry| -> Vec<String> {
        std::iter::once(e.source_path.clone()).chain(e.extra_source_paths.iter().cloned()).collect()
    };
    let dir_of = |f: &str| -> PathBuf {
        relative(Path::new(f), scan_root, &canonical_root).parent().map(Path::to_path_buf).unwrap_or_default()
    };
    // Every place a non-conditional component is declared, by identity.
    let mut holders: BTreeMap<String, Vec<(usize, PathBuf)>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if conditional(e) {
            continue;
        }
        if let Some(id) = repository_identity(e) {
            let at = holders.entry(id).or_default();
            at.extend(files_of(e).iter().map(|f| (i, dir_of(f))));
        }
    }
    let mut required_by_purl: BTreeMap<String, usize> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if !conditional(e) {
            required_by_purl.entry(e.purl.as_str().to_string()).or_insert(i);
        }
    }
    // The supplier for one file: the holder declared in the nearest
    // ancestor-or-self directory. A tie between two components there is
    // no supplier.
    let supplier = |at: &[(usize, PathBuf)], dir: &Path, entries: &[PackageDbEntry]| -> Option<usize> {
        let depth = at.iter().filter(|(_, d)| dir.starts_with(d)).map(|(_, d)| d.components().count()).max()?;
        let mut nearest = at.iter().filter(|(_, d)| dir.starts_with(d) && d.components().count() == depth).map(|(h, _)| *h);
        let h = nearest.next()?;
        nearest.all(|o| entries[o].purl == entries[h].purl).then_some(h)
    };
    let mut remove: Vec<usize> = Vec::new();
    for i in 0..entries.len() {
        if !conditional(&entries[i]) {
            continue;
        }
        // A required component with exactly this PURL is this package
        // wherever it is declared; recording the fallback on it keeps the
        // survivor's own rows, which PURL dedup would not.
        let h = if let Some(&h) = required_by_purl.get(entries[i].purl.as_str()) {
            h
        } else {
            let Some(at) = repository_identity(&entries[i]).and_then(|id| holders.get(&id)) else { continue };
            let mut targets = files_of(&entries[i]).into_iter().map(|f| supplier(at, &dir_of(&f), entries));
            let Some(Some(h)) = targets.next() else { continue };
            if !targets.all(|t| t.is_some_and(|o| entries[o].purl == entries[h].purl)) {
                continue;
            }
            h
        };
        let lost = entries[i].clone();
        // Spec 1639 S6: C233 records the declaration below, C56 the mechanism.
        let subject = entries[h].purl.as_str().to_string();
        crate::scan_fs::also_detected::record("S6", &subject, &mut entries[h].extra_annotations, &lost.extra_annotations);
        let files: Vec<String> = std::iter::once(lost.source_path.clone()).chain(lost.extra_source_paths.iter().cloned()).collect();
        for f in &files {
            let rel = relative(Path::new(f), scan_root, &canonical_root);
            superseded(&mut entries[h], &rel, &lost.version);
            if !entries[h].extra_source_paths.contains(f) && &entries[h].source_path != f {
                entries[h].extra_source_paths.push(f.clone());
            }
        }
        let n = cov.declarations.remove(lost.purl.as_str()).unwrap_or(1);
        cov.emitted -= n.min(cov.emitted);
        cov.merged_existing_component += n;
        cov.components = cov.components.saturating_sub(1);
        cov.conditional = cov.conditional.saturating_sub(1);
        remove.push(i);
    }
    for i in remove.into_iter().rev() {
        entries.remove(i);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn repository_urls_normalise_to_one_identity() {
        assert_eq!(normalise_repository("git+https://github.com/ROCm/rocm-cmake.git"), "github.com/rocm/rocm-cmake");
        assert_eq!(normalise_repository("git@github.com:ROCm/rocm-cmake.git/"), "github.com/rocm/rocm-cmake");
        assert_eq!(normalise_repository("/tmp/x/pdep"), "/tmp/x/pdep");
        assert_eq!(normalise_repository("file:///tmp/x/pdep.git"), "/tmp/x/pdep");
    }

    #[test]
    fn percent_decoding_keeps_plus() {
        assert_eq!(percent_decode("git%2Bhttps%3A%2F%2Fx"), "git+https://x");
        assert_eq!(percent_decode("git+https://x"), "git+https://x");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn wire_is_absent_when_nothing_was_read_and_v1_holds_otherwise() {
        assert!(FetchCoverage::default().wire().is_none());
        let c = FetchCoverage { read: 4, emitted: 2, merged_first_declaration_wins: 1, unresolved_identity: 1, components: 2, ..Default::default() };
        let v: serde_json::Value = serde_json::from_str(&c.wire().unwrap()).unwrap();
        assert_eq!(v["read"], 4);
        assert_eq!(v["merged"]["first-declaration-wins"], 1);
        assert!(v.get("wrappers").is_none(), "no wrapper defined: spec 1538's shape");
        let w = FetchCoverage { wrapper_definitions: 1, wrapper_calls: 2, wrapper_ambiguous_calls: 1, ..c };
        let wire = w.wire().unwrap();
        assert!(wire.contains(r#""wrappers":{"ambiguous-calls":1,"calls":2,"definitions":1}"#), "{wire}");
    }

    #[test]
    fn commented_out_declarations_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("CMakeLists.txt");
        std::fs::write(
            &f,
            "# FetchContent_Declare(waybill-fixture-a URL https://example.invalid/a-1.0.tar.gz)\n\
             #[[ FetchContent_Declare(waybill-fixture-b URL https://example.invalid/b-1.0.tar.gz) ]]\n\
             FetchContent_Declare(waybill-fixture-c URL https://example.invalid/c-1.0.tar.gz)\n",
        )
        .unwrap();
        let (entries, cov) = read_fetches(dir.path(), &[f]);
        assert_eq!(cov.read, 1, "only the live declaration is read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "waybill-fixture-c");
    }

    fn files(dir: &Path, list: &[(&str, &str)]) -> Vec<PathBuf> {
        list.iter()
            .map(|(rel, text)| {
                let f = dir.join(rel);
                std::fs::create_dir_all(f.parent().unwrap()).unwrap();
                std::fs::write(&f, text).unwrap();
                f
            })
            .collect()
    }

    /// T018 / R4: one PURL declared conditionally in one file and
    /// unconditionally in another is required, and carries no C232.
    #[test]
    fn a_required_declaration_keeps_its_purl_required() {
        let dir = tempfile::tempdir().unwrap();
        let decl = "FetchContent_Declare(waybill-fixture-p URL https://example.invalid/waybill-fixture-p-1.0.tar.gz)\n";
        let fs = files(dir.path(), &[("a/CMakeLists.txt", &format!("if(WIN32)\n{decl}endif()\n")), ("b/CMakeLists.txt", decl)]);
        let (entries, cov) = read_fetches(dir.path(), &fs);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lifecycle_scope, None, "required: some build takes it");
        assert!(!entries[0].extra_annotations.contains_key(ANN_CMAKE_FETCH_CONDITIONS));
        assert_eq!((cov.emitted, cov.components, cov.conditional), (2, 1, 0));
    }

    /// Spec 1638 research R2: wrapper recognition is unchanged. A body whose
    /// name is a parameter and whose GitHub tag is unresolved is still a
    /// wrapper, and its call keeps C234. A body with a literal name and an
    /// unresolved tag is still not one.
    #[test]
    fn wrapper_recognition_is_unchanged_by_unresolved_github_identity() {
        let dir = tempfile::tempdir().unwrap();
        let gh = "https://github.com/waybill-org/waybill-fixture-dep.git";
        let fs = files(
            dir.path(),
            &[(
                "CMakeLists.txt",
                &format!(
                    "function(wf_dep noimage project)\nExternalProject_Add(${{project}} GIT_REPOSITORY {gh} GIT_TAG ${{FX_UNSET}})\nendfunction()\n\
                     wf_dep(OFF waybill-fixture-target)\n"
                ),
            )],
        );
        let (entries, cov) = read_fetches(dir.path(), &fs);
        assert_eq!((cov.wrapper_definitions, cov.wrapper_calls), (1, 1), "still a wrapper");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].purl.as_str(), "pkg:github/waybill-org/waybill-fixture-dep");
        assert_eq!(entries[0].name, "waybill-fixture-target");
        assert!(entries[0].extra_annotations.contains_key(ANN_CMAKE_FETCH_WRAPPER));

        let dir = tempfile::tempdir().unwrap();
        let fs = files(
            dir.path(),
            &[(
                "CMakeLists.txt",
                &format!("function(lit tag)\nFetchContent_Declare(waybill-fixture-l GIT_REPOSITORY {gh} GIT_TAG ${{tag}})\nendfunction()\nlit(v1)\n"),
            )],
        );
        let (_, cov) = read_fetches(dir.path(), &fs);
        assert_eq!(cov.wrapper_definitions, 0, "an identity of its own (spec 1539 R2), as on main");
    }

    /// T021 / R3 P6: a duplicate ExternalProject_Add is a configure error,
    /// not a superseded declaration, so it never merges.
    #[test]
    fn external_project_never_merges_under_first_declaration_wins() {
        let dir = tempfile::tempdir().unwrap();
        let ep = |v: &str| format!("ExternalProject_Add(waybill-fixture-ep URL https://example.invalid/waybill-fixture-ep-{v}.tar.gz)\n");
        let fs = files(dir.path(), &[("CMakeLists.txt", &ep("1.0")), ("sub/CMakeLists.txt", &ep("2.0"))]);
        let (entries, cov) = read_fetches(dir.path(), &fs);
        assert_eq!(entries.len(), 2, "both stay components");
        assert_eq!(cov.merged_first_declaration_wins, 0);
    }

}
