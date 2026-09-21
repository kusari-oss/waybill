//! Milestone 924 (#932) — FR-021a/b: which directories earn their own record.
//!
//! Report size must be bounded by a repository's *significant* structure, not
//! by its directory count (FR-021). A 5,118-directory repository produces 393
//! records at the default threshold — 7.7% — which is a document a person can
//! read.
//!
//! Directories that earn no record do **not** vanish: their counts roll into
//! the nearest recorded ancestor (FR-021b), so FR-003's reconciliation holds
//! either way. Aggregation loses records, never counts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::census::{Census, DirCensus};

/// Unclaimed-file count above which an otherwise-unremarkable directory earns
/// its own record.
///
/// **Measured, not chosen** (research R3). Across 8 real repositories, the
/// size-triggered records beyond the marker-bearing baseline are 202 at N=10,
/// 59 at N=25, and 17 at N=50 for this repository. N=10 is noise; N=50 is too
/// coarse to surface a large unclassified blob.
///
/// Reported in every document (FR-021c) because two reports produced under
/// different thresholds are not comparable, and 8 repositories is enough to
/// pick a starting value and not enough to call it correct.
pub(crate) const DEFAULT_SIGNIFICANCE_THRESHOLD: u32 = 25;

/// Filenames that mark a project root. Presence makes a directory significant
/// regardless of size.
///
/// Deliberately broad: a marker waybill cannot read is *more* interesting than
/// one it can, because that is the gap the report exists to surface.
const MARKERS: &[&str] = &[
    "go.mod", "go.sum", "Cargo.toml", "package.json", "pom.xml",
    "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts",
    "pyproject.toml", "setup.py", "requirements.txt", "Pipfile", "uv.lock",
    "Gemfile", "Gemfile.lock", "composer.json", "mix.exs", "rebar.config",
    "pubspec.yaml", "Package.swift", "Podfile", "build.sbt", "stack.yaml",
    "CMakeLists.txt", "conanfile.txt", "conanfile.py", "vcpkg.json",
    "WORKSPACE", "WORKSPACE.bazel", "BUILD.bazel", "MODULE.bazel",
    "deno.json", "deno.jsonc", "pixi.toml", "Project.toml", "nimble.toml",
    "shard.yml", "dune-project", "build.zig", "pants.toml",
];

pub(crate) fn is_marker(name: &str) -> bool {
    MARKERS.contains(&name) || name.ends_with(".cabal") || name.ends_with(".csproj")
}

/// Why a directory earned its own record. Carried so the report can explain
/// itself rather than presenting an unexplained selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Significance {
    ScanRoot,
    HasMarker,
    Claimed,
    /// Carries an ambiguity record.
    ///
    /// Added at implement time after a fixture exposed the gap: a directory
    /// whose subtree spans several ecosystems typically has **no marker, no
    /// claim and no files of its own** — so every other rule aggregated it
    /// away, discarding the exact signal this feature exists to surface. An
    /// ambiguity that is never recorded is not an observation.
    Ambiguous,
    ExceedsThreshold,
    /// Not significant — roll into the nearest recorded ancestor.
    Aggregated,
}

pub(crate) fn classify(
    dir: &Path,
    root: &Path,
    d: &DirCensus,
    threshold: u32,
    ambiguous: bool,
) -> Significance {
    if dir == root {
        return Significance::ScanRoot;
    }
    if d.filenames.iter().any(|f| is_marker(f)) {
        return Significance::HasMarker;
    }
    if !d.claimed_by.is_empty() {
        return Significance::Claimed;
    }
    if ambiguous {
        return Significance::Ambiguous;
    }
    if d.files_unclaimed > u64::from(threshold) {
        return Significance::ExceedsThreshold;
    }
    Significance::Aggregated
}

/// A directory that will appear in the report, with the counts rolled up from
/// every aggregated descendant.
#[derive(Debug, Clone)]
pub(crate) struct RecordedDir {
    pub(crate) path: PathBuf,
    pub(crate) significance: Significance,
    pub(crate) files_direct: u64,
    pub(crate) files_aggregated: u64,
    pub(crate) census: DirCensus,
}

/// Partition the census into recorded directories, rolling aggregated ones up.
///
/// **Ordered input, ordered output.** `Census::dirs()` is a `BTreeMap`, so
/// ancestors sort before their descendants and the nearest recorded ancestor
/// is found by walking the path upward — no ordering assumption beyond what
/// the key type already guarantees (FR-020).
pub(crate) fn partition(
    census: &Census,
    root: &Path,
    threshold: u32,
    is_ambiguous: &dyn Fn(&Path) -> bool,
) -> Vec<RecordedDir> {
    let mut recorded: BTreeMap<PathBuf, RecordedDir> = BTreeMap::new();
    let mut deferred: Vec<(&PathBuf, &DirCensus)> = Vec::new();

    for (dir, d) in census.dirs() {
        match classify(dir, root, d, threshold, is_ambiguous(dir)) {
            Significance::Aggregated => deferred.push((dir, d)),
            sig => {
                recorded.insert(
                    dir.clone(),
                    RecordedDir {
                        path: dir.clone(),
                        significance: sig,
                        files_direct: d.files_direct,
                        files_aggregated: 0,
                        census: d.clone(),
                    },
                );
            }
        }
    }

    // FR-021b — every aggregated directory's files land on an ancestor. The
    // scan root is always recorded, so this terminates with a home for
    // everything; a file cannot be silently lost.
    for (dir, d) in deferred {
        let mut cur = dir.parent();
        while let Some(p) = cur {
            if let Some(anc) = recorded.get_mut(p) {
                anc.files_aggregated += d.files_direct;
                break;
            }
            cur = p.parent();
        }
    }

    recorded.into_values().collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::walk_registry::ReaderId;

    const R1: ReaderId = ReaderId::new("r1");

    fn census_with(root: &Path, spec: &[(&str, &[&str], bool)]) -> Census {
        let mut c = Census::new();
        c.touch_dir(root);
        for (dir, files, claimed) in spec {
            let p = root.join(dir);
            c.touch_dir(&p);
            for f in *files {
                c.record_file(&p, f, if *claimed { &[R1] } else { &[] });
            }
        }
        c
    }

    #[test]
    fn a_marker_makes_a_directory_significant_regardless_of_size() {
        let root = Path::new("/r");
        let c = census_with(root, &[("svc", &["go.mod"], false)]);
        let d = &c.dirs()[&root.join("svc")];
        assert_eq!(classify(&root.join("svc"), root, d, 25, false), Significance::HasMarker);
    }

    #[test]
    fn a_small_unmarked_unclaimed_directory_aggregates() {
        let root = Path::new("/r");
        let c = census_with(root, &[("notes", &["a.txt", "b.txt"], false)]);
        let d = &c.dirs()[&root.join("notes")];
        assert_eq!(classify(&root.join("notes"), root, d, 25, false), Significance::Aggregated);
    }

    /// FR-021b. The property that lets aggregation be safe: records are lost,
    /// counts never are.
    #[test]
    fn aggregation_preserves_counts_on_the_nearest_recorded_ancestor() {
        let root = Path::new("/r");
        let mut c = Census::new();
        c.touch_dir(root);
        c.record_file(root, "Cargo.toml", &[R1]);
        let deep = root.join("a").join("b").join("c");
        c.touch_dir(&deep);
        for i in 0..5 {
            c.record_file(&deep, &format!("f{i}.txt"), &[]);
        }
        let recorded = partition(&c, root, 25, &|_| false);
        let total: u64 = recorded.iter().map(|r| r.files_direct + r.files_aggregated).sum();
        assert_eq!(total, c.files_walked(), "aggregation must not lose files");
        assert_eq!(recorded.len(), 1, "only the scan root should be recorded");
        assert_eq!(recorded[0].files_aggregated, 5);
    }

    /// An ambiguous directory is significant even with no marker, no claim
    /// and no files of its own — which is the shape ambiguous containers
    /// actually have.
    #[test]
    fn an_ambiguous_directory_is_significant_even_when_otherwise_unremarkable() {
        let root = Path::new("/r");
        let c = census_with(root, &[("samples", &[], false)]);
        let d = &c.dirs()[&root.join("samples")];
        assert_eq!(classify(&root.join("samples"), root, d, 25, false), Significance::Aggregated);
        assert_eq!(classify(&root.join("samples"), root, d, 25, true), Significance::Ambiguous);
    }

    #[test]
    fn the_scan_root_is_always_recorded_even_when_unremarkable() {
        let root = Path::new("/r");
        let c = census_with(root, &[]);
        let d = &c.dirs()[root];
        assert_eq!(classify(root, root, d, 25, false), Significance::ScanRoot);
    }
}
