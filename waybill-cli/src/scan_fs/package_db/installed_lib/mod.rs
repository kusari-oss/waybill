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
    if classify_name(name).is_none() {
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

/// Research R1: three basename patterns, and descent into the Python
/// virtual environments the pip reader reads (spec FR-001, analysis C1).
pub(crate) fn registration() -> anyhow::Result<ReaderRegistration> {
    let patterns = globset_from_patterns(&["**/*.pc", "**/*ConfigVersion.cmake", "**/*-config-version.cmake"])?;
    let descend_into = globset_from_patterns(&[".venv", "venv"])?;
    Ok(ReaderRegistration {
        reader_id: ReaderId::INSTALLED_LIB,
        state: Some(Arc::new(Mutex::new(InstalledLibMarker::default()))),
        patterns,
        on_file: Some(on_file),
        on_dir: None,
        descend_into: Some(descend_into),
    })
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

/// Parse every matched file into a record, in path order.
pub(crate) fn finalize(marker: InstalledLibMarker, scan_root: &Path) -> Vec<Record> {
    marker.paths.into_iter().filter_map(|p| read_record(&p, scan_root)).collect()
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
    }
}
