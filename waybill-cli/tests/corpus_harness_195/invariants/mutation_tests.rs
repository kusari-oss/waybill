//! SC-004 — every invariant must be shown to fail.
//!
//! A green cell is not evidence until you have seen it go red. Two traps
//! this repo has recorded the hard way:
//!
//! - *a passing check that exercises nothing* —
//!   `feedback_a_passing_test_may_exercise_nothing`;
//! - *healthy and broken looking identical* —
//!   `feedback_zero_results_can_mean_broken`.
//!
//! Each test below takes a target whose cell **passes**, asserts that
//! baseline explicitly rather than assuming it, perturbs the emitted
//! document so the property is violated, and asserts the cell goes red
//! with the expected kind. An invariant that cannot be made to fail
//! checks nothing.

use super::checks;
use super::goldens;
use super::{CellOutcome, Invariant, REGISTRY};

fn registry_entry(id: &str) -> &'static Invariant {
    REGISTRY.iter().find(|i| i.id == id).expect("invariant in registry")
}

/// Load a golden, assert the invariant currently passes on it, apply a
/// mutation, and return the mutated outcome.
fn mutate(id: &str, target: &str, mutate: impl FnOnce(&mut serde_json::Value)) -> CellOutcome {
    let inv = registry_entry(id);
    let mut sboms = goldens::load(target).expect("golden triple");
    let before = (inv.checker)(&sboms);
    assert_eq!(
        before,
        CellOutcome::Pass,
        "{id} must PASS on {target} before the mutation, else the test proves nothing \
         about the mutation. Got {before:?}"
    );
    mutate(&mut sboms.cdx);
    (inv.checker)(&sboms)
}

fn assert_fails_with(outcome: &CellOutcome, kind: &str) {
    match outcome {
        CellOutcome::Fail(vs) => assert!(
            vs.iter().any(|v| v.kind == kind),
            "expected kind `{kind}`, got {:?}",
            vs.iter().map(|v| v.kind).collect::<Vec<_>>()
        ),
        other => panic!("expected Fail({kind}), got {other:?}"),
    }
}

/// Which target passes a given invariant, for picking mutation subjects.
fn first_passing(id: &str) -> String {
    let inv = registry_entry(id);
    goldens::load_all()
        .into_iter()
        .find(|(_, s)| (inv.checker)(s) == CellOutcome::Pass)
        .map(|(n, _)| n)
        .unwrap_or_else(|| panic!("no target passes {id}; cannot mutate a pass into a fail"))
}

fn subject_ref(cdx: &serde_json::Value) -> String {
    cdx["metadata"]["component"]["bom-ref"].as_str().expect("subject bom-ref").to_string()
}

fn push_dependson(cdx: &mut serde_json::Value, from: &str, to: &str) {
    let deps = cdx["dependencies"].as_array_mut().expect("dependencies array");
    if let Some(entry) = deps.iter_mut().find(|d| d["ref"].as_str() == Some(from)) {
        entry["dependsOn"].as_array_mut().expect("dependsOn").push(serde_json::json!(to));
    } else {
        deps.push(serde_json::json!({"ref": from, "dependsOn": [to]}));
    }
}

#[test]
fn i1_can_be_made_to_fail() {
    let target = first_passing("I1");
    let out = mutate("I1", &target, |cdx| {
        // Point the subject at a file-tier component it does not depend on.
        let file_ref = cdx["components"]
            .as_array()
            .expect("components")
            .iter()
            .find(|c| {
                c["type"].as_str() == Some("file")
                    || c["properties"].as_array().map(|ps| {
                        ps.iter().any(|p| {
                            p["name"].as_str() == Some("waybill:component-tier")
                                && p["value"].as_str() == Some("file")
                        })
                    }) == Some(true)
            })
            .and_then(|c| c["bom-ref"].as_str())
            .expect("a file-tier component, since I1's precondition was met")
            .to_string();
        let subject = subject_ref(cdx);
        push_dependson(cdx, &subject, &file_ref);
    });
    assert_fails_with(&out, checks::file_tier_edges::KIND_FILE_TIER_DEPENDENCY_EDGE);
}

#[test]
fn i2_can_be_made_to_fail() {
    let target = first_passing("I2");
    let out = mutate("I2", &target, |cdx| {
        // Emit one component twice under the same identity.
        let arr = cdx["components"].as_array_mut().expect("components");
        let dup = arr[0].clone();
        arr.push(dup);
    });
    assert_fails_with(&out, checks::identity_unique::KIND_LIBRARY);
}

#[test]
fn i3_can_be_made_to_fail() {
    let target = first_passing("I3");
    let out = mutate("I3", &target, |cdx| {
        // Hang an unmarked component off a marked non-runtime one, so it
        // is reachable only through a non-runtime edge.
        let marked = cdx["components"]
            .as_array()
            .expect("components")
            .iter()
            .find(|c| {
                c["properties"].as_array().map(|ps| {
                    ps.iter().any(|p| {
                        p["name"].as_str() == Some("waybill:lifecycle-scope")
                            && checks::NON_RUNTIME_SCOPES
                                .contains(&p["value"].as_str().unwrap_or(""))
                    })
                }) == Some(true)
            })
            .and_then(|c| c["bom-ref"].as_str())
            .expect("a non-runtime-marked component, since I3's precondition was met")
            .to_string();
        cdx["components"].as_array_mut().expect("components").push(serde_json::json!({
            "bom-ref": "pkg:generic/mutation-unmarked@1",
            "name": "mutation-unmarked",
            "version": "1",
            "type": "library",
            "purl": "pkg:generic/mutation-unmarked@1"
        }));
        push_dependson(cdx, &marked, "pkg:generic/mutation-unmarked@1");
    });
    assert_fails_with(&out, checks::scope_propagation::KIND_UNMARKED_NON_RUNTIME_REACHABLE);
}

#[test]
fn i4_can_be_made_to_fail() {
    let target = first_passing("I4");
    let out = mutate("I4", &target, |cdx| {
        // Drop one of the subject's direct edges on the CycloneDX side
        // only, so the two formats disagree.
        let subject = subject_ref(cdx);
        let deps = cdx["dependencies"].as_array_mut().expect("dependencies");
        let entry = deps
            .iter_mut()
            .find(|d| d["ref"].as_str() == Some(subject.as_str()))
            .expect("subject has a dependencies entry");
        let list = entry["dependsOn"].as_array_mut().expect("dependsOn");
        assert!(!list.is_empty(), "subject must have a direct dependency to drop");
        list.remove(0);
    });
    assert_fails_with(&out, checks::root_edge_parity::KIND_ONLY_IN_SPDX);
}

#[test]
fn i5_can_be_made_to_fail() {
    let target = first_passing("I5");
    let out = mutate("I5", &target, |cdx| {
        // Add a component nothing reaches, while the verdict still says
        // `complete`.
        cdx["components"].as_array_mut().expect("components").push(serde_json::json!({
            "bom-ref": "pkg:generic/mutation-unreachable@1",
            "name": "mutation-unreachable",
            "version": "1",
            "type": "library",
            "purl": "pkg:generic/mutation-unreachable@1"
        }));
    });
    assert_fails_with(&out, checks::completeness_honesty::KIND_COMPLETE_WITH_UNREACHABLE);
}

#[test]
fn i6_can_be_made_to_fail() {
    let target = first_passing("I6");
    let out = mutate("I6", &target, |cdx| {
        // One package, two different spellings of "we could not resolve a
        // version". Both components are added by the mutation so the
        // perturbation does not depend on which sentinel the golden
        // happens to carry — an earlier version of this test picked an
        // existing `v0.0.0-unknown` component and added a second
        // `v0.0.0-unknown`, which is not a mixed spelling and so
        // correctly did not fail.
        let arr = cdx["components"].as_array_mut().expect("components");
        arr.push(serde_json::json!({
            "bom-ref": "pkg:generic/mutation-two-spellings@a",
            "name": "mutation-two-spellings",
            "type": "library",
            "version": "v0.0.0-unknown"
        }));
        arr.push(serde_json::json!({
            "bom-ref": "pkg:generic/mutation-two-spellings@b",
            "name": "mutation-two-spellings",
            "type": "library",
            "version": "unknown"
        }));
    });
    assert_fails_with(&out, checks::version_sentinels::KIND_MIXED_UNKNOWN_VERSION_SPELLING);
}

/// The counterpart to the mutation: a Go pseudo-version is **not** a
/// sentinel. A substring match on `0.0.0` would flag roughly forty
/// innocent components across the corpus, and a gate that reddens on
/// innocent data is the gate that gets switched off.
#[test]
fn i6_does_not_flag_go_pseudo_versions_or_dotted_zero_versions() {
    let target = first_passing("I6");
    let out = mutate("I6", &target, |cdx| {
        let arr = cdx["components"].as_array_mut().expect("components");
        arr.push(serde_json::json!({
            "bom-ref": "pkg:golang/example.com/m@v0.0.0-20260928230214-8a89bd6388cc",
            "name": "mutation-pseudo",
            "type": "library",
            "version": "v0.0.0-20260928230214-8a89bd6388cc"
        }));
        arr.push(serde_json::json!({
            "bom-ref": "pkg:nuget/mutation-pseudo@0.0.0.0",
            "name": "mutation-pseudo",
            "type": "library",
            "version": "0.0.0.0"
        }));
    });
    assert_eq!(
        out,
        CellOutcome::Pass,
        "a Go pseudo-version and a .NET four-part version are real versions, not sentinels"
    );
}

#[test]
fn i7_can_be_made_to_fail() {
    let target = first_passing("I7");
    let out = mutate("I7", &target, |cdx| {
        // A component with no edge and nothing explaining why.
        cdx["components"].as_array_mut().expect("components").push(serde_json::json!({
            "bom-ref": "pkg:generic/mutation-orphan@1",
            "name": "mutation-orphan",
            "version": "1",
            "type": "library",
            "purl": "pkg:generic/mutation-orphan@1"
        }));
    });
    assert_fails_with(&out, checks::orphan_explained::KIND_UNREACHABLE_AND_UNEXPLAINED);
}

/// SC-007 — a cell moving from pass to fail without a declaration fails
/// the corpus lane, naming the invariant, the target and the
/// observation. This is the regression path, verified deliberately.
#[test]
fn an_undeclared_regression_fails_the_lane_and_names_the_cell() {
    let target = first_passing("I7");
    let mut sboms = goldens::load(&target).expect("golden");
    sboms.cdx["components"].as_array_mut().expect("components").push(serde_json::json!({
        "bom-ref": "pkg:generic/mutation-orphan@1",
        "name": "mutation-orphan",
        "version": "1",
        "type": "library",
        "purl": "pkg:generic/mutation-orphan@1"
    }));
    let cells = super::evaluate_matrix(&[(target.clone(), sboms)]);
    let decls = super::declarations::load();
    let rep = super::report::build(cells, &decls);
    let failures = rep.lane_failures();
    let named = failures.iter().find(|f| f.starts_with("I7 /"));
    let named = named.unwrap_or_else(|| {
        panic!("an undeclared I7 regression must fail the lane; got {failures:?}")
    });
    assert!(named.contains(&target), "the failure must name the target: {named}");
    assert!(
        named.contains("unreachable-and-unexplained"),
        "the failure must name the observation: {named}"
    );
}

/// SC-003 — the per-ecosystem reach of a change is readable from a diff
/// of two runs' matrix artifacts, with no document inspection.
///
/// A mutation is the inverse of a fix and exercises the same diff, so
/// the criterion is demonstrated here rather than waiting on one of
/// #1206-#1211 (spec Assumption 1 puts those fixes out of scope).
/// Writes both artifacts so the diff is reproducible by hand:
///
/// ```sh
/// diff <(jq -S '.cells' target/invariants/run-goldens.json) \
///      <(jq -S '.cells' target/invariants/run-goldens-mutated.json)
/// ```
#[test]
fn a_matrix_diff_shows_which_targets_a_change_reached() {
    let decls = super::declarations::load();

    let baseline = super::evaluate_matrix(&goldens::load_all());
    let base_report = super::report::build(baseline, &decls);
    let base_path = base_report.write_artifact("goldens").expect("write baseline artifact");

    // Perturb exactly two targets, leaving the other fifteen alone.
    let touched = ["go-cobra", "npm-express"];
    let mut documents = goldens::load_all();
    for (name, sboms) in documents.iter_mut() {
        if touched.contains(&name.as_str()) {
            sboms.cdx["components"].as_array_mut().expect("components").push(serde_json::json!({
                "bom-ref": "pkg:generic/mutation-orphan@1",
                "name": "mutation-orphan",
                "version": "1",
                "type": "library",
                "purl": "pkg:generic/mutation-orphan@1"
            }));
        }
    }
    let mutated = super::evaluate_matrix(&documents);
    let mut_report = super::report::build(mutated, &decls);
    let mut_path = mut_report.write_artifact("goldens-mutated").expect("write mutated artifact");

    // The diff: cells whose state changed.
    let mut moved: Vec<String> = Vec::new();
    for b in &base_report.cells {
        let m = mut_report
            .cells
            .iter()
            .find(|c| c.invariant == b.invariant && c.target == b.target)
            .expect("same cell set in both runs");
        if std::mem::discriminant(&b.outcome) != std::mem::discriminant(&m.outcome) {
            moved.push(format!("{}/{}", b.invariant, b.target));
        }
    }
    moved.sort();

    assert_eq!(
        moved,
        vec!["I7/go-cobra".to_string(), "I7/npm-express".to_string()],
        "the diff must name exactly the cells the change reached, and nothing else.\n\
         baseline: {}\nmutated:  {}",
        base_path.display(),
        mut_path.display()
    );
}
