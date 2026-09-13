// Feature 840 — contract C-5 verification.
//
// A normaliser that silently ate real changes would be worse than no
// normaliser: it would make a reviewer confident about a diff they never
// actually saw. So these assert both directions — noise disappears, and
// signal survives.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use super::normalise;
use serde_json::json;

// ---------------------------------------------------------------
// C-5.1 — an ordering-only difference normalises away
// ---------------------------------------------------------------

#[test]
fn ordering_only_difference_normalises_to_equal() {
    // The motivating case: SPDX 3 wraps output in `@graph`, whose order
    // is not stable across runs. Without this, a reordered array reads
    // as every element having changed.
    let a = json!({"@graph": [
        {"spdxId": "a", "name": "alpha"},
        {"spdxId": "b", "name": "beta"},
        {"spdxId": "c", "name": "gamma"}
    ]});
    let b = json!({"@graph": [
        {"spdxId": "c", "name": "gamma"},
        {"spdxId": "a", "name": "alpha"},
        {"spdxId": "b", "name": "beta"}
    ]});
    assert_eq!(normalise(&a), normalise(&b));
}

#[test]
fn nested_ordering_also_normalises() {
    // Reordering inside a nested array must normalise too, or the tool
    // only fixes the outermost level and quietly fails deeper down.
    let a = json!({"components": [{"name": "x", "licenses": ["MIT", "Apache-2.0"]}]});
    let b = json!({"components": [{"name": "x", "licenses": ["Apache-2.0", "MIT"]}]});
    assert_eq!(normalise(&a), normalise(&b));
}

// ---------------------------------------------------------------
// C-5.2 — a real change survives normalisation
// ---------------------------------------------------------------

#[test]
fn single_changed_licence_still_differs() {
    let a = json!({"components": [
        {"name": "x", "licenses": ["MIT"]},
        {"name": "y", "licenses": ["Apache-2.0"]}
    ]});
    let b = json!({"components": [
        {"name": "x", "licenses": ["GPL-3.0"]},
        {"name": "y", "licenses": ["Apache-2.0"]}
    ]});
    assert_ne!(
        normalise(&a),
        normalise(&b),
        "a changed licence must survive normalisation — this is the \
         failure mode that would make the tool actively harmful",
    );
}

#[test]
fn added_component_still_differs() {
    let a = json!({"components": [{"name": "x"}]});
    let b = json!({"components": [{"name": "x"}, {"name": "y"}]});
    assert_ne!(normalise(&a), normalise(&b));
}

#[test]
fn removed_field_still_differs() {
    let a = json!({"components": [{"name": "x", "purl": "pkg:npm/x@1"}]});
    let b = json!({"components": [{"name": "x"}]});
    assert_ne!(normalise(&a), normalise(&b));
}

// ---------------------------------------------------------------
// C-5.3 — self-comparison is empty
// ---------------------------------------------------------------

#[test]
fn document_compared_with_itself_is_equal() {
    let doc = json!({
        "bomFormat": "CycloneDX",
        "components": [
            {"name": "b", "licenses": ["MIT"]},
            {"name": "a", "licenses": ["Apache-2.0"]}
        ],
        "dependencies": [{"ref": "x", "dependsOn": ["y", "z"]}]
    });
    assert_eq!(normalise(&doc), normalise(&doc));
}

// ---------------------------------------------------------------
// C-2.5 — the sort key must be total and deterministic
// ---------------------------------------------------------------

#[test]
fn normalisation_is_idempotent() {
    // Normalising twice must equal normalising once. If it does not, the
    // sort key is unstable and the tool would report phantom differences
    // depending on input order.
    let doc = json!({"a": [3, 1, 2], "b": [{"z": 1}, {"y": 2}]});
    let once = normalise(&doc);
    let twice = normalise(&once);
    assert_eq!(once, twice);
}

#[test]
fn elements_that_tie_on_prefix_still_order_deterministically() {
    // Two elements sharing a prefix must not tie. A comparator that tied
    // here would leave order input-dependent, reintroducing exactly the
    // problem this tool exists to solve.
    let a = json!([{"n": "pkg"}, {"n": "pkg-extra"}]);
    let b = json!([{"n": "pkg-extra"}, {"n": "pkg"}]);
    assert_eq!(normalise(&a), normalise(&b));
}

// ---------------------------------------------------------------
// C-2.3 / C-5.4 — never writes to a golden
// ---------------------------------------------------------------

#[test]
fn normalise_does_not_mutate_its_input() {
    // The in-process half of "never writes back". The on-disk half is
    // covered by the committed-goldens check below.
    let original = json!({"components": [{"n": "b"}, {"n": "a"}]});
    let snapshot = original.clone();
    let _ = normalise(&original);
    assert_eq!(original, snapshot, "normalise must not mutate its argument");
}

#[test]
fn committed_goldens_are_untouched_by_a_diff_run() {
    // C-5.4. The gate compares bytes; if this tool ever wrote back, a
    // reordered-but-equal golden would start comparing EQUAL and the
    // gate would silently stop detecting reordering.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("waybill-cli/tests/fixtures/public_corpus");
    if !root.is_dir() {
        // Fixtures live in a sibling repo on some checkouts; skipping is
        // correct here, and the CI lane covers the populated case.
        return;
    }
    let sample = root.join("go-cobra").join("cdx.json");
    if !sample.exists() {
        return;
    }
    let before = std::fs::read(&sample).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let _ = normalise(&parsed);
    let after = std::fs::read(&sample).unwrap();
    assert_eq!(
        before, after,
        "a corpus-diff run must leave committed goldens byte-identical",
    );
}

// ---------------------------------------------------------------
// C-3.3 — arrays whose LENGTH changed must still be described
// element-wise. Before milestone 840's attribution pass, `walk`
// descended into arrays only when both sides had equal length;
// anything else fell through to the catch-all and printed one
// `changed $.path` line. That made SPDX 3 `@graph` unreviewable for
// every target that gained or lost an element — 7 of 11 in the
// refresh, including a 5036 -> 6315 change reported as a single line.
// ---------------------------------------------------------------

use super::{align_by_identity, identity_key, walk};
use serde_json::Value;

/// Run the same pipeline `report` uses, returning the raw lines.
fn diff_lines(a: &Value, b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(&normalise(a), &normalise(b), String::from("$"), &mut out);
    out
}

#[test]
fn length_mismatch_reports_the_added_element_not_the_whole_array() {
    let a = json!({"@graph": [
        {"type": "software_Package", "name": "alpha", "spdxId": "SPDXRef-1"}
    ]});
    let b = json!({"@graph": [
        {"type": "software_Package", "name": "alpha", "spdxId": "SPDXRef-1"},
        {"type": "software_Package", "name": "beta",  "spdxId": "SPDXRef-2"}
    ]});
    let lines = diff_lines(&a, &b);
    assert!(
        lines.iter().any(|l| l.starts_with("added") && l.contains("beta")),
        "the new element must be named; got {lines:?}",
    );
    assert!(
        !lines.iter().any(|l| l == "changed $.@graph"),
        "the whole array must not collapse to one opaque line; got {lines:?}",
    );
}

#[test]
fn identity_ignores_content_addressed_ids() {
    // The motivating trap: SPDX ids are hashes of the content whose
    // change we are describing. Keying on them pairs nothing, and every
    // element reports as both added and removed.
    let a = json!({"@graph": [
        {"type": "software_Package", "name": "alpha", "spdxId": "SPDXRef-OLD"},
        {"type": "software_Package", "name": "beta",  "spdxId": "SPDXRef-X"}
    ]});
    let b = json!({"@graph": [
        {"type": "software_Package", "name": "alpha", "spdxId": "SPDXRef-NEW"}
    ]});
    let lines = diff_lines(&a, &b);
    assert!(
        lines.iter().any(|l| l.contains("spdxId")),
        "alpha must pair by name so its id change is visible; got {lines:?}",
    );
    assert!(
        lines.iter().any(|l| l.starts_with("removed") && l.contains("beta")),
        "beta must report as removed; got {lines:?}",
    );
}

#[test]
fn scalar_array_length_mismatch_names_the_values() {
    let a = json!({"documentDescribes": ["a", "b"]});
    let b = json!({"documentDescribes": ["a"]});
    let lines = diff_lines(&a, &b);
    assert!(
        lines.iter().any(|l| l.starts_with("removed") && l.contains('b')),
        "a dropped scalar must be named, not summarised; got {lines:?}",
    );
}

#[test]
fn purl_identity_survives_a_field_edit() {
    let a = json!({"components": [
        {"purl": "pkg:npm/x@1.0.0", "licenses": ["MIT"]},
        {"purl": "pkg:npm/y@1.0.0", "licenses": ["MIT"]}
    ]});
    let b = json!({"components": [
        {"purl": "pkg:npm/x@1.0.0", "licenses": ["GPL-3.0"]}
    ]});
    let lines = diff_lines(&a, &b);
    assert!(
        lines.iter().any(|l| l.contains("licenses") && l.contains("pkg:npm/x@1.0.0")),
        "x must pair by purl so the licence change is attributed; got {lines:?}",
    );
}

#[test]
fn identity_key_never_derives_from_a_content_hash() {
    let file_tier = json!({"bom-ref": "pkg:generic/file-tier?content-sha256=deadbeef"});
    let key = identity_key(&file_tier);
    assert!(
        key.as_deref().is_none_or(|k| !k.contains("deadbeef")),
        "a content hash must not become an identity key; got {key:?}",
    );
}

#[test]
fn unkeyable_elements_are_reported_not_dropped() {
    // An element no rule can key must still surface. Silently ignoring
    // it is the failure this tool exists to prevent.
    let mut out = Vec::new();
    let a: Vec<Value> = vec![];
    let b: Vec<Value> = vec![json!({"no_identity_field": 1})];
    align_by_identity(&a, &b, "$.x", &mut out);
    assert!(
        !out.is_empty(),
        "an unkeyable added element must produce a line",
    );
}
