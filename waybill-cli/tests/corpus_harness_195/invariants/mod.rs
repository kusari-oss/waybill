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
                    carries a non-runtime marker, unless the compiler supplies it",
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
                    or containment carries a marker explaining why",
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
    Invariant {
        id: "I9",
        statement: "A component the compiler supplies carries no non-runtime scope",
        precondition: "at least one component is marked compiler-supplied",
        kinds: checks::compiler_supplied_scope::KINDS,
        checker: checks::compiler_supplied_scope::check,
    },
    Invariant {
        id: "I10",
        statement: "A closure member's non-runtime marker agrees with the evidence that decided it, \
                    or the member says it was exempted",
        // Reports n/a rather than passing vacuously on the 16 corpus targets
        // that emit no closure roles at all. A row that passes where it
        // cannot apply is indistinguishable from one that passes where it
        // can.
        precondition: "at least one component carries a closure role",
        kinds: checks::closure_role_scope::KINDS,
        checker: checks::closure_role_scope::check,
    },
    Invariant {
        id: "I11",
        statement: "A component the closure resolved is not `design`, and the root does not \
                    depend only on `design` components",
        // n/a, not a vacuous pass, on every document without a closure
        // member: there `design` is correct by definition.
        precondition: "at least one component is a closure member (carries C214)",
        kinds: checks::closure_resolved_tier::KINDS,
        checker: checks::closure_resolved_tier::check,
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
