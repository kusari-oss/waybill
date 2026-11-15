//! Git submodules as components (spec 1530, issue #1515; milestone 105
//! User Story 6, finished).
//!
//! One component per *gitlink* — a tree entry of mode `160000` — pinned
//! to the commit the super-project records, whether or not the submodule
//! is initialised. `.gitmodules` supplies only the URL.
//!
//! The scanned repository is untrusted. Research R1 measured `git
//! ls-files` and `git status` running a program the repository names in
//! its own `core.fsmonitor`; `git ls-tree` does not. This reader runs
//! only commands measured not to load the index, and disables
//! `core.fsmonitor` and hooks on every invocation (`gitlinks.rs`).

pub(crate) mod gitlinks;
pub(crate) mod gitmodules;
pub(crate) mod identity;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::scan_fs::walk_registry::{
    globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext,
};

use super::PackageDbEntry;
use gitlinks::{GitError, Gitlink};
use gitmodules::SubmoduleDeclaration;

/// C55 value for every component this reader emits.
pub(crate) const SOURCE_MECHANISM: &str = "git-submodule";
/// C222 (spec 1530 FR-011).
pub(crate) const ANN_GIT_SUBMODULE_COVERAGE: &str = "waybill:git-submodule-coverage";

/// Why a submodule entry produced no component, or (for `NoUrl`) produced
/// one without a URL. Closed; serialised only by [`SkipReason::as_wire`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SkipReason {
    GitRefused,
    GitUnavailable,
    NoGitlink,
    NoRepositoryData,
    NoUrl,
    UnsafePathOrUrl,
}

impl SkipReason {
    pub(crate) const ALL: [SkipReason; 6] = [
        Self::GitRefused,
        Self::GitUnavailable,
        Self::NoGitlink,
        Self::NoRepositoryData,
        Self::NoUrl,
        Self::UnsafePathOrUrl,
    ];

    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::GitRefused => "git-refused",
            Self::GitUnavailable => "git-unavailable",
            Self::NoGitlink => "no-gitlink",
            Self::NoRepositoryData => "no-repository-data",
            Self::NoUrl => "no-url",
            Self::UnsafePathOrUrl => "unsafe-path-or-url",
        }
    }
}

/// C222: what the reader pinned and what it skipped, by reason
/// (data-model.md `SubmoduleCoverage`). Present only when a `.gitmodules`
/// file was seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubmoduleCoverage {
    pinned: usize,
    skipped: BTreeMap<SkipReason, usize>,
}

impl SubmoduleCoverage {
    fn skip(&mut self, reason: SkipReason, n: usize) {
        *self.skipped.entry(reason).or_default() += n;
    }

    pub(crate) fn skipped(&self, reason: SkipReason) -> usize {
        self.skipped.get(&reason).copied().unwrap_or(0)
    }

    /// The wire value: a JSON object with sorted keys, every reason present
    /// (contracts/wire.md, C222).
    pub fn wire(&self) -> String {
        let mut skipped = serde_json::Map::new();
        for r in SkipReason::ALL {
            skipped.insert(r.as_wire().to_string(), serde_json::json!(self.skipped(r)));
        }
        let mut obj = serde_json::Map::new();
        obj.insert("pinned".into(), serde_json::json!(self.pinned));
        obj.insert("skipped".into(), serde_json::Value::Object(skipped));
        serde_json::Value::Object(obj).to_string()
    }
}

/// Walker marker: every `.gitmodules` the shared walker dispatched.
#[derive(Default, Debug)]
pub(crate) struct GitSubmoduleMarker {
    pub(crate) gitmodules: BTreeSet<PathBuf>,
}

fn on_gitmodules(path: &Path, ctx: &SharedWalkerContext<'_>) {
    if path.file_name().and_then(|s| s.to_str()) != Some(".gitmodules") {
        return;
    }
    let Some(state) = ctx.state::<Mutex<GitSubmoduleMarker>>(ReaderId::GIT_SUBMODULE) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.gitmodules.insert(path.to_path_buf());
}

pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/.gitmodules"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::GIT_SUBMODULE,
        state: Some(Arc::new(Mutex::new(GitSubmoduleMarker::default()))),
        patterns,
        on_file: Some(on_gitmodules),
        on_dir: None,
        descend_into: None,
    })
}

pub(crate) fn extract_marker(registration: &ReaderRegistration) -> GitSubmoduleMarker {
    let Some(state_arc) = registration.state.as_ref() else {
        return GitSubmoduleMarker::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<GitSubmoduleMarker>>() else {
        return GitSubmoduleMarker::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Lexically normalise a path: drop `.` components and resolve `..`.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Declarations gathered for one repository, keyed by path relative to its
/// top level. The first declaration of a path wins; files are visited
/// shallowest first (FR-001).
#[derive(Default)]
struct RepoDecls {
    by_path: BTreeMap<PathBuf, SubmoduleDeclaration>,
    first_gitmodules: Option<PathBuf>,
}

/// Read every dispatched `.gitmodules`, join with gitlinks, and emit one
/// entry per gitlink (FR-001 to FR-008, FR-011). Returns the entries and
/// the C222 coverage, which is `None` when no `.gitmodules` was seen.
pub(crate) fn finalize(
    marker: GitSubmoduleMarker,
    scan_root: &Path,
) -> (Vec<PackageDbEntry>, Option<SubmoduleCoverage>) {
    if marker.gitmodules.is_empty() {
        return (Vec::new(), None);
    }
    let scan_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut coverage = SubmoduleCoverage::default();
    let mut warned_unavailable = false;

    // Shallowest first, then lexical: a top-level declaration of a path
    // precedes a nested file's declaration of the same gitlink.
    let mut files: Vec<PathBuf> = marker
        .gitmodules
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    files.sort_by_key(|p| (p.components().count(), p.clone()));
    files.dedup();

    let mut repos: BTreeMap<PathBuf, RepoDecls> = BTreeMap::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            tracing::warn!(reader = "git-submodule", file = %file.display(), "could not read .gitmodules");
            continue;
        };
        let parsed = gitmodules::parse(&text, file);
        coverage.skip(SkipReason::UnsafePathOrUrl, parsed.unsafe_entries);
        let n = parsed.declarations.len();
        let Some(dir) = file.parent() else { continue };
        match gitlinks::toplevel(dir) {
            Ok(Some(top)) if top.starts_with(&scan_root) => {
                // FR-001: a `.gitmodules` below its repository's top level
                // (a vendored or monorepo project) names gitlinks of the
                // enclosing repository, under its own directory.
                let prefix = dir.strip_prefix(&top).map(Path::to_path_buf).unwrap_or_default();
                let repo = repos.entry(top).or_default();
                repo.first_gitmodules.get_or_insert_with(|| file.clone());
                for mut d in parsed.declarations {
                    d.path = normalize(&prefix.join(&d.path));
                    if let Some(prev) = repo.by_path.get(&d.path) {
                        if prev.url != d.url {
                            tracing::warn!(
                                reader = "git-submodule",
                                path = %d.path.display(),
                                first = %prev.declared_in.display(),
                                second = %d.declared_in.display(),
                                "two .gitmodules files give different URLs for one path; the first is used",
                            );
                        }
                        continue;
                    }
                    repo.by_path.insert(d.path.clone(), d);
                }
            }
            // No enclosing repository at or below the scan root (research
            // R14: never read a repository above it).
            Ok(_) => {
                tracing::warn!(reader = "git-submodule", file = %file.display(), entries = n, "no repository data for this .gitmodules; its submodules have no pinned commit");
                coverage.skip(SkipReason::NoRepositoryData, n);
            }
            Err(GitError::Refused) => {
                tracing::warn!(
                    reader = "git-submodule",
                    file = %file.display(),
                    "git refused the repository (\"dubious ownership\"); add it with `git config --global --add safe.directory <path>` to read its submodules",
                );
                coverage.skip(SkipReason::GitRefused, n);
            }
            Err(GitError::Unavailable(why)) => {
                if !std::mem::replace(&mut warned_unavailable, true) {
                    tracing::warn!(reader = "git-submodule", reason = %why, "git is unavailable; no submodule is pinned");
                }
                coverage.skip(SkipReason::GitUnavailable, n);
            }
            Err(GitError::Failed(msg)) => {
                tracing::warn!(reader = "git-submodule", file = %file.display(), error = %msg, "git could not read the repository");
                coverage.skip(SkipReason::GitUnavailable, n);
            }
        }
    }

    // C57 inputs, once per scan.
    let find_package_targets = super::cmake::collect_find_package_targets(&scan_root);
    let path_refs = super::cmake::collect_cmake_path_references(&scan_root);

    // Each emitted submodule's directory -> its PURL, for nested edges.
    let mut emitted: Vec<PackageDbEntry> = Vec::new();
    let mut purl_of_dir: BTreeMap<PathBuf, String> = BTreeMap::new();

    for (top, decls) in &repos {
        let links = match gitlinks::read_gitlinks(top) {
            Ok(l) => l,
            Err(e) => {
                let reason = match e {
                    GitError::Refused => SkipReason::GitRefused,
                    _ => SkipReason::GitUnavailable,
                };
                coverage.skip(reason, decls.by_path.len());
                continue;
            }
        };
        let origin = gitlinks::origin_url(top);
        let linked: BTreeSet<&Path> = links.iter().map(|l| l.path.as_path()).collect();
        for (path, d) in &decls.by_path {
            if !linked.contains(path.as_path()) {
                tracing::warn!(reader = "git-submodule", file = %d.declared_in.display(), path = %path.display(), "declared submodule has no gitlink; skipped");
                coverage.skip(SkipReason::NoGitlink, 1);
            }
        }
        for Gitlink { path, commit } in &links {
            let abs = normalize(&top.join(path));
            if !abs.starts_with(&scan_root) {
                continue;
            }
            let decl = decls.by_path.get(path);
            let Some(id) = identity::identity(decl.and_then(|d| d.url.as_deref()), origin.as_deref(), path, commit) else {
                tracing::warn!(reader = "git-submodule", path = %path.display(), "could not form a PURL for this gitlink; skipped");
                continue;
            };
            if decl.is_none() {
                tracing::warn!(reader = "git-submodule", path = %path.display(), "gitlink has no .gitmodules entry; emitted without a URL");
                coverage.skip(SkipReason::NoUrl, 1);
            }
            if id.credentials_stripped {
                tracing::warn!(
                    reader = "git-submodule",
                    file = %decl.map(|d| d.declared_in.display().to_string()).unwrap_or_default(),
                    url = %id.url.as_deref().unwrap_or(""),
                    "credentials stripped from a submodule URL",
                );
            }
            // Spec edge case: an initialised checkout at another commit.
            if let Ok(Some(sub_top)) = gitlinks::toplevel(&abs) {
                if sub_top == abs {
                    if let Some(head) = gitlinks::head_commit(&abs) {
                        if &head != commit {
                            tracing::warn!(
                                reader = "git-submodule",
                                path = %path.display(),
                                pinned = %commit,
                                checked_out = %head,
                                "submodule checkout differs from the commit the super-project pins; the pinned commit is used",
                            );
                        }
                    }
                }
            }
            let build_reference = super::cmake::build_reference(&abs, &find_package_targets, &path_refs);
            let source_path = decl
                .map(|d| d.declared_in.clone())
                .or_else(|| decls.first_gitmodules.clone())
                .unwrap_or_else(|| top.join(".gitmodules"));
            coverage.pinned += 1;
            purl_of_dir.insert(abs.clone(), id.purl.as_str().to_string());
            emitted.push(entry(&id, commit, &source_path, build_reference));
        }
    }

    // FR-010: a nested submodule is a dependency of the submodule whose
    // directory holds its repository.
    let parents: BTreeMap<PathBuf, String> = purl_of_dir.clone();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (dir, purl) in &purl_of_dir {
        if let Some(parent) = dir.ancestors().skip(1).find_map(|a| parents.get(a)) {
            children.entry(parent.clone()).or_default().push(purl.clone());
        }
    }
    for e in &mut emitted {
        if let Some(kids) = children.remove(e.purl.as_str()) {
            e.depends.extend(kids);
        }
    }

    tracing::info!(reader = "git-submodule", coverage = %coverage.wire(), "git submodules");
    (emitted, Some(coverage))
}

fn entry(id: &identity::Identity, commit: &str, source_path: &Path, build_reference: &'static str) -> PackageDbEntry {
    let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    extra_annotations.insert("waybill:source-mechanism".to_string(), serde_json::json!(SOURCE_MECHANISM));
    extra_annotations.insert("waybill:build-reference".to_string(), serde_json::json!(build_reference));
    PackageDbEntry {
        extra_source_paths: Vec::new(),
        depends_ecosystem: None,
        build_inclusion: None,
        purl: id.purl.clone(),
        name: id.name.clone(),
        version: commit.to_string(),
        arch: None,
        source_path: source_path.to_string_lossy().into_owned(),
        depends: Vec::new(),
        maintainer: None,
        licenses: Vec::new(),
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: None,
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        raw_version: None,
        parent_purl: None,
        npm_role: None,
        co_owned_by: None,
        hashes: Vec::new(),
        sbom_tier: Some("source".to_string()),
        shade_relocation: None,
        extra_annotations,
        binary_role: None,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn s1530_coverage_wire_lists_every_reason_sorted() {
        let mut c = SubmoduleCoverage { pinned: 2, ..Default::default() };
        c.skip(SkipReason::NoGitlink, 1);
        assert_eq!(
            c.wire(),
            r#"{"pinned":2,"skipped":{"git-refused":0,"git-unavailable":0,"no-gitlink":1,"no-repository-data":0,"no-url":0,"unsafe-path-or-url":0}}"#
        );
    }

    #[test]
    fn s1530_no_gitmodules_means_no_coverage_and_no_git() {
        let tmp = tempfile::tempdir().unwrap();
        let (entries, coverage) = finalize(GitSubmoduleMarker::default(), tmp.path());
        assert!(entries.is_empty());
        assert!(coverage.is_none());
    }
}
