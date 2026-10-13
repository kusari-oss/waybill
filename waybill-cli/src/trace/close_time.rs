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
use std::time::Instant;

use tracing::{debug, warn};
use waybill_common::attestation::compiler_pipeline::{CompilerPipelineData, ReadKind};
use waybill_common::attestation::integrity::{BudgetSource, CompilerHashCoverage};
use waybill_common::types::hash::{ContentHash, HashAlgorithm, HexString};

use crate::scan_fs::walker::DEFAULT_SIZE_CAP_BYTES;
use crate::trace::hasher::sha256_file_hex;

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

/// Resolve the `--compiler-hash-budget-bytes` flag into a `Budget`
/// (C-1072-8).
///
/// A free function rather than a few lines at the call site because the
/// call site is behind
/// `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`, where no
/// unprivileged test can reach it. Here it is testable on every host.
///
/// `Some(0)` is an operator saying "hash nothing", not an absent flag, and
/// must not collapse into the default.
pub fn budget_from_flag(flag: Option<u64>, default_bytes: u64) -> Budget {
    match flag {
        Some(total_bytes) => Budget {
            total_bytes,
            source: BudgetSource::Operator,
        },
        None => Budget {
            total_bytes: default_bytes,
            source: BudgetSource::Default,
        },
    }
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

/// One path's visit: what was learned, which count it belongs to, how much
/// of the budget it consumed, and whether it was the budget that stopped it.
struct Visit {
    facts: PathFacts,
    outcome: Outcome,
    bytes_hashed: u64,
    over_budget: bool,
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
    run(data, budget, DEFAULT_SIZE_CAP_BYTES)
}

/// The pass, with the per-file size cap as a parameter.
///
/// Exists so the over-cap branch is reachable from a unit test with a
/// two-byte cap instead of a 256 MB fixture. Production always passes
/// `DEFAULT_SIZE_CAP_BYTES`.
fn run(data: &mut CompilerPipelineData, budget: Budget, cap: u64) -> CompilerHashCoverage {
    let started = Instant::now();
    let ordered = distinct_paths(data);

    let mut facts: BTreeMap<PathBuf, PathFacts> = BTreeMap::new();
    let mut hashed = 0u64;
    let mut absent = 0u64;
    let mut skipped = 0u64;
    let mut bytes_hashed = 0u64;
    let mut remaining = budget.total_bytes;
    let mut truncated = false;

    for path in &ordered {
        let v = visit(path, cap, remaining);
        match v.outcome {
            Outcome::Hashed => hashed += 1,
            Outcome::Absent => absent += 1,
            Outcome::Skipped => skipped += 1,
        }
        bytes_hashed += v.bytes_hashed;
        // Charged once per distinct path, at its first (only) visit, since
        // the walk is over distinct paths and the result is fanned out.
        remaining = remaining.saturating_sub(v.bytes_hashed);
        if v.over_budget {
            // Do NOT break. One oversized artifact early in sort order
            // must not blind the pass to every path after it (FR-012);
            // smaller files later can still fit the remainder.
            truncated = true;
        }
        facts.insert(path.clone(), v.facts);
    }

    fan_out(data, &facts);

    // Deliberately DEBUG, not INFO: with the feature on, an unconditional
    // INFO line would be new default output, and C-1072-11 is a promise
    // about not changing what an operator sees. It is also how SC-008's
    // ratio is measured -- see `measurements/cost.md` -- so the elapsed
    // figure needs to be obtainable without a code change.
    //
    // Nothing here reaches the attestation: a duration in the predicate
    // would break the byte-identity that signing depends on (C-1072-6).
    debug!(
        paths_total = ordered.len(),
        paths_hashed = hashed,
        paths_absent = absent,
        paths_skipped = skipped,
        bytes_hashed,
        elapsed_ms = started.elapsed().as_millis(),
        "close-time pass complete"
    );

    // FR-015, C-1072-10: exactly one WARN per trace, naming the counts and
    // the budget. Principle III is not engaged -- no observation is lost
    // when hashing truncates, so the attestation is still written and the
    // command still exits zero. Principle X's remedy for "cannot
    // guarantee" is structured disclosure, which the coverage record is.
    if truncated {
        warn!(
            paths_hashed = hashed,
            paths_skipped = skipped,
            paths_absent = absent,
            paths_total = ordered.len(),
            budget_bytes = budget.total_bytes,
            "close-time hashing budget exhausted; coverage is partial"
        );
    }

    CompilerHashCoverage {
        paths_total: ordered.len() as u64,
        paths_hashed: hashed,
        paths_absent: absent,
        paths_skipped: skipped,
        truncated,
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
fn visit(path: &PathBuf, cap: u64, remaining: u64) -> Visit {
    let skipped = |facts: PathFacts, over_budget: bool| Visit {
        facts,
        outcome: Outcome::Skipped,
        bytes_hashed: 0,
        over_budget,
    };

    let Ok(md) = std::fs::symlink_metadata(path) else {
        return Visit {
            facts: PathFacts {
                existed: false,
                hash: None,
            },
            outcome: Outcome::Absent,
            bytes_hashed: 0,
            over_budget: false,
        };
    };

    // The name resolved. Whether anything readable is behind it is a
    // separate question, which `sha256_at_close`'s presence answers.
    let existed = PathFacts {
        existed: true,
        hash: None,
    };

    if !md.is_file() {
        debug!(path = %path.display(), "close-time: not a regular file, not hashed");
        return skipped(existed, false);
    }

    // FR-012: the budget is checked before the read, not after. Hashing a
    // file and then discarding it would spend the cost the budget exists
    // to bound.
    if md.len() > remaining {
        debug!(
            path = %path.display(),
            size = md.len(),
            remaining,
            "close-time: over the remaining budget, not hashed"
        );
        return skipped(existed, true);
    }

    // `sha256_file_hex` refuses a file over the cap rather than hashing a
    // prefix (research R2), so an over-cap file arrives here as an `Err`
    // and lands in `paths_skipped` like any other unreadable file. The
    // budget is not consulted yet: T037 adds the charging, and the call
    // site passes `u64::MAX` until T036 measures a default, so US1 ships
    // correct-but-unbounded exactly as spec.md describes.
    match sha256_file_hex(path, cap) {
        Ok(hex) => match HexString::new(&hex) {
            Ok(value) => Visit {
                facts: PathFacts {
                    existed: true,
                    hash: Some(ContentHash {
                        algorithm: HashAlgorithm::Sha256,
                        value,
                    }),
                },
                outcome: Outcome::Hashed,
                bytes_hashed: md.len(),
                over_budget: false,
            },
            Err(e) => {
                // Unreachable in practice -- the hasher emits lowercase
                // hex -- but a panic here would abort a whole trace's
                // post-processing over one path (Boundary 4).
                warn!(
                    path = %path.display(),
                    error = %e,
                    "close-time: hasher returned a value that is not hex; not hashed"
                );
                skipped(existed, false)
            }
        },
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "close-time: could not hash; counted as skipped"
            );
            skipped(existed, false)
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
        // Before T020 added hashing this was `skipped: 2, hashed: 0` --
        // the regular file was "existed but not hashed", which was true
        // then. T020 reclassified it, and the partition held across the
        // move, which is what T016 exists to prove.
        assert_eq!(cov.paths_skipped, 1, "the directory");
        assert_eq!(cov.paths_hashed, 1, "the regular file");
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
        let hash = inv.read_set[0]
            .sha256_at_close
            .as_ref()
            .expect("a readable regular file is hashed");
        // sha256 of "fn main() {}" from `shasum -a 256`, not from this
        // code -- the same independence SC-003 asks for on a real trace.
        assert_eq!(
            hash.value.as_str(),
            "ef32637cb9c3ec2e3968c9cbdf26a5e9c172be94f88af533e14bd43f892d5297"
        );
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

    // std::os::unix doesn't expose geteuid directly; use the C ABI
    // (always available on unix without adding a libc dep). Same idiom as
    // `scan_fs/podman_source.rs:571`.
    #[cfg(unix)]
    extern "C" {
        fn geteuid() -> u32;
    }

    /// T021: a file over the per-file cap is skipped, not truncated.
    ///
    /// Research R2's correction: `sha256_file_hex` returns
    /// `Err(InvalidData)` for an oversized file rather than hashing a
    /// prefix, so there is no way for a partial digest to be emitted as
    /// if it were the whole file. The path still existed, so the entry
    /// says so — "existed but not hashed" is the honest answer.
    #[test]
    fn over_cap_file_is_skipped_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("over-the-cap.rlib");
        std::fs::write(&big, b"more than two bytes").unwrap();

        let mut data = pipeline(&[], std::slice::from_ref(&big));
        let cov = run(&mut data, unbudgeted(), 2);

        assert_eq!(cov.paths_total, 1);
        assert_eq!(cov.paths_skipped, 1);
        assert_eq!(cov.paths_hashed, 0);
        assert_eq!(cov.paths_absent, 0, "it was right there");
        assert!(partition_holds(&cov));

        let e = &data.invocations[0].write_set[0];
        assert!(e.existed_at_close);
        assert!(
            e.sha256_at_close.is_none(),
            "a prefix hash would be worse than no hash"
        );

        // Control: the same file under a cap that admits it does hash,
        // so the skip above is the cap's doing and not a broken fixture.
        let mut data = pipeline(&[], std::slice::from_ref(&big));
        let cov = run(&mut data, unbudgeted(), 1024);
        assert_eq!(cov.paths_hashed, 1);
        assert!(data.invocations[0].write_set[0].sha256_at_close.is_some());
    }

    /// T022: an unreadable file is skipped and the pass keeps going.
    ///
    /// One bad path must not cost the other 400 their close-time facts.
    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_skipped_and_the_pass_continues() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { geteuid() } == 0 {
            // Root ignores the mode bits, so the file would hash and the
            // assertion would be meaningless. Skipping beats asserting
            // something untrue of the environment.
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("no-permissions.rlib");
        std::fs::write(&locked, b"secret").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let after = dir.path().join("zzz-later-in-sort-order.rlib");
        std::fs::write(&after, b"readable").unwrap();

        let mut data = pipeline(&[], &[locked.clone(), after.clone()]);
        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 2);
        assert_eq!(cov.paths_skipped, 1, "the unreadable one");
        assert_eq!(cov.paths_hashed, 1, "the pass did not abort");
        assert!(partition_holds(&cov));

        let bad = data.invocations[0]
            .write_set
            .iter()
            .find(|e| e.path == locked)
            .unwrap();
        assert!(bad.existed_at_close, "the name resolved");
        assert!(bad.sha256_at_close.is_none());

        let good = data.invocations[0]
            .write_set
            .iter()
            .find(|e| e.path == after)
            .unwrap();
        assert!(
            good.sha256_at_close.is_some(),
            "a path after the failure in sort order must still be measured"
        );
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

    /// T026 / US2 scenario 3, Principle VIII: a read path deleted before
    /// the trace ended reports `existed_at_close: false` and stays in the
    /// read set. Dropping the entry would erase an observation the kernel
    /// actually made -- the compiler did read that file.
    #[test]
    fn deleted_read_path_is_reported_not_removed() {
        let dir = tempfile::tempdir().unwrap();
        let generated = dir.path().join("build-script-output.rs");
        std::fs::write(&generated, b"// generated, then cleaned up").unwrap();
        let kept = dir.path().join("lib.rs");
        std::fs::write(&kept, b"// still here").unwrap();

        let mut data = pipeline(
            &[
                (generated.clone(), ReadKind::File),
                (kept.clone(), ReadKind::File),
            ],
            &[],
        );

        // The build deletes it between the read and the trace ending.
        std::fs::remove_file(&generated).unwrap();

        let cov = apply_close_time_facts(&mut data, unbudgeted());

        assert_eq!(cov.paths_total, 2);
        assert_eq!(cov.paths_absent, 1);
        assert_eq!(cov.paths_hashed, 1);
        assert!(partition_holds(&cov));

        assert_eq!(
            data.invocations[0].read_set.len(),
            2,
            "the deleted path must still be in the read set"
        );
        let gone = data.invocations[0]
            .read_set
            .iter()
            .find(|e| e.path == generated)
            .expect("entry retained");
        assert!(!gone.existed_at_close);
        assert!(gone.sha256_at_close.is_none());
    }

    /// T040 / FR-012, SC-006: a zero budget hashes nothing, says so, and
    /// still reports every path's existence. Truncation costs hashes, not
    /// observations.
    #[test]
    fn zero_budget_hashes_nothing_and_reports_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.rlib");
        let b = dir.path().join("b.rlib");
        std::fs::write(&a, b"content a").unwrap();
        std::fs::write(&b, b"content b").unwrap();
        let gone = dir.path().join("gone.o");

        let mut data = pipeline(&[], &[a.clone(), b.clone(), gone.clone()]);
        let cov = apply_close_time_facts(
            &mut data,
            Budget {
                total_bytes: 0,
                source: BudgetSource::Operator,
            },
        );

        assert!(cov.truncated);
        assert_eq!(cov.paths_hashed, 0);
        assert_eq!(cov.paths_skipped, 2);
        assert_eq!(cov.paths_absent, 1, "existence is still reported");
        assert!(partition_holds(&cov));
        assert_eq!(cov.budget_source, BudgetSource::Operator);

        for e in &data.invocations[0].write_set {
            assert!(e.sha256_at_close.is_none());
        }
    }

    /// T040 / FR-012: a budget that cuts mid-way does not stop the pass.
    /// A file too big for the remainder is skipped, and smaller files
    /// later in sort order still get hashed — one oversized artifact must
    /// not blind the pass to everything after it.
    #[test]
    fn budget_skips_the_oversized_and_keeps_going() {
        let dir = tempfile::tempdir().unwrap();
        // Sort order is a, b, c. `b` is the one that cannot fit.
        let a = dir.path().join("a.rlib");
        let b = dir.path().join("b.rlib");
        let c = dir.path().join("c.rlib");
        std::fs::write(&a, vec![b'a'; 10]).unwrap();
        std::fs::write(&b, vec![b'b'; 500]).unwrap();
        std::fs::write(&c, vec![b'c'; 10]).unwrap();

        let mut data = pipeline(&[], &[a.clone(), b.clone(), c.clone()]);
        // Room for a and c (10 + 10) but never for b (500).
        let cov = apply_close_time_facts(
            &mut data,
            Budget {
                total_bytes: 25,
                source: BudgetSource::Default,
            },
        );

        assert!(cov.truncated);
        assert_eq!(cov.paths_hashed, 2, "a and c");
        assert_eq!(cov.paths_skipped, 1, "b");
        assert!(partition_holds(&cov));

        let hash_of = |p: &PathBuf| {
            data.invocations[0]
                .write_set
                .iter()
                .find(|e| &e.path == p)
                .and_then(|e| e.sha256_at_close.clone())
        };
        assert!(hash_of(&a).is_some());
        assert!(hash_of(&b).is_none(), "over the remaining budget");
        assert!(
            hash_of(&c).is_some(),
            "a path after the oversized one must still be measured"
        );
    }

    /// T040 / SC-005: truncation is reproducible. Two runs over the same
    /// input under the same budget must hash the identical path set, since
    /// the budget is charged in sorted-path order and nothing about the
    /// decision depends on wall-clock, thread scheduling or map iteration.
    #[test]
    fn truncation_point_is_identical_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for i in 0..8 {
            let p = dir.path().join(format!("artifact-{i}.rlib"));
            std::fs::write(&p, vec![b'x'; 100]).unwrap();
            paths.push(p);
        }

        let hashed_set = |budget: u64| -> Vec<PathBuf> {
            let mut data = pipeline(&[], &paths);
            apply_close_time_facts(
                &mut data,
                Budget {
                    total_bytes: budget,
                    source: BudgetSource::Default,
                },
            );
            let mut v: Vec<PathBuf> = data.invocations[0]
                .write_set
                .iter()
                .filter(|e| e.sha256_at_close.is_some())
                .map(|e| e.path.clone())
                .collect();
            v.sort();
            v
        };

        // 350 bytes admits exactly three 100-byte files.
        let first = hashed_set(350);
        let second = hashed_set(350);
        assert_eq!(first, second, "the truncation point moved between runs");
        assert_eq!(first.len(), 3);
        // And it is the first three in sort order, not an arbitrary three.
        assert_eq!(first, {
            let mut expected = paths[0..3].to_vec();
            expected.sort();
            expected
        });
    }

    /// T041: the `source` on the budget handed in is the one that reaches
    /// the coverage record, for both variants. The pass never sees a flag,
    /// so the "absent flag → Default" half of the mapping is asserted at
    /// the flag site in `cli/scan.rs`, not here.
    #[test]
    fn budget_source_passes_through_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.rlib");
        std::fs::write(&f, b"x").unwrap();

        for source in [BudgetSource::Default, BudgetSource::Operator] {
            let mut data = pipeline(&[], std::slice::from_ref(&f));
            let cov = apply_close_time_facts(
                &mut data,
                Budget {
                    total_bytes: u64::MAX,
                    source,
                },
            );
            assert_eq!(cov.budget_source, source);
            assert!(!cov.truncated, "u64::MAX cannot truncate");
        }
    }

    /// T036 / C-1072-8: the flag-to-`Budget` mapping, asserted at the
    /// layer that owns it. An absent flag takes the measured default and
    /// reports `Default`; any supplied value reports `Operator`.
    ///
    /// `Some(0)` is the case worth writing down: an operator asking for
    /// no hashing at all must not be folded into the default, which is
    /// what a `unwrap_or(default)` would have done.
    #[test]
    fn flag_maps_to_budget_and_source() {
        let default_bytes = 2 * 1024 * 1024 * 1024;

        let absent = budget_from_flag(None, default_bytes);
        assert_eq!(absent.total_bytes, default_bytes);
        assert_eq!(absent.source, BudgetSource::Default);

        let supplied = budget_from_flag(Some(4096), default_bytes);
        assert_eq!(supplied.total_bytes, 4096);
        assert_eq!(supplied.source, BudgetSource::Operator);

        let zero = budget_from_flag(Some(0), default_bytes);
        assert_eq!(zero.total_bytes, 0, "an explicit 0 is not an absent flag");
        assert_eq!(zero.source, BudgetSource::Operator);
    }

    /// T041 / SC-007, FR-014: a `TraceIntegrity` without a coverage record
    /// serializes without the key, so a trace that captured no compiler
    /// invocations stays byte-identical to a pre-m1072 attestation.
    #[test]
    fn absent_coverage_record_serializes_without_the_key() {
        use waybill_common::attestation::integrity::TraceIntegrity;

        let integrity = TraceIntegrity::default();
        assert!(integrity.compiler_hash_coverage.is_none());
        let json = serde_json::to_string(&integrity).unwrap();
        assert!(!json.contains("compiler_hash_coverage"), "{json}");

        let with = TraceIntegrity {
            compiler_hash_coverage: Some(CompilerHashCoverage {
                paths_total: 1,
                paths_hashed: 1,
                paths_absent: 0,
                paths_skipped: 0,
                truncated: false,
                budget_source: BudgetSource::Default,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&with).unwrap();
        assert!(json.contains("\"compiler_hash_coverage\""), "{json}");
        assert!(json.contains("\"budget_source\":\"default\""), "{json}");
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
