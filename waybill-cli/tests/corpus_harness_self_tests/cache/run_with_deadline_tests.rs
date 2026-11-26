#[allow(unused_imports)]
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::CorpusInfraError;
#[allow(unused_imports)]
use crate::corpus_harness_195::manifest::{CorpusTarget, PinnedRef, SourceKind};
use crate::corpus_harness_195::cache::run_with_deadline;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn a_hung_command_is_killed_at_the_deadline() {
    let started = Instant::now();
    let err = run_with_deadline(Command::new("sh").args(["-c", "echo building; sleep 30"]), Duration::from_secs(2))
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10), "not killed: {:?}", started.elapsed());
    assert!(err.contains("killed after the 2s deadline"), "{err}");
    assert!(err.contains("building"), "the output before the hang is kept: {err}");
}

#[test]
fn a_failure_reports_the_tail_of_its_output() {
    let err = run_with_deadline(
        Command::new("sh").args(["-c", "for i in $(seq 1 100); do echo line$i; done; echo 'error: the cause' >&2; exit 3"]),
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(err.contains("error: the cause"), "{err}");
    assert!(err.contains("line100") && !err.contains("line1\n"), "the tail, not the head: {err}");
}

#[test]
fn success_is_ok() {
    assert_eq!(run_with_deadline(Command::new("sh").args(["-c", "echo fine"]), Duration::from_secs(30)), Ok(()));
}
