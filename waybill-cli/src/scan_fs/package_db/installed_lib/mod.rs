//! Installed C/C++ library versions from pkg-config (`*.pc`) and CMake
//! package version files (`*ConfigVersion.cmake`, `*-config-version.cmake`).
//! Spec 1531, #1531.
//!
//! Two phases (research R2):
//! - **Walker pass and finalize** ([`registration`], [`finalize`]): collect
//!   matching paths, then parse each into a [`Record`]. Pure; needs no
//!   claims.
//! - **Post-pass** ([`resolve::resolve`]): called from `scan_fs::scan_path`
//!   after the binary scan, where the OS-package claims and the binary
//!   file components are both available. It drops OS-owned records,
//!   resolves `Requires`, builds entries and the coverage record (C225).

pub(crate) mod cmake_version;
pub(crate) mod location;
pub(crate) mod pkgconfig;
pub(crate) mod resolve;
pub(crate) mod targets;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::scan_fs::walk_registry::{globset_from_patterns, ReaderId, ReaderRegistration, SharedWalkerContext};

/// Files larger than this are `malformed` without being parsed. A real
/// `.pc` or version file is a few hundred bytes to a few KiB.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Which format a record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RecordKind {
    PkgConfig,
    CmakeVersion,
}

impl RecordKind {
    /// C55 `waybill:source-mechanism` value.
    pub(crate) fn source_mechanism(self) -> &'static str {
        match self {
            Self::PkgConfig => "pkg-config",
            Self::CmakeVersion => "cmake-package-version",
        }
    }
}

/// Why a matched file produced no component (data-model.md). Serialised
/// in exactly one place, [`SkipReason::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SkipReason {
    UnresolvedVersion,
    NoVersion,
    ComputedVersion,
    Malformed,
    OwnedByOsPackage,
    UnrecognisedLocation,
}

impl SkipReason {
    pub(crate) const ALL: [SkipReason; 6] = [
        SkipReason::ComputedVersion,
        SkipReason::Malformed,
        SkipReason::NoVersion,
        SkipReason::OwnedByOsPackage,
        SkipReason::UnrecognisedLocation,
        SkipReason::UnresolvedVersion,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::UnresolvedVersion => "unresolved-version",
            Self::NoVersion => "no-version",
            Self::ComputedVersion => "computed-version",
            Self::Malformed => "malformed",
            Self::OwnedByOsPackage => "owned-by-os-package",
            Self::UnrecognisedLocation => "unrecognised-location",
        }
    }
}

/// One matched file, parsed (data-model.md "Record").
#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) kind: RecordKind,
    /// The path as the walker saw it.
    pub(crate) path: PathBuf,
    /// The file's own location after root-relative symlink resolution
    /// (`location::resolve_in_root`). `None` when it escapes the root.
    pub(crate) resolved: Option<PathBuf>,
    /// Module or package name, declared case.
    pub(crate) name: String,
    /// The install prefix, absolute. `None` for an unrecognised location.
    pub(crate) prefix: Option<PathBuf>,
    pub(crate) version: Result<String, SkipReason>,
    pub(crate) requires: Vec<String>,
    pub(crate) requires_private: Vec<String>,
}

/// One exported-targets file (spec 1532, data-model.md `TargetsFile`).
#[derive(Debug, Clone)]
pub(crate) struct TargetsFile {
    pub(crate) path: PathBuf,
    /// The directory holding it: one export set per directory.
    pub(crate) dir: PathBuf,
    pub(crate) role: targets::Role,
    /// The install prefix (CMake search-location rules), or `None` for an
    /// unrecognised location.
    pub(crate) prefix: Option<PathBuf>,
    /// The file's own location after root-relative resolution.
    pub(crate) resolved: Option<PathBuf>,
    /// `None` when the file could not be read (`malformed`).
    pub(crate) parsed: Option<targets::Parsed>,
}

/// Phase 1's output: #1531 records and spec 1532 exported-targets files.
#[derive(Debug, Default)]
pub(crate) struct Phase1 {
    pub(crate) records: Vec<Record>,
    pub(crate) targets: Vec<TargetsFile>,
}

/// Whether the reader wants a file of this name.
fn wanted(file_name: &str) -> bool {
    classify_name(file_name).is_some() || targets::classify(file_name).is_some()
}

/// Walker state: every matched path.
#[derive(Default, Debug)]
pub(crate) struct InstalledLibMarker {
    pub(crate) paths: BTreeSet<PathBuf>,
}

/// The kind a file name selects, and the name it declares.
pub(crate) fn classify_name(file_name: &str) -> Option<(RecordKind, &str)> {
    if let Some(stem) = file_name.strip_suffix(".pc") {
        return (!stem.is_empty()).then_some((RecordKind::PkgConfig, stem));
    }
    for suffix in ["ConfigVersion.cmake", "-config-version.cmake"] {
        if let Some(stem) = file_name.strip_suffix(suffix) {
            return (!stem.is_empty()).then_some((RecordKind::CmakeVersion, stem));
        }
    }
    None
}

fn on_file(path: &Path, ctx: &SharedWalkerContext<'_>) {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    if !wanted(name) {
        return;
    }
    let Some(state) = ctx.state::<Mutex<InstalledLibMarker>>(ReaderId::INSTALLED_LIB) else {
        return;
    };
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.paths.insert(path.to_path_buf());
}

/// Research R1: three basename patterns. Python virtual environments, which
/// the shared walker skips, are reached by [`venv_candidates`] instead of a
/// `descend_into` override: walking a whole environment measured +44% scan
/// time on 50,000 files (task T040, `measurements/venv-walk-timing.md`).
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&[
        "**/*.pc",
        "**/*ConfigVersion.cmake",
        "**/*-config-version.cmake",
        // Spec 1532: CMake exported-targets files.
        "**/*Targets.cmake",
        "**/*Targets-*.cmake",
        "**/*-targets.cmake",
        "**/*-targets-*.cmake",
        "**/*Exports.cmake",
        "**/*Exports-*.cmake",
    ])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::INSTALLED_LIB,
        state: Some(Arc::new(Mutex::new(InstalledLibMarker::default()))),
        patterns,
        on_file: Some(on_file),
        on_dir: None,
        descend_into: None,
    })
}

/// Library-directory names probed below a site-packages directory.
const PROBE_LIB_DIRS: [&str; 5] = ["lib", "lib64", "lib32", "libx32", "share"];
/// How many directory levels below site-packages a wheel's `lib*` may sit:
/// `<pkg>/lib`, `<ns>/<pkg>/lib`, `<ns>/<pkg>/<sub>/lib`.
const PROBE_DEPTH: usize = 3;

/// Spec FR-001 (analysis C1): matching files inside the Python virtual
/// environments the pip reader reads (`.venv`, `venv`), which the shared
/// walker skips. A bounded probe, not a walk: below each site-packages
/// directory it lists directories up to [`PROBE_DEPTH`] levels deep, and
/// reads only `<lib*>/pkgconfig/` and `<lib*>/cmake/` (two levels) where it
/// finds one. Real directories only, no symlinks followed; directories the
/// operator excluded are skipped.
pub(crate) fn venv_candidates(
    scan_root: &Path,
    exclude_set: &crate::scan_fs::package_db::exclude_path::ExclusionSet,
) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let is_venv = |p: &Path| p.components().any(|c| c.as_os_str() == ".venv" || c.as_os_str() == "venv");
    for sp in crate::scan_fs::package_db::pip::candidate_site_packages_roots(scan_root) {
        let Some(rel) = location::relative_to_root(&sp, scan_root) else { continue };
        if !is_venv(&rel) || !sp.is_dir() {
            continue;
        }
        let mut frontier = vec![sp];
        for _ in 0..PROBE_DEPTH {
            let mut next = Vec::new();
            for dir in &frontier {
                for child in real_subdirs(dir) {
                    let excluded = location::relative_to_root(&child, scan_root)
                        .and_then(|r| r.to_str().map(|s| exclude_set.matches(s)))
                        .unwrap_or(false);
                    if excluded {
                        continue;
                    }
                    let name = child.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if PROBE_LIB_DIRS.contains(&name) {
                        collect_matching(&child.join("pkgconfig"), 0, &mut out);
                        collect_matching(&child.join("cmake"), 2, &mut out);
                    }
                    next.push(child);
                }
            }
            frontier = next;
        }
    }
    out
}

/// Real (non-symlink) subdirectories of `dir`, sorted.
fn real_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> =
        rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
    v.sort();
    v
}

/// Matching files in `dir` and, `depth` more levels down, its real
/// subdirectories.
fn collect_matching(dir: &Path, depth: usize, out: &mut BTreeSet<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let p = e.path();
        if t.is_dir() {
            if depth > 0 {
                collect_matching(&p, depth - 1, out);
            }
        } else if p.file_name().and_then(|s| s.to_str()).is_some_and(wanted) {
            out.insert(p);
        }
    }
}

pub(crate) fn extract_marker(registration: &ReaderRegistration) -> InstalledLibMarker {
    let Some(state_arc) = registration.state.as_ref() else {
        return InstalledLibMarker::default();
    };
    let Some(mutex) = state_arc.downcast_ref::<Mutex<InstalledLibMarker>>() else {
        return InstalledLibMarker::default();
    };
    let mut guard = match mutex.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::mem::take(&mut *guard)
}

/// Parse every matched file, plus those in virtual environments
/// ([`venv_candidates`]), in path order: #1531 records and spec 1532
/// exported-targets files.
pub(crate) fn finalize(
    mut marker: InstalledLibMarker,
    scan_root: &Path,
    exclude_set: &crate::scan_fs::package_db::exclude_path::ExclusionSet,
) -> Phase1 {
    marker.paths.extend(venv_candidates(scan_root, exclude_set));
    let mut out = Phase1::default();
    for p in marker.paths {
        if let Some(r) = read_record(&p, scan_root) {
            out.records.push(r);
        } else if let Some(t) = read_targets(&p, scan_root) {
            out.targets.push(t);
        }
    }
    out
}

fn read_targets(path: &Path, scan_root: &Path) -> Option<TargetsFile> {
    let (role, base) = targets::classify_with_base(path.file_name()?.to_str()?)?;
    let rel = location::relative_to_root(path, scan_root)?;
    let dir = path.parent()?.to_path_buf();
    // The package the file is named for, not its directory's name: in the
    // `share/<pkg>/cmake/` layout the directory is `cmake` (measured on the
    // ROCm SDK: `share/elfio/cmake/elfioTargets.cmake`).
    let prefix = location::prefix_of(&rel, RecordKind::CmakeVersion, base)
        .map(|p| if p.as_os_str().is_empty() { scan_root.to_path_buf() } else { scan_root.join(p) });
    let resolved = location::resolve_in_root(scan_root, &rel);
    let parsed = resolved.as_deref().and_then(read_bounded).map(|c| targets::parse(&c));
    Some(TargetsFile { path: path.to_path_buf(), dir, role, prefix, resolved, parsed })
}

fn read_record(path: &Path, scan_root: &Path) -> Option<Record> {
    let file_name = path.file_name()?.to_str()?;
    let (kind, name) = classify_name(file_name)?;
    let name = name.to_string();
    let rel = location::relative_to_root(path, scan_root)?;
    let prefix = location::prefix_of(&rel, kind, &name)
        .map(|p| if p.as_os_str().is_empty() { scan_root.to_path_buf() } else { scan_root.join(p) });
    let resolved = location::resolve_in_root(scan_root, &rel);
    let mut record = Record {
        kind,
        path: path.to_path_buf(),
        resolved: resolved.clone(),
        name,
        prefix,
        version: Err(SkipReason::Malformed),
        requires: Vec::new(),
        requires_private: Vec::new(),
    };
    if record.prefix.is_none() {
        record.version = Err(SkipReason::UnrecognisedLocation);
        return Some(record);
    }
    let Some(content) = resolved.as_deref().and_then(read_bounded) else {
        return Some(record);
    };
    match kind {
        RecordKind::PkgConfig => {
            let pc = pkgconfig::parse(&content);
            record.version = pc.version;
            record.requires = pc.requires;
            record.requires_private = pc.requires_private;
        }
        RecordKind::CmakeVersion => record.version = cmake_version::parse(&content),
    }
    Some(record)
}

/// The file's text, or `None` (→ `malformed`) when unreadable, over
/// [`MAX_FILE_BYTES`], or not UTF-8.
fn read_bounded(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn classify_name_selects_kind_and_stem() {
        assert_eq!(classify_name("zlib.pc"), Some((RecordKind::PkgConfig, "zlib")));
        assert_eq!(classify_name("FooConfigVersion.cmake"), Some((RecordKind::CmakeVersion, "Foo")));
        assert_eq!(classify_name("foo-config-version.cmake"), Some((RecordKind::CmakeVersion, "foo")));
        assert_eq!(classify_name("x.pc.in"), None);
        assert_eq!(classify_name("FooConfigVersion.cmake.in"), None);
        assert_eq!(classify_name(".pc"), None);
        assert_eq!(classify_name("ConfigVersion.cmake"), None);
        assert_eq!(classify_name("FooConfig.cmake"), None);
        // Spec 1532: exported-targets files are not #1531 records, but the
        // reader wants them.
        assert_eq!(classify_name("FooTargets.cmake"), None);
        for n in ["FooTargets.cmake", "foo-targets-release.cmake", "LLVMExports.cmake", "zlib.pc", "FooConfigVersion.cmake"] {
            assert!(wanted(n), "{n}");
        }
        for n in ["FooTargets.cmake.in", "FooConfig.cmake", "targets.cmake"] {
            assert!(!wanted(n), "{n}");
        }
    }
}
