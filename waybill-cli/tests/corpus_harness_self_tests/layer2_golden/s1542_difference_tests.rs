#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms, FailureFormat, update_goldens_gate};
#[allow(unused_imports)]
use crate::corpus_harness_195::js_filter;
use crate::corpus_harness_195::layer2_golden::*;
use serde_json::json;

/// A masked field differing is not a difference.
#[test]
fn a_masked_field_is_no_difference() {
    let a = json!({ "serialNumber": "urn:uuid:1", "components": [{ "purl": "pkg:generic/x" }] });
    let b = json!({ "serialNumber": "urn:uuid:2", "components": [{ "purl": "pkg:generic/x" }] });
    assert_eq!(masked_difference(&a, &b), None);
}

/// An unmasked field differing is a content difference, at its path.
#[test]
fn an_unmasked_field_is_a_content_difference() {
    let a = json!({ "components": [{ "purl": "pkg:generic/x" }] });
    let b = json!({ "components": [{ "purl": "pkg:generic/y" }] });
    let d = masked_difference(&a, &b).unwrap();
    assert!(d.starts_with("content: /components[0]/purl"), "{d}");
}

/// The same elements in another order is an order-only difference.
#[test]
fn a_reordered_array_is_an_order_only_difference() {
    let a = json!({ "components": [{ "purl": "pkg:generic/x" }, { "purl": "pkg:generic/y" }] });
    let b = json!({ "components": [{ "purl": "pkg:generic/y" }, { "purl": "pkg:generic/x" }] });
    assert!(masked_difference(&a, &b).unwrap().starts_with("order-only"));
}
