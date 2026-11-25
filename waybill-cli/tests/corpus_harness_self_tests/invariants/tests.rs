#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet};
use crate::corpus_harness_195::invariants::*;
use crate::corpus_harness_195::harness::{EmittedPaths, EmittedSboms};

fn sboms(cdx: serde_json::Value) -> EmittedSboms {
    EmittedSboms {
        cdx,
        spdx_2_3: serde_json::json!({}),
        spdx_3: serde_json::json!({}),
        paths: EmittedPaths {
            cdx: "/dev/null".into(),
            spdx_2_3: "/dev/null".into(),
            spdx_3: "/dev/null".into(),
        },
    }
}

/// The matrix over the committed goldens, printed. This is the
/// hermetic measurement T032 of milestone 1155 asked for — it needs
/// no network and no container pulls, because the golden triples
/// carry everything the invariants read.
#[test]
fn golden_matrix_is_evaluable_and_gated() {
    // #1238 — stand down in regen mode, the way the sibling gates at
    // `harness::update_goldens_gate` call sites already do
    // (`manifest.rs::every_target_has_committed_goldens`,
    // `layer2_golden.rs`).
    //
    // This gate reads the COMMITTED goldens. In regen mode those are
    // stale by definition — replacing them is the entire point of the
    // run — so without this guard the gate measures the thing the run
    // exists to change, and any change that moves a cell deadlocks:
    //
    //   declarations left as-is  → this gate passes, the live lane
    //                              fails with StaleDeclaration
    //   declarations updated     → the live lane passes, this gate
    //                              fails with UndeclaredFailure
    //
    // No declaration state satisfies both, so no regen run can be
    // green, so the workflow's upload step — which is ANDed with an
    // implicit `success()` — never fires, so the goldens can never be
    // installed and the two can never be made to agree. Feature 1218
    // hit this and could not refresh its goldens at all.
    //
    // Verify runs (the PR lane, nightly, and every local
    // `cargo test --workspace`) are unaffected and remain the place
    // this gate does its work.
    if crate::corpus_harness_195::harness::update_goldens_gate() {
        eprintln!(
            "#1238 guard skipped: regen mode is replacing the committed \
             goldens this gate reads, so evaluating them would gate the \
             run against the state it exists to change. Fires on every \
             verify run."
        );
        return;
    }
    let documents = goldens::load_all();
    assert_eq!(
        documents.len(),
        crate::corpus_harness_195::manifest::TARGETS.len() - goldens::GOLDENS_ARE_FILTERED.len(),
        "every corpus target whose goldens are unfiltered should have them committed;              the filtered ones are the corpus lane's business (see goldens::GOLDENS_ARE_FILTERED)"
    );
    let cells = evaluate_matrix(&documents);
    assert_eq!(cells.len(), REGISTRY.len() * documents.len());

    let decls = declarations::load();
    let errors = declarations::validate(&decls);
    assert!(errors.is_empty(), "declarations invalid: {errors:?}");

    let rep = report::build(cells, &decls);
    // Printed on failure, and on success under --nocapture.
    println!("{}", rep.console());
    let _ = rep.write_artifact("goldens");

    let failures = rep.lane_failures();
    assert!(
        failures.is_empty(),
        "{} undeclared or stale cell(s) in the golden matrix:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// FR-004 — a run reports every failing cell, not just the first.
/// The existing `Result<(), AssertionFailure>` shape cannot express
/// this, which is why the layer has its own outcome type.
#[test]
fn the_driver_does_not_short_circuit() {
    // Two documents that both fail I2, via a duplicated identity.
    let dup = serde_json::json!({
        "components": [
            {"name": "a", "version": "1", "type": "library", "purl": "pkg:x/a@1"},
            {"name": "a", "version": "1", "type": "library", "purl": "pkg:x/a@1"}
        ]
    });
    let docs = vec![("one".to_string(), sboms(dup.clone())), ("two".to_string(), sboms(dup))];
    let cells = evaluate_matrix(&docs);
    let i2_fails: Vec<&Cell> = cells
        .iter()
        .filter(|c| c.invariant == "I2" && matches!(c.outcome, CellOutcome::Fail(_)))
        .collect();
    assert_eq!(i2_fails.len(), 2, "both failing cells must be reported, got {i2_fails:?}");
}

/// FR-007 — a panicking checker yields `Error` for its own cell and
/// every other cell still evaluates.
#[test]
fn a_panicking_checker_does_not_stop_the_run() {
    fn boom(_: &EmittedSboms) -> CellOutcome {
        panic!("deliberate checker panic");
    }
    let inv = Invariant {
        id: "I999",
        statement: "never true",
        precondition: "always",
        kinds: &["never"],
        checker: boom,
    };
    let doc = sboms(serde_json::json!({"components": []}));
    // Silence the panic hook so the deliberate panic does not look
    // like a test failure in the log.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let cell = evaluate_cell(&inv, "t", &doc);
    std::panic::set_hook(prev);
    match cell.outcome {
        CellOutcome::Error { ref message } => {
            assert!(message.contains("deliberate"), "{message}")
        }
        other => panic!("expected Error, got {other:?}"),
    }
    // And the registry's own cells still evaluate afterwards.
    let cells = evaluate_matrix(&[("t".to_string(), doc)]);
    assert_eq!(cells.len(), REGISTRY.len());
}

/// SC-009 — no invariant reports `Pass` on a document whose
/// precondition for it is unmet. An empty document meets no
/// precondition, so every cell must be `NotApplicable`.
#[test]
fn an_empty_document_is_not_applicable_everywhere_never_passing() {
    let doc = sboms(serde_json::json!({"components": [], "dependencies": []}));
    for inv in REGISTRY {
        let cell = evaluate_cell(inv, "empty", &doc);
        assert!(
            matches!(cell.outcome, CellOutcome::NotApplicable { .. }),
            "{} reported {:?} on an empty document; a pass here would be vacuous",
            inv.id,
            cell.outcome
        );
    }
}

// ---------------- the verdict state machine (FR-009/011/012) -------

fn decl(kind: &str) -> declarations::Declaration {
    declarations::Declaration {
        invariant: "I1".to_string(),
        target: "t".to_string(),
        kind: kind.to_string(),
        count_at_declaration: 7,
        reason: "because".to_string(),
        issue: Some(1206),
    }
}

fn fail_with(kinds: &[&'static str], count: usize) -> CellOutcome {
    CellOutcome::Fail(
        kinds
            .iter()
            .map(|k| Violation { kind: k, count, example: Some("e".to_string()) })
            .collect(),
    )
}

#[test]
fn an_undeclared_failure_fails_the_lane() {
    assert_eq!(
        verdict(&fail_with(&["file-tier-dependency-edge"], 3), None),
        Verdict::UndeclaredFailure
    );
}

/// FR-012 — the kind is pinned, the count drifts. A count that moved
/// from 7 to 4000 is still the accepted defect.
#[test]
fn a_drifted_count_is_accepted_but_a_changed_kind_is_not() {
    let d = decl("file-tier-dependency-edge");
    assert_eq!(
        verdict(&fail_with(&["file-tier-dependency-edge"], 4000), Some(&d)),
        Verdict::AcceptedKnownFailure
    );
    let changed = verdict(&fail_with(&["duplicate-library-identity"], 7), Some(&d));
    assert!(matches!(changed, Verdict::UndeclaredViolationKind(_)), "{changed:?}");
}

#[test]
fn a_second_kind_under_an_existing_declaration_fails() {
    let d = decl("file-tier-dependency-edge");
    let v = verdict(
        &fail_with(&["file-tier-dependency-edge", "duplicate-library-identity"], 1),
        Some(&d),
    );
    match v {
        Verdict::UndeclaredViolationKind(ks) => {
            assert!(ks.contains("duplicate-library-identity"), "{ks:?}")
        }
        other => panic!("expected UndeclaredViolationKind, got {other:?}"),
    }
}

/// FR-011 — a declaration for a cell that now passes is stale, in
/// both of the ways a cell can stop failing.
#[test]
fn a_stale_declaration_fails_the_lane() {
    let d = decl("file-tier-dependency-edge");
    assert_eq!(verdict(&CellOutcome::Pass, Some(&d)), Verdict::StaleDeclaration);
    assert_eq!(
        verdict(&CellOutcome::NotApplicable { reason: "x".into() }, Some(&d)),
        Verdict::StaleDeclaration
    );
}

#[test]
fn an_error_is_never_accepted_even_when_declared() {
    let d = decl("file-tier-dependency-edge");
    let v = verdict(&CellOutcome::Error { message: "boom".into() }, Some(&d));
    assert!(v.fails_lane(), "{v:?}");
}

#[test]
fn passing_and_not_applicable_cells_without_declarations_are_fine() {
    assert_eq!(verdict(&CellOutcome::Pass, None), Verdict::Ok);
    assert_eq!(
        verdict(&CellOutcome::NotApplicable { reason: "x".into() }, None),
        Verdict::OkNotApplicable
    );
}

/// SC-010 / FR-006a — an invariant that is not applicable on every
/// target fails the lane. Its row of dashes is indistinguishable
    /// from a working check.
#[test]
fn an_invariant_applicable_nowhere_fails_the_lane() {
    let doc = sboms(serde_json::json!({"components": [], "dependencies": []}));
    let cells = evaluate_matrix(&[("only".to_string(), doc)]);
    let unfalsifiable = unfalsifiable_invariants(&cells);
    assert_eq!(
        unfalsifiable.len(),
        REGISTRY.len(),
        "on an empty corpus every invariant is unfalsifiable: {unfalsifiable:?}"
    );
}

/// FR-013 — a new target is evaluated against every invariant and
/// cannot inherit an exemption. A target with no declaration whose
/// document fails is an undeclared failure, full stop.
#[test]
fn a_new_target_cannot_inherit_an_exemption() {
    let d = decl("file-tier-dependency-edge"); // declares target "t"
    let file = serde_json::json!({
        "metadata": {"component": {"bom-ref": "root", "purl": "pkg:generic/root@1"}},
        "components": [
            {"name": "f", "version": "1", "type": "file", "bom-ref": "fileref",
             "properties": [{"name": "waybill:component-tier", "value": "file"}]}
        ],
        "dependencies": [{"ref": "root", "dependsOn": ["fileref"]}]
    });
    let out = (checks::file_tier_edges::check)(&sboms(file));
    assert!(matches!(out, CellOutcome::Fail(_)), "{out:?}");
    // The declaration names target "t"; a different target gets
    // nothing from it.
    let decls =
        declarations::DeclarationsFile { declaration: vec![d] };
    assert!(decls.find("I1", "brand-new-target").is_none());
    assert_eq!(verdict(&out, decls.find("I1", "brand-new-target")), Verdict::UndeclaredFailure);
}
