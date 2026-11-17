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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use waybill_common::resolution::LifecycleScope;

use super::super::PackageDbEntry;
use super::fallback::{self, Context, FallbackShape, FetchKind, TryMode};
use crate::scan_fs::package_db::installed_lib::cmake_version::{commands_checked, strip_comments};

/// C231 `waybill:cmake-fetch-coverage`.
pub(crate) const ANN_CMAKE_FETCH_COVERAGE: &str = "waybill:cmake-fetch-coverage";
/// C232 `waybill:cmake-fetch-conditions`.
pub(crate) const ANN_CMAKE_FETCH_CONDITIONS: &str = "waybill:cmake-fetch-conditions";
/// C233 `waybill:superseded-declarations`.
pub(crate) const ANN_SUPERSEDED: &str = "waybill:superseded-declarations";
const ANN_OPTIONAL_DERIVATION: &str = "waybill:optional-derivation";
const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";
const TEST_DIRS: [&str; 3] = ["test", "tests", "unittests"];

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
}

impl FetchCoverage {
    /// Sorted-key JSON, or `None` when no declaration was read (I-4).
    pub(crate) fn wire(&self) -> Option<String> {
        (self.read > 0).then(|| {
            serde_json::json!({
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
            })
            .to_string()
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
type ParsedFile = (PathBuf, PathBuf, String, Vec<(String, Vec<String>)>);

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

pub(crate) fn read_fetches(scan_root: &Path, files: &[PathBuf]) -> (Vec<PackageDbEntry>, FetchCoverage) {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut cov = FetchCoverage::default();
    // Pre-pass: every file's commands, the mode each directory sets, and
    // whether any file defines a FetchContent-capable provider (R3).
    let mut parsed: Vec<ParsedFile> = Vec::new();
    let mut modes: BTreeMap<PathBuf, TryMode> = BTreeMap::new();
    let mut provider = false;
    for path in files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "cmake: a CMake file could not be read (spec 1538 C231)");
                cov.unparseable_files += 1;
                continue;
            }
        };
        let (cmds, unterminated) = commands_checked(&strip_comments(&content));
        if unterminated {
            tracing::warn!(path = %path.display(), "cmake: a CMake file ends inside an open command (spec 1538 C231)");
            cov.unparseable_files += 1;
        }
        let cmds: Vec<(String, Vec<String>)> = cmds
            .into_iter()
            .map(|(n, a)| (n.to_ascii_lowercase(), a.into_iter().map(|x| x.text).collect()))
            .collect();
        let rel = relative(path, scan_root, &canonical_root);
        let dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        for (name, args) in &cmds {
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
    }

    let mut decls: Vec<Declaration> = Vec::new();
    for (path, rel, content, cmds) in &parsed {
        let vars = super::collect_set_vars(content);
        let source_path = path.to_string_lossy().into_owned();
        let dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        let in_test_dir = rel.parent().is_some_and(|d| {
            d.components().any(|c| matches!(c, Component::Normal(s) if TEST_DIRS.iter().any(|t| s == *t)))
        });
        let mut blocks = super::blocks::BlockStack::default();
        for (name, args) in cmds {
            if blocks.observe(name, args) {
                continue;
            }
            let (kind, rule) = match name.as_str() {
                "fetchcontent_declare" => (FetchKind::FetchContent, "FetchContent_Declare"),
                "externalproject_add" => (FetchKind::ExternalProject, "ExternalProject_Add"),
                _ => continue,
            };
            cov.read += 1;
            let raw_name = args.first().cloned().unwrap_or_default();
            let decl_name = super::expand_known_vars(&raw_name, &vars);
            let entry = if decl_name.is_empty() || decl_name.contains("${") || decl_name.contains("$<") {
                None
            } else {
                super::fetch_entry(&decl_name, &args[1..].join(" "), &source_path, rule, &vars)
            };
            let Some(mut entry) = entry else {
                cov.unresolved_identity += 1;
                continue;
            };
            let if_conds = blocks.if_conditions();
            let conditions = blocks.conditions();
            let ctx = Context { mode: mode_for(&dir, &modes), fetchcontent_provider: provider };
            let shape = fallback::classify(kind, args, &if_conds, blocks.function().is_some(), ctx);
            let test = if in_test_dir {
                Some("source-test-directory")
            } else if fallback::build_testing(&if_conds) {
                Some("cmake-build-testing")
            } else {
                None
            };
            mark(&mut entry, shape, test, &conditions);
            decls.push(Declaration { kind, name: decl_name.to_ascii_lowercase(), rel: rel.clone(), entry: Some(entry) });
        }
    }

    first_declaration_wins(&mut decls, &mut cov);
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
            "cmake: fetch declarations (spec 1538)"
        );
    }
    (entries, cov)
}

/// Test outranks Optional: a component's scope is one value (R7).
fn mark(entry: &mut PackageDbEntry, shape: Option<FallbackShape>, test: Option<&str>, conditions: &[String]) {
    if let Some(derivation) = test {
        entry.lifecycle_scope = Some(LifecycleScope::Test);
        entry.extra_annotations.insert(ANN_SCOPE_DERIVATION.to_string(), serde_json::json!(derivation));
    } else if let Some(shape) = shape {
        entry.lifecycle_scope = Some(LifecycleScope::Optional);
        entry.extra_annotations.insert(ANN_OPTIONAL_DERIVATION.to_string(), serde_json::json!(shape.as_str()));
    }
    if entry.lifecycle_scope.is_some() && !conditions.is_empty() {
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
        if d.kind != FetchKind::FetchContent {
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
        let rank = |e: &PackageDbEntry| match e.lifecycle_scope {
            None => 0,
            Some(LifecycleScope::Optional) => 1,
            _ => 2,
        };
        group.sort_by_key(|e| (rank(e), e.source_path.clone()));
        let mut first = group.remove(0);
        let mut conds: BTreeSet<String> = conditions_of(&first).into_iter().collect();
        let mut sup: Vec<serde_json::Value> =
            first.extra_annotations.get(ANN_SUPERSEDED).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        for other in group {
            conds.extend(conditions_of(&other));
            first.extra_source_paths.push(other.source_path.clone());
            first.extra_source_paths.extend(other.extra_source_paths.iter().cloned());
            if let Some(a) = other.extra_annotations.get(ANN_SUPERSEDED).and_then(|v| v.as_array()) {
                sup.extend(a.iter().cloned());
            }
        }
        first.extra_source_paths.sort();
        first.extra_source_paths.dedup();
        if first.lifecycle_scope.is_some() && !conds.is_empty() {
            first.extra_annotations.insert(ANN_CMAKE_FETCH_CONDITIONS.to_string(), serde_json::json!(conds));
        } else {
            first.extra_annotations.remove(ANN_CMAKE_FETCH_CONDITIONS);
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

/// R9: a **conditional** fetch of a repository another component already
/// stands for (a git submodule, another reader's component, an
/// unconditional fetch) is recorded on that component, not emitted.
pub(crate) fn merge_into_existing(entries: &mut Vec<PackageDbEntry>, cov: &mut FetchCoverage, scan_root: &Path) {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let is_fetch = |e: &PackageDbEntry| {
        e.extra_annotations
            .get("waybill:source-mechanism")
            .and_then(|v| v.as_str())
            .is_some_and(|m| m.starts_with("cmake-fetchcontent") || m == "cmake-externalproject")
    };
    let conditional = |e: &PackageDbEntry| {
        is_fetch(e)
            && e.lifecycle_scope == Some(LifecycleScope::Optional)
            && e.extra_annotations.get(ANN_OPTIONAL_DERIVATION).and_then(|v| v.as_str()).is_some_and(|d| d.starts_with("cmake-"))
    };
    let mut holders: BTreeMap<String, usize> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if conditional(e) {
            continue;
        }
        if let Some(id) = repository_identity(e) {
            holders.entry(id).or_insert(i);
        }
    }
    let mut remove: Vec<usize> = Vec::new();
    for i in 0..entries.len() {
        if !conditional(&entries[i]) {
            continue;
        }
        let Some(&h) = repository_identity(&entries[i]).as_ref().and_then(|id| holders.get(id)) else { continue };
        let lost = entries[i].clone();
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
    }
}
