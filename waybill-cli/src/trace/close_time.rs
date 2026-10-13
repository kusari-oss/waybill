//! Close-time facts for compiler read and write sets (milestone 1072, #1144).
//!
//! After the traced command exits, this pass walks every path named by a
//! compiler invocation's read or write set and records what the filesystem
//! says about it *now*: whether the name still resolves, and the content
//! hash if it does. The kernel supplies neither — it reports that a path was
//! opened, not what the bytes were (research R1) — so before this pass the
//! predicate carried a 64-zero sentinel hash and a hardcoded
//! `survived_trace_window: true`.
//!
//! The facts belong to a distinct tier from what the trace observed, and are
//! named with an `_at_close` suffix so a consumer can tell them apart from
//! the field name alone. `sha256_at_close` is **not** the content the
//! compiler read; see `contracts/read-write-set-shape.md` C-1072-1.
//!
//! # Deliberately not cfg-gated
//!
//! This module contains no eBPF and compiles on every host (research R6).
//! Only its call site in `cli/scan.rs` sits behind
//! `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`, because
//! that is where a trace exists to apply the pass to.
//!
//! Gating the module would put its whole body outside what macOS
//! `cargo check` and the default CI lane can see — the standing eBPF
//! container test gap — and 15 of this milestone's 49 tasks are unit tests
//! of this file. Ungated, they run under plain `cargo test`.
//!
//! The cost of that choice is this attribute: with the call site compiled
//! out, nothing in the crate calls into here, so every item reads as dead.
//! Same shape as `trace/cwd.rs` and `trace/observed.rs`, which are ungated
//! for the same reason.
#![cfg_attr(
    not(all(target_os = "linux", feature = "ebpf-tracing")),
    allow(dead_code)
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tracing::debug;
use waybill_common::attestation::compiler_pipeline::{CompilerPipelineData, ReadKind};
use waybill_common::attestation::integrity::{BudgetSource, CompilerHashCoverage};
use waybill_common::types::hash::ContentHash;

/// How many bytes the pass may hash in total, and where that number came
/// from.
///
/// Deliberately derives no `Default`: there is no defensible default byte
/// figure until it has been measured on a real kernel trace
/// (`contracts/cli-budget.md` C-1072-9), so every caller must say what it
/// is passing. The parameter exists from the pass's first commit even
/// though T037 is what charges against it — threading it in later would
/// mean changing every call site twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub total_bytes: u64,
    pub source: BudgetSource,
}

/// What the pass determined about one distinct path, before fan-out.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PathFacts {
    /// `symlink_metadata` succeeded: the name resolved.
    existed: bool,
    /// Content hash, when the path was hashed. `None` means "not
    /// measured", never "zero".
    hash: Option<ContentHash>,
}

/// Which of the three coverage counts a path's visit landed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Hashed,
    Absent,
    Skipped,
}

/// Apply close-time facts to every entry in `data`, in place, and return
/// what was covered.
///
/// Never adds, removes or reorders an entry, and never touches `path`,
/// `kind` or `renamed_from`: the observation tier is read-only here
/// (FR-010, contract C-1072-7, Principle II). A path no eBPF event
/// witnessed does not reach a set, however visible it is on disk.
///
/// Each distinct path is visited once, in a fixed order, and the result is
/// fanned out to every entry naming it across every invocation (FR-011).
/// Two entries for one path therefore cannot disagree.
pub fn apply_close_time_facts(
    data: &mut CompilerPipelineData,
    budget: Budget,
) -> CompilerHashCoverage {
    let ordered = distinct_paths(data);

    let mut facts: BTreeMap<PathBuf, PathFacts> = BTreeMap::new();
    let mut hashed = 0u64;
    let mut absent = 0u64;
    let mut skipped = 0u64;

    for path in &ordered {
        let (f, outcome) = visit(path);
        match outcome {
            Outcome::Hashed => hashed += 1,
            Outcome::Absent => absent += 1,
            Outcome::Skipped => skipped += 1,
        }
        facts.insert(path.clone(), f);
    }

    fan_out(data, &facts);

    CompilerHashCoverage {
        paths_total: ordered.len() as u64,
        paths_hashed: hashed,
        paths_absent: absent,
        paths_skipped: skipped,
        truncated: false,
        budget_source: budget.source,
    }
}

/// The distinct paths to visit, in the order they are visited.
///
/// Write-set paths first, lexicographically sorted, then read-set paths
/// the write sets did not already name, also sorted. A path in both is
/// visited once, in its write-set position. The order is a function of the
/// build's content alone — never of `HashMap` iteration, thread scheduling
/// or wall-clock — which is what makes the budget's truncation point
/// reproducible (SC-005, feature 006 US2).
///
/// `StdinInput` entries contribute no path: `"<stdin>"` is a marker, not a
/// location, and stat'ing it would resolve against the working directory,
/// so a build that happened to contain a file of that name would be
/// reported as the stdin entry having survived (contract C-1072-2a).
fn distinct_paths(data: &CompilerPipelineData) -> Vec<PathBuf> {
    let writes: BTreeSet<&PathBuf> = data
        .invocations
        .iter()
        .flat_map(|inv| inv.write_set.iter().map(|e| &e.path))
        .collect();

    let reads: BTreeSet<&PathBuf> = data
        .invocations
        .iter()
        .flat_map(|inv| inv.read_set.iter())
        .filter(|e| matches!(e.kind, ReadKind::File))
        .map(|e| &e.path)
        .collect();

    writes
        .iter()
        .copied()
        .chain(reads.iter().copied().filter(|p| !writes.contains(*p)))
        .cloned()
        .collect()
}

/// Stat one path and classify it (data-model E6 step 2).
///
/// `symlink_metadata`, not `metadata`: the question is whether the name
/// resolved, so a dangling symlink is a successful stat of the link itself
/// — present, not a regular file, no hash.
fn visit(path: &PathBuf) -> (PathFacts, Outcome) {
    match std::fs::symlink_metadata(path) {
        Err(_) => (
            PathFacts {
                existed: false,
                hash: None,
            },
            Outcome::Absent,
        ),
        Ok(md) => {
            // Hashing lands in T020. Until then a present regular file is
            // "existed but not hashed", which is exactly true, and keeps
            // FR-013's partition satisfied at this commit rather than only
            // after US1.
            if !md.is_file() {
                debug!(path = %path.display(), "close-time: not a regular file, skipped");
            }
            (
                PathFacts {
                    existed: true,
                    hash: None,
                },
                Outcome::Skipped,
            )
        }
    }
}

/// Write each path's facts onto every entry naming it, in both sets,
/// across every invocation (data-model E6 step 3, FR-011).
///
/// An entry whose path has no facts is a `StdinInput` entry, which was
/// excluded from the walk: it reports `existed_at_close: false` with no
/// hash and contributes to none of the counts.
fn fan_out(data: &mut CompilerPipelineData, facts: &BTreeMap<PathBuf, PathFacts>) {
    for inv in &mut data.invocations {
        for entry in &mut inv.read_set {
            match facts.get(&entry.path) {
                Some(f) => {
                    entry.existed_at_close = f.existed;
                    entry.sha256_at_close = f.hash.clone();
                }
                None => {
                    entry.existed_at_close = false;
                    entry.sha256_at_close = None;
                }
            }
        }
        for entry in &mut inv.write_set {
            if let Some(f) = facts.get(&entry.path) {
                entry.existed_at_close = f.existed;
                entry.sha256_at_close = f.hash.clone();
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::attestation::compiler_pipeline::{
        CompilerFamily, CompilerInvocation, CompletenessState, ReadSetEntry, WriteSetEntry,
    };
    use waybill_common::types::timestamp::Timestamp;

    /// A budget that cannot truncate. Until T037 charges against it the
    /// value is inert, but every caller still has to name one — see
    /// `Budget`'s doc comment on why it has no `Default`.
    fn unbudgeted() -> Budget {
        Budget {
            total_bytes: u64::MAX,
            source: BudgetSource::Default,
        }
    }

    /// Build a one-invocation `CompilerPipelineData` from `(path, kind)`
    /// pairs. Close-time fields start at the honest pre-pass state --
    /// `false` and `None` -- exactly as `finalize_invocation` leaves them,
    /// so a test that asserts `existed_at_close == true` cannot pass by
    /// inheriting a default.
    fn pipeline(reads: &[(PathBuf, ReadKind)], writes: &[PathBuf]) -> CompilerPipelineData {
        pipeline_multi(&[(reads.to_vec(), writes.to_vec())])
    }

    /// The same, for several invocations — the FR-011 fan-out cases.
    fn pipeline_multi(invocations: &[InvocationSpec]) -> CompilerPipelineData {
        CompilerPipelineData {
            invocations: invocations
                .iter()
                .enumerate()
                .map(|(i, (reads, writes))| CompilerInvocation {
                    invocation_id: i as u64,
                    compiler: CompilerFamily::Rustc,
                    pid: 1000 + i as u32,
                    ppid: 1,
                    parent_invocation_id: None,
                    cgroup_id: 0,
                    start_timestamp: Timestamp::now(),
                    end_timestamp: None,
                    argv_full_path: None,
                    argv: vec![],
                    cwd: None,
                    exit_code: None,
                    read_set: reads
                        .iter()
                        .map(|(path, kind)| ReadSetEntry {
                            path: path.clone(),
                            kind: kind.clone(),
                            existed_at_close: false,
                            sha256_at_close: None,
                        })
                        .collect(),
                    write_set: writes
                        .iter()
                        .map(|path| WriteSetEntry {
                            path: path.clone(),
                            renamed_from: vec![],
                            existed_at_close: false,
                            sha256_at_close: None,
                        })
                        .collect(),
                    events_dropped: 0,
                })
                .collect(),
            dag_edges: vec![],
            completeness: CompletenessState::Complete,
            secrets_read_filtered: 0,
            include_system_reads_flag: false,
            filter_categories_applied: vec![],
        }
    }

    /// `(reads, writes)` for one synthetic invocation.
    type InvocationSpec = (Vec<(PathBuf, ReadKind)>, Vec<PathBuf>);

    /// The observation tier of one invocation: what the pass may not
    /// touch. `(read paths + kinds, write paths + rename histories)`.
    type ObservationTier = (Vec<(PathBuf, ReadKind)>, Vec<(PathBuf, Vec<PathBuf>)>);

    /// Snapshot the observation tier for the T017 equality assertion.
    fn observation_tier(data: &CompilerPipelineData) -> Vec<ObservationTier> {
        data.invocations
            .iter()
            .map(|inv| {
                (
                    inv.read_set
                        .iter()
                        .map(|e| (e.path.clone(), e.kind.clone()))
                        .collect(),
                    inv.write_set
                        .iter()
                        .map(|e| (e.path.clone(), e.renamed_from.clone()))
                        .collect(),
                )
            })
            .collect()
    }

    fn partition_holds(c: &CompilerHashCoverage) -> bool {
        c.paths_hashed + c.paths_absent + c.paths_skipped == c.paths_total
    }

    /// T016 / FR-013: every stat outcome lands in exactly one count, so
    /// the three always sum to the total. Asserted per case as well as in
    /// aggregate — a partition that holds only on the sum could still be
    /// charging a path to the wrong bucket.
    #[test]
    fn count_partition_holds_across_every_stat_outcome() {
        let dir = tempfile::tempdir().unwrap();

        let regular = dir.path().join("regular.rs");
        std::fs::write(&regular, b"fn main() {}").unwrap();
        let subdir = dir.path().join("a-directory");
        std::fs::create_dir(&subdir).unwrap();
        let missing = dir.path().join("deleted.o");

        let mut data = pipeline(
            &[(regular.clone(), ReadKind::File)],
            &[subdir.clone(), missing.clone()],
        );
        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 3);
        assert_eq!(cov.paths_absent, 1, "the deleted write");
        // The regular file is "existed but not hashed" until T020 adds
        // hashing, which is exactly true at this commit.
        assert_eq!(cov.paths_skipped, 2, "the directory and the regular file");
        assert_eq!(cov.paths_hashed, 0, "hashing arrives in T020");
        assert!(partition_holds(&cov));

        let inv = &data.invocations[0];
        let absent = inv
            .write_set
            .iter()
            .find(|e| e.path == missing)
            .expect("entry retained");
        assert!(!absent.existed_at_close, "a deleted write must say so");
        assert!(absent.sha256_at_close.is_none());

        let present_dir = inv
            .write_set
            .iter()
            .find(|e| e.path == subdir)
            .expect("entry retained");
        assert!(present_dir.existed_at_close, "the name resolved");
        assert!(present_dir.sha256_at_close.is_none(), "not a regular file");

        assert!(inv.read_set[0].existed_at_close);
        assert!(inv.read_set[0].sha256_at_close.is_none());
    }

    /// T016: a dangling symlink is a *successful* `symlink_metadata` of
    /// the link itself, so the name resolved even though nothing readable
    /// is behind it. `existed_at_close: true` with no hash, counted as
    /// skipped (contract C-1072-2).
    #[test]
    fn dangling_symlink_resolved_but_unhashable() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("dangling");

        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("nothing-here"), &link).unwrap();
        #[cfg(not(unix))]
        return;

        let mut data = pipeline(&[], std::slice::from_ref(&link));
        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 1);
        assert_eq!(cov.paths_skipped, 1);
        assert_eq!(cov.paths_absent, 0, "the link's own name did resolve");
        assert!(partition_holds(&cov));

        let e = &data.invocations[0].write_set[0];
        assert!(e.existed_at_close);
        assert!(e.sha256_at_close.is_none());
    }

    /// T017 / FR-010 / C-1072-7: the observation tier is read-only. The
    /// pass may not add, drop or reorder an entry, nor touch `path`,
    /// `kind` or `renamed_from`.
    #[test]
    fn observation_tier_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.rs");
        std::fs::write(&kept, b"// kept").unwrap();
        let gone = dir.path().join("gone.o");

        let mut data = pipeline(
            &[
                (kept.clone(), ReadKind::File),
                (PathBuf::from("<stdin>"), ReadKind::StdinInput { bytes_read: 7 }),
            ],
            &[gone.clone(), kept.clone()],
        );

        let before = observation_tier(&data);
        apply_close_time_facts(&mut data, unbudgeted());
        let after = observation_tier(&data);

        assert_eq!(before, after, "the pass rewrote the observation tier");
    }

    /// T018 / C-1072-2a: a `StdinInput` entry is excluded by its `kind`,
    /// not by how its path happens to be spelled.
    ///
    /// The entry points at a path that **does** resolve. A stat of a
    /// missing path also yields `false`, so an absent path would make this
    /// test pass whether or not the exclusion exists — the positive
    /// control is that the file is really there and the answer is still
    /// `false`.
    #[test]
    fn stdin_entry_is_never_statted() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("this-file-exists");
        std::fs::write(&real, b"not stdin").unwrap();

        let mut data = pipeline(
            &[(real.clone(), ReadKind::StdinInput { bytes_read: 42 })],
            &[],
        );
        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 0, "stdin contributes no path");
        assert_eq!(cov.paths_hashed, 0);
        assert_eq!(cov.paths_absent, 0);
        assert_eq!(cov.paths_skipped, 0);
        assert!(partition_holds(&cov));

        let e = &data.invocations[0].read_set[0];
        assert!(
            !e.existed_at_close,
            "stdin was stat'd: the file is there, so a stat would say true"
        );
        assert!(e.sha256_at_close.is_none());
    }

    /// FR-011: one path named by several invocations is visited once and
    /// the result fanned out, so two entries for it cannot disagree.
    #[test]
    fn shared_path_agrees_across_invocations() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared.rmeta");
        std::fs::write(&shared, b"shared").unwrap();
        let absent = dir.path().join("never-existed");

        let mut data = pipeline_multi(&[
            (vec![(shared.clone(), ReadKind::File)], vec![absent.clone()]),
            (vec![(shared.clone(), ReadKind::File)], vec![]),
            (vec![(absent.clone(), ReadKind::File)], vec![]),
        ]);
        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 2, "distinct paths, not entries");
        assert!(partition_holds(&cov));

        assert!(data.invocations[0].read_set[0].existed_at_close);
        assert!(data.invocations[1].read_set[0].existed_at_close);
        assert!(!data.invocations[0].write_set[0].existed_at_close);
        assert!(
            !data.invocations[2].read_set[0].existed_at_close,
            "the same path in a read set must agree with the write set"
        );
    }

    /// The visit order is a pure function of the path set: write-set paths
    /// lexicographically, then read-only paths lexicographically. SC-005
    /// and the budget's truncation point both rest on this.
    #[test]
    fn visit_order_is_writes_then_reads_each_sorted() {
        let data = pipeline(
            &[
                (PathBuf::from("/b/read"), ReadKind::File),
                (PathBuf::from("/a/read"), ReadKind::File),
                (PathBuf::from("/z/write"), ReadKind::File),
                (PathBuf::from("<stdin>"), ReadKind::StdinInput { bytes_read: 1 }),
            ],
            &[PathBuf::from("/z/write"), PathBuf::from("/m/write")],
        );

        assert_eq!(
            distinct_paths(&data),
            vec![
                PathBuf::from("/m/write"),
                PathBuf::from("/z/write"),
                PathBuf::from("/a/read"),
                PathBuf::from("/b/read"),
            ],
            "a path in both sets is visited once, in its write-set position"
        );
    }
}
