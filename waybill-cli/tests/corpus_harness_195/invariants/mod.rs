//! The universal semantic-invariant layer — milestone 1155.
//!
//! The harness had three layers and this is the missing fourth. Layer 0
//! (`expectations::layer0_document_integrity`) asks whether a document
//! is internally consistent, universally. The per-target expectations
//! ask whether one specific target looks the way we expect. Layer 2
//! diffs goldens. Nothing asked whether the graph a document describes
//! is *meaningful*, universally — so every semantic check lived in one
//! of 17 per-target functions and a defect class was checked only where
//! someone noticed it.
//!
//! An **invariant** is a property expected of any emitted SBOM. An
//! **expectation** is a claim about one specific target. The two words
//! are load-bearing since the rename (FR-020); if you cannot tell which
//! one a check is from its name, that is a bug (FR-020a).
//!
//! See `specs/1155-invariant-major-corpus/` and the catalog at
//! `docs/reference/sbom-invariants.md`.

use std::collections::{BTreeMap, BTreeSet};

use super::harness::EmittedSboms;

pub mod catalog;
pub mod checks;
pub mod declarations;
pub mod goldens;
#[cfg(test)]
mod line_stability;
#[cfg(test)]
mod mutation_tests;
#[cfg(test)]
mod no_new_io;
#[cfg(test)]
mod terminology;
pub mod report;

// -----------------------------------------------------------------------
// Cell outcome — data-model.md "CellOutcome"
// -----------------------------------------------------------------------

/// One violation of an invariant, classified by `kind`.
///
/// `kind` is the pinned part of a declaration (FR-012); `count` is
/// recorded for context and is deliberately **not** a gate condition,
/// because counts drift for reasons unrelated to waybill — an upstream
/// base-image bump, a corpus repin — and a gate that reddens on
/// innocent drift is the gate that gets switched off. That is the
/// history milestone 117 had to undo for `walk.audit-allowlist.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: &'static str,
    pub count: usize,
    pub example: Option<String>,
}

/// The state of one (invariant, target) cell.
///
/// The reason this is an enum and not a `Result` is FR-003: a cell whose
/// precondition is unmet is **not** a pass. Collapsing the two is the
/// vacuity trap this whole layer exists to close — a check that silently
/// applies to nothing reads exactly like a check that applies and
/// succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellOutcome {
    Pass,
    Fail(Vec<Violation>),
    NotApplicable { reason: String },
    Error { message: String },
}

impl CellOutcome {
    /// The kinds observed, empty unless this is a `Fail`.
    pub fn kinds(&self) -> BTreeSet<&'static str> {
        match self {
            CellOutcome::Fail(vs) => vs.iter().map(|v| v.kind).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub fn is_applicable(&self) -> bool {
        !matches!(self, CellOutcome::NotApplicable { .. })
    }

    /// One-character grid glyph for the console report.
    pub fn glyph(&self) -> char {
        match self {
            CellOutcome::Pass => '.',
            CellOutcome::Fail(_) => 'X',
            CellOutcome::NotApplicable { .. } => '-',
            CellOutcome::Error { .. } => 'E',
        }
    }
}

// -----------------------------------------------------------------------
// Invariant + registry — data-model.md "Invariant"
// -----------------------------------------------------------------------

/// One catalogued invariant. `id` is its durable identity: refining the
/// statement keeps it, changing its meaning retires it (FR-001a/b/c).
pub struct Invariant {
    /// `"I1"`, `"I2"`, … — must match a catalog row.
    pub id: &'static str,
    pub statement: &'static str,
    /// What must be true of a document for the invariant to be
    /// meaningful. A cell whose precondition is unmet reports
    /// `NotApplicable`, never `Pass` (FR-006).
    pub precondition: &'static str,
    /// Every kind this checker can emit. The gate asserts this agrees
    /// with the catalog row in both directions (FR-012a): a checker able
    /// to emit a kind the catalog never listed means a reviewer
    /// approving a declaration cannot know what they are accepting.
    pub kinds: &'static [&'static str],
    pub checker: fn(&EmittedSboms) -> CellOutcome,
}

/// The machine side of `docs/reference/sbom-invariants.md`. Adding an
/// invariant is a line here, a file in `checks/`, and a catalog row.
pub static REGISTRY: &[Invariant] = &[
    Invariant {
        id: "I1",
        statement: "No dependency edge targets a file-tier component",
        precondition: "document contains at least one file-tier component",
        kinds: checks::file_tier_edges::KINDS,
        checker: checks::file_tier_edges::check,
    },
    Invariant {
        id: "I2",
        statement: "`(name, version, type)` identifies at most one component",
        precondition: "at least two components",
        kinds: checks::identity_unique::KINDS,
        checker: checks::identity_unique::check,
    },
    Invariant {
        id: "I3",
        statement: "A component reachable from the subject only through non-runtime edges \
                    carries a non-runtime marker",
        precondition: "the document declares at least one non-runtime dependency edge",
        kinds: checks::scope_propagation::KINDS,
        checker: checks::scope_propagation::check,
    },
    Invariant {
        id: "I4",
        statement: "CycloneDX and SPDX 2.3 agree on the subject's direct dependency set",
        precondition: "both documents emitted, each with a resolvable subject",
        kinds: checks::root_edge_parity::KINDS,
        checker: checks::root_edge_parity::check,
    },
    Invariant {
        id: "I5",
        statement: "A `complete` graph-completeness verdict implies nothing was excluded \
                    from the judgement",
        precondition: "document-scope completeness verdict is `complete`",
        kinds: checks::completeness_honesty::KINDS,
        checker: checks::completeness_honesty::check,
    },
    Invariant {
        id: "I6",
        statement: "A package does not appear under several different unknown-version spellings",
        precondition: "at least one component carries an unknown-version sentinel",
        kinds: checks::version_sentinels::KINDS,
        checker: checks::version_sentinels::check,
    },
    Invariant {
        id: "I7",
        statement: "A component not reachable from the subject by any dependency edge \
                    carries a marker explaining why",
        precondition: "subject resolvable and at least one non-subject component",
        kinds: checks::orphan_explained::KINDS,
        checker: checks::orphan_explained::check,
    },
    Invariant {
        id: "I8",
        statement: "A component with no version carries an annotation explaining why \
                    it has none",
        precondition: "at least one component has an absent or empty version",
        kinds: checks::versionless_explained::KINDS,
        checker: checks::versionless_explained::check,
    },
];

pub fn invariant(id: &str) -> Option<&'static Invariant> {
    REGISTRY.iter().find(|i| i.id == id)
}

// -----------------------------------------------------------------------
// Evaluation driver — FR-002, FR-004, FR-007
// -----------------------------------------------------------------------

/// One evaluated cell.
#[derive(Debug, Clone)]
pub struct Cell {
    pub invariant: &'static str,
    pub target: String,
    pub outcome: CellOutcome,
}

/// Evaluate one invariant against one document.
///
/// FR-007: a panicking checker yields `Error` for its own cell and must
/// not prevent the remaining cells from evaluating. `catch_unwind` is
/// sound here because no profile sets `panic = "abort"` — verified
/// across every `[profile.*]` in the workspace.
pub fn evaluate_cell(inv: &Invariant, target: &str, sboms: &EmittedSboms) -> Cell {
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (inv.checker)(sboms)
    })) {
        Ok(o) => o,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "checker panicked with a non-string payload".to_string());
            CellOutcome::Error { message }
        }
    };
    Cell { invariant: inv.id, target: target.to_string(), outcome }
}

/// Evaluate every registry invariant against every supplied document,
/// collecting all outcomes rather than stopping at the first failure
/// (FR-004 — the existing `Result<(), AssertionFailure>` shape reports
/// one failure per target and so cannot express a matrix).
///
/// Cells come back sorted by `(invariant, target)` so two runs diff
/// cleanly, which is what makes SC-003 a diff rather than an
/// investigation.
pub fn evaluate_matrix(documents: &[(String, EmittedSboms)]) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::with_capacity(REGISTRY.len() * documents.len());
    for (name, sboms) in documents {
        cells.extend(evaluate_target(name, sboms));
    }
    cells.sort_by(|a, b| (a.invariant, &a.target).cmp(&(b.invariant, &b.target)));
    cells
}

/// Evaluate every registry invariant against one already-emitted
/// document set, without taking ownership of it.
///
/// The lane needs this rather than `evaluate_matrix`: the documents are
/// borrowed from the caller, which still has to hand them to layer 2.
/// An owning signature there would force either a clone of three parsed
/// JSON documents or a second scan — and a second scan is exactly what
/// FR-016 forbids.
pub fn evaluate_target(target: &str, sboms: &EmittedSboms) -> Vec<Cell> {
    let mut cells: Vec<Cell> = REGISTRY.iter().map(|inv| evaluate_cell(inv, target, sboms)).collect();
    cells.sort_by(|a, b| (a.invariant, &a.target).cmp(&(b.invariant, &b.target)));
    cells
}

/// Per-invariant count of targets whose precondition was met (FR-015a).
///
/// Published because an invariant satisfying FR-006a only marginally —
/// applicable on 1 of 17 — passes the gate while being nearly
/// decoration, and a bare `pass` row cannot be told from a broad one.
pub fn applicability(cells: &[Cell]) -> BTreeMap<&'static str, usize> {
    let mut out: BTreeMap<&'static str, usize> = REGISTRY.iter().map(|i| (i.id, 0)).collect();
    for c in cells {
        if c.outcome.is_applicable() {
            *out.entry(c.invariant).or_insert(0) += 1;
        }
    }
    out
}

// -----------------------------------------------------------------------
// Cell verdict — the state machine in data-model.md
// -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    OkNotApplicable,
    AcceptedKnownFailure,
    /// A declaration for a cell that no longer fails (FR-011).
    StaleDeclaration,
    UndeclaredFailure,
    /// `Fail` carrying a kind the declaration does not name (FR-012).
    UndeclaredViolationKind(BTreeSet<&'static str>),
    Error(String),
}

impl Verdict {
    pub fn fails_lane(&self) -> bool {
        !matches!(self, Verdict::Ok | Verdict::OkNotApplicable | Verdict::AcceptedKnownFailure)
    }
}

/// The complete per-cell gate behaviour — the eight-row table in
/// `data-model.md`. The kind is pinned; the count drifts.
pub fn verdict(outcome: &CellOutcome, declared: Option<&declarations::Declaration>) -> Verdict {
    match (outcome, declared) {
        (CellOutcome::Error { message }, _) => Verdict::Error(message.clone()),
        (CellOutcome::Pass, None) => Verdict::Ok,
        (CellOutcome::Pass, Some(_)) => Verdict::StaleDeclaration,
        (CellOutcome::NotApplicable { .. }, None) => Verdict::OkNotApplicable,
        (CellOutcome::NotApplicable { .. }, Some(_)) => Verdict::StaleDeclaration,
        (CellOutcome::Fail(_), None) => Verdict::UndeclaredFailure,
        (CellOutcome::Fail(_), Some(d)) => {
            let observed = outcome.kinds();
            let undeclared: BTreeSet<&'static str> =
                observed.iter().filter(|k| **k != d.kind.as_str()).copied().collect();
            if undeclared.is_empty() {
                // Count drift is accepted by design; only the kind set matters.
                Verdict::AcceptedKnownFailure
            } else {
                Verdict::UndeclaredViolationKind(undeclared)
            }
        }
    }
}

/// Invariants that are `NotApplicable` on every evaluated target
/// (FR-006a). Such an invariant is unfalsifiable against this corpus and
/// its row of dashes is indistinguishable from a working check, so the
/// lane fails rather than reporting a clean sweep.
pub fn unfalsifiable_invariants(cells: &[Cell]) -> Vec<&'static str> {
    applicability(cells)
        .into_iter()
        .filter(|(_, n)| *n == 0)
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use super::super::harness::{EmittedPaths, EmittedSboms};

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
        if super::super::harness::update_goldens_gate() {
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
            super::super::manifest::TARGETS.len() - goldens::GOLDENS_ARE_FILTERED.len(),
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
}
