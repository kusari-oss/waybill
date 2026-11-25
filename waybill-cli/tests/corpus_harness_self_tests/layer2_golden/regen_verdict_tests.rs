#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms, FailureFormat, update_goldens_gate};
#[allow(unused_imports)]
use crate::corpus_harness_195::js_filter;
use crate::corpus_harness_195::layer2_golden::regen_verdict;

#[test]
fn an_identical_golden_is_unchanged() {
    assert_eq!(regen_verdict(Some(b"{}"), b"{}"), "unchanged");
}

#[test]
fn a_differing_golden_is_changed() {
    // The case #1345 is about: a golden that differs by one property
    // was reported nowhere, because the regen path wrote without
    // reading and the summary fell back to listing failed tests.
    assert_eq!(regen_verdict(Some(b"{\"a\":1}"), b"{\"a\":2}"), "CHANGED");
}

#[test]
fn an_absent_golden_is_created() {
    assert_eq!(regen_verdict(None, b"{}"), "CREATED");
}

#[test]
fn a_byte_difference_is_enough() {
    // Goldens are compared as bytes elsewhere in this file, so the
    // verdict must not be cleverer than the thing it reports on.
    assert_eq!(regen_verdict(Some(b"{}\n"), b"{}"), "CHANGED");
}
