//! Spec 1652 T016/T017: `contracts/settlement.md`, the rules.

use std::collections::BTreeSet;
use std::path::Path;

use super::super::fallback::FetchKind;
use super::{BuildDir, BuildEvidence};

/// Precedence is the declaration order reversed: `Taken` wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SettlementState {
    NoEvidence,
    Incomplete,
    NotTaken,
    Taken,
}

impl SettlementState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Taken => "taken",
            Self::NotTaken => "not-taken",
            Self::Incomplete => "incomplete",
            Self::NoEvidence => "no-evidence",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        [Self::Taken, Self::NotTaken, Self::Incomplete, Self::NoEvidence].into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceKind {
    CacheSourceDir,
    FetchContentSourceDirEntry,
    DepsSrcDir,
    SourceDirOverride,
    CachePackageDir,
    DownloadStamp,
    StampDirOnly,
}

impl EvidenceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CacheSourceDir => "cache-source-dir",
            Self::FetchContentSourceDirEntry => "fetchcontent-source-dir-entry",
            Self::DepsSrcDir => "deps-src-dir",
            Self::SourceDirOverride => "source-dir-override",
            Self::CachePackageDir => "cache-package-dir",
            Self::DownloadStamp => "download-stamp",
            Self::StampDirOnly => "stamp-dir-only",
        }
    }
}

/// The settlement key a declaration's name gives (research R1, T004):
/// FetchContent lower-cases, ExternalProject keeps the declared case.
pub(crate) fn key(kind: FetchKind, name: &str) -> String {
    match kind {
        FetchKind::FetchContent => name.to_lowercase(),
        FetchKind::ExternalProject => name.to_string(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Settlement {
    pub(crate) state: SettlementState,
    pub(crate) basis: BTreeSet<EvidenceKind>,
    /// Indices into `BuildEvidence::dirs` that gave evidence.
    pub(crate) dirs: BTreeSet<usize>,
    /// Owning build directories proposed different states.
    pub(crate) conflict: bool,
    /// FetchContent's R1 signals disagreed in one build directory.
    pub(crate) signal_disagreement: bool,
}

/// One build directory's proposal for a FetchContent declaration.
fn fetchcontent(lc: &str, d: &BuildDir) -> (SettlementState, BTreeSet<EvidenceKind>, bool) {
    let mut basis = BTreeSet::new();
    let source_dir = d.cache.contains(&format!("{lc}_SOURCE_DIR"));
    let fc_entry = d.cache.get(&format!("FETCHCONTENT_SOURCE_DIR_{}", lc.to_uppercase()));
    let src_dir = d.populations.contains(lc);
    if source_dir {
        basis.insert(EvidenceKind::CacheSourceDir);
    }
    if fc_entry.is_some() {
        basis.insert(EvidenceKind::FetchContentSourceDirEntry);
    }
    if src_dir {
        basis.insert(EvidenceKind::DepsSrcDir);
    }
    let overridden = fc_entry.is_some_and(|e| !e.value.trim().is_empty());
    if overridden {
        basis.insert(EvidenceKind::SourceDirOverride);
    }
    if source_dir || fc_entry.is_some() || src_dir {
        // R1: in the probe all three always agreed. An override has no
        // `_deps/<lc>-src`, by design, so it is not a disagreement.
        let disagreement = !overridden && !(source_dir && fc_entry.is_some() && src_dir);
        return (SettlementState::Taken, basis, disagreement);
    }
    // R2: `<name>_DIR` set by a find_package that succeeded, not by
    // FetchContent's own redirect. CMake keeps the case of the find_package
    // call, so both spellings are tried.
    let found = d.cache.entries_ci(&format!("{lc}_DIR")).any(|v| {
        let v = v.trim_end_matches(['/', '\\']);
        !v.is_empty() && !v.ends_with("NOTFOUND") && !v.replace('\\', "/").ends_with("CMakeFiles/pkgRedirects")
    });
    if found {
        basis.insert(EvidenceKind::CachePackageDir);
        return (SettlementState::NotTaken, basis, false);
    }
    (SettlementState::NoEvidence, basis, false)
}

/// One build directory's proposal for an ExternalProject declaration (R3).
fn externalproject(name: &str, d: &BuildDir) -> (SettlementState, BTreeSet<EvidenceKind>) {
    if d.downloads.contains(name) {
        return (SettlementState::Taken, BTreeSet::from([EvidenceKind::DownloadStamp]));
    }
    if d.stamp_dirs.contains(name) {
        return (SettlementState::Incomplete, BTreeSet::from([EvidenceKind::StampDirOnly]));
    }
    (SettlementState::NoEvidence, BTreeSet::new())
}

/// Settle one declaration made in `decl_file` (contracts/settlement.md).
pub(crate) fn settle(ev: &BuildEvidence, kind: FetchKind, name: &str, decl_file: &Path) -> Settlement {
    let k = key(kind, name);
    let mut out = Settlement {
        state: SettlementState::NoEvidence,
        basis: BTreeSet::new(),
        dirs: BTreeSet::new(),
        conflict: false,
        signal_disagreement: false,
    };
    let owners = ev.owners(decl_file);
    let mut proposals = BTreeSet::new();
    let mut by_dir: Vec<(usize, SettlementState)> = Vec::new();
    for i in owners {
        let d = &ev.dirs[i];
        let (state, basis) = match kind {
            FetchKind::FetchContent => {
                let (s, b, dis) = fetchcontent(&k, d);
                out.signal_disagreement |= dis;
                (s, b)
            }
            FetchKind::ExternalProject => externalproject(&k, d),
        };
        proposals.insert(state);
        by_dir.push((i, state));
        if state > out.state {
            out.state = state;
            out.basis = basis;
        } else if state == out.state {
            out.basis.extend(basis);
        }
    }
    out.conflict = proposals.len() > 1;
    // `builds` (C244): the directories whose evidence gave the final state.
    if out.state != SettlementState::NoEvidence {
        out.dirs = by_dir.into_iter().filter(|&(_, s)| s == out.state).map(|(i, _)| i).collect();
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::cache::CacheView;
    use super::*;
    use std::path::PathBuf;

    fn dir(cache: &str, pops: &[&str], downloads: &[&str], stamps: &[&str]) -> BuildDir {
        BuildDir {
            root: PathBuf::from("/b"),
            home: Some(PathBuf::from("/src")),
            cache: CacheView::parse(&format!("CMAKE_HOME_DIRECTORY:INTERNAL=/src\n{cache}")).unwrap(),
            populations: pops.iter().map(|s| s.to_string()).collect(),
            downloads: downloads.iter().map(|s| s.to_string()).collect(),
            stamp_dirs: stamps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn fetchcontent_taken_from_the_three_signals() {
        let d = dir("hdronly_SOURCE_DIR:STATIC=/b/_deps/hdronly-src\nFETCHCONTENT_SOURCE_DIR_HDRONLY:PATH=\n", &["hdronly"], &[], &[]);
        let (s, b, dis) = fetchcontent("hdronly", &d);
        assert_eq!(s, SettlementState::Taken);
        assert!(!dis);
        assert_eq!(b, BTreeSet::from([EvidenceKind::CacheSourceDir, EvidenceKind::FetchContentSourceDirEntry, EvidenceKind::DepsSrcDir]));
    }

    #[test]
    fn fetchcontent_one_signal_is_taken_and_a_disagreement() {
        let d = dir("", &["x"], &[], &[]);
        let (s, _, dis) = fetchcontent("x", &d);
        assert_eq!(s, SettlementState::Taken);
        assert!(dis);
    }

    #[test]
    fn fetchcontent_override_is_taken_without_a_population() {
        let d = dir("x_SOURCE_DIR:STATIC=/local/x\nFETCHCONTENT_SOURCE_DIR_X:PATH=/local/x\n", &[], &[], &[]);
        let (s, b, dis) = fetchcontent("x", &d);
        assert_eq!(s, SettlementState::Taken);
        assert!(b.contains(&EvidenceKind::SourceDirOverride));
        assert!(!dis);
    }

    #[test]
    fn fetchcontent_found_on_the_system_is_not_taken_but_a_redirect_is_no_evidence() {
        let found = dir("hdronly_DIR:PATH=/usr/lib/cmake/hdronly\n", &[], &[], &[]);
        assert_eq!(fetchcontent("hdronly", &found).0, SettlementState::NotTaken);
        let mixed = dir("HdrOnly_DIR:PATH=/usr/lib/cmake/hdronly\n", &[], &[], &[]);
        assert_eq!(fetchcontent("hdronly", &mixed).0, SettlementState::NotTaken, "find_package keeps its case");
        let redirect = dir("hdronly_DIR:INTERNAL=/b/CMakeFiles/pkgRedirects\n", &[], &[], &[]);
        assert_eq!(fetchcontent("hdronly", &redirect).0, SettlementState::NoEvidence);
        let notfound = dir("hdronly_DIR:PATH=hdronly_DIR-NOTFOUND\n", &[], &[], &[]);
        assert_eq!(fetchcontent("hdronly", &notfound).0, SettlementState::NoEvidence);
    }

    #[test]
    fn externalproject_download_stamp_or_stamp_dir() {
        let d = dir("", &[], &["extproj"], &["extproj", "extgit"]);
        assert_eq!(externalproject("extproj", &d).0, SettlementState::Taken);
        assert_eq!(externalproject("extgit", &d).0, SettlementState::Incomplete);
        assert_eq!(externalproject("other", &d).0, SettlementState::NoEvidence);
    }

    #[test]
    fn keys_fold_case_only_for_fetchcontent() {
        assert_eq!(key(FetchKind::FetchContent, "MixedCase"), "mixedcase");
        assert_eq!(key(FetchKind::ExternalProject, "ExtMixed"), "ExtMixed");
    }

    #[test]
    fn several_owners_take_the_strongest_and_flag_a_conflict() {
        let ev = BuildEvidence::for_tests(vec![
            dir("hdronly_DIR:PATH=/usr/lib/cmake/hdronly\n", &[], &[], &[]),
            dir("hdronly_SOURCE_DIR:STATIC=/b/_deps/hdronly-src\nFETCHCONTENT_SOURCE_DIR_HDRONLY:PATH=\n", &["hdronly"], &[], &[]),
        ]);
        let s = settle(&ev, FetchKind::FetchContent, "hdronly", Path::new("/src/CMakeLists.txt"));
        assert_eq!(s.state, SettlementState::Taken);
        assert!(s.conflict);
        assert_eq!(s.dirs, BTreeSet::from([1]), "only the directory that gave the final state");
    }
}
