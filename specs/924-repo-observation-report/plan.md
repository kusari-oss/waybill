# Implementation Plan: Repo Observation Report

**Branch**: `924-repo-observation-report` | **Date**: 2026-09-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/924-repo-observation-report/spec.md` · Issue #932

## Summary

Emit a versioned, machine-readable account of what waybill saw, claimed,
ignored, and **could not determine** when traversing a repository. The primary
consumers are an operator asking "is my project shape supported?" and a
maintainer handed a report for a repository they cannot see.

The technical approach is almost entirely *surfacing*, not computing.
`dispatch_file` already returns the set of readers that claimed each file and
the walker already binds that value one line before handing it to a metrics
sink (research R1). The census is that value, retained. On top of it sit three
enrichments: marker-driven ecosystem attribution, typed uncertainty, and a
published schema.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly.
`waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (report
construction), `globset` (already the reader-registry match engine),
`std::str::from_utf8` (R4 content sniffing), `tracing`, `anyhow`. Dev/test:
existing `jsonschema = "0.46"` (R7), `tempfile`. **Zero new Cargo
dependencies** at any level.
**Storage**: None. The report is written to an operator-named path; no cache,
no persistence, no state carried between runs.
**Testing**: `cargo +stable test --workspace`; schema conformance via the
existing `jsonschema` dev-dep; this repository as the primary fixture (SC-001).
**Target Platform**: Every platform waybill builds for. Nothing
platform-specific — filesystem traversal and JSON.
**Project Type**: CLI subcommand over the existing single-pass walker.
**Performance Goals**: None stated, deliberately. The report reuses a
traversal already being paid for (R1), so a latency target here would be an
unmeasured number of exactly the kind this project's CLAUDE.md prohibits.
SC-008 bounds report **size** instead, which is measurable today.
**Constraints**: Offline by construction (FR-022 / R5). Deterministic
(FR-020 — free per R2). Bounded in size independent of repository size
(FR-021 — threshold 25 per R3). Must not alter emitted SBOM content (FR-024).
**Scale/Scope**: Validated against repositories from 3 to 5,118 directories
(R3). At the top of that range the report holds 393 records, 7.7% of
directories walked.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1.*

| Principle | Assessment | Verdict |
|---|---|---|
| **I. Pure Rust, Statically Linked** | Zero new dependencies (R1–R7). Content sniffing is `std::str::from_utf8` over an 8 KiB sample rather than a detection crate. | **PASS** |
| **II. eBPF-Only Observation** | Not engaged — no kernel-side work; `waybill-ebpf` untouched. | **N/A** |
| **III. Fail Closed** | A directory that cannot be read is recorded as skipped-with-reason, never silently dropped (spec Edge Cases). Unreadable input degrades the report's completeness *visibly* — which is the feature's whole subject. | **PASS** |
| **IV. Type-Driven Correctness** | Claim status is an exclusive enumeration (FR-012a); ambiguity is a distinct optional structure (FR-012b). The two-field split exists precisely so the type system cannot express "claimed, therefore unambiguous". | **PASS** |
| **V. Specification Compliance** | Standards-native audit is recorded **in the spec's Functional Requirements** (FR-025), not only here: no CycloneDX or SPDX construct models a tool's traversal or its own uncertainty; SARIF considered and rejected as finding-shaped rather than census-shaped. No new `waybill:*` SBOM property is introduced (FR-024). | **PASS** |
| **VI. Three-Crate Architecture** | Entirely within `waybill-cli`. No change to `waybill-common` or `waybill-ebpf`. | **PASS** |
| **VII. Test Isolation** | Fixtures are per-test temp directories plus this repository read-only. No env-var mutation, so no `EnvGuard` serialisation needed. | **PASS** |
| **VIII. Completeness** | The census must reconcile exactly — walked = claimed + unclaimed + skipped (FR-003), with an explicit test (SC-003). A report that does not reconcile is defined as invalid rather than merely imperfect. | **PASS** |
| **IX. Accuracy** | **Deliberate tension, resolved in favour of the principle.** Elsewhere Accuracy means asserting nothing unsupported; here the *uncertainty itself* is the payload. FR-014 keeps the principle intact by forbidding any classification the evidence does not support — the feature records ambiguity rather than resolving it, which is Accuracy applied to a report rather than relaxed for one. | **PASS** |
| **X. Transparency** | This feature is Principle X made into an artifact: it exists to make waybill's ignorance legible. | **PASS** |
| **XI. Enrichment** | Not engaged — the report path never invokes enrichment (R5). | **N/A** |
| **XII. External Data Source Enrichment** | Not engaged — no external data source is consulted; FR-022 forbids network access. | **N/A** |

**Gate result: PASS.** No violations; the Complexity Tracking table is
therefore omitted.

One item is recorded as a **qualification rather than a violation**: FR-022b
says the offline guarantee is "structural rather than flag-dependent". Per R5
that is exactly true of enrichment (the path never reaches it) and true *in
effect* of resolution (offline is hard-coded with no operator-facing switch),
but resolution does contain network-capable code. The plan states this plainly
rather than letting the spec's wording imply something stronger.

### Post-Design Re-Evaluation (after Phase 1)

Re-run against the completed data model and contract. **Still PASS**, with two
things the design surfaced that the pre-Phase-0 pass could not have seen:

- **Principle IV strengthened by the design, not merely satisfied.** Splitting
  `claim_status` (exclusive) from `ambiguity` (optional, independent) means the
  type system cannot express "claimed, therefore unambiguous". The pre-design
  gate recorded this as an intention; the data model makes it structural.
- **Principle VIII gained a checkable form.** C-4 states the reconciliation
  invariant in a way a consumer can verify *without access to the repository
  being described*. Completeness that can only be checked by the producer is a
  weaker property than one a recipient can check, and this feature's whole
  purpose is that recipients check things.

No new violations. Complexity Tracking remains omitted.

## Project Structure

### Documentation (this feature)

```text
specs/924-repo-observation-report/
├── plan.md              # This file
├── research.md          # Phase 0 — R1..R7, with measurements
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   └── report-schema.md # Phase 1 — the report contract
├── checklists/
│   └── requirements.md  # From /speckit.specify
└── tasks.md             # Phase 2 (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/
├── report/                          # NEW — the feature
│   ├── mod.rs                       # assembly + emission entry point
│   ├── census.rs                    # claimed/unclaimed accumulation (R1)
│   ├── significance.rs              # FR-021a record-or-aggregate decision (R3)
│   ├── content_kind.rs              # FR-011 binary-vs-text sampling (R4)
│   ├── ecosystems.rs                # FR-007 marker table + lookup (R6)
│   ├── ecosystems.data              # FR-009 the table itself — data, not code
│   └── schema.rs                    # FR-016/FR-017 version + serialisation
├── scan_fs/walk_registry/
│   ├── walker.rs                    # MODIFIED — retain `dispatched_to` (R1)
│   └── perf_metrics.rs              # MODIFIED — census sink alongside counters
└── cli/
    └── report_cmd.rs                # NEW — subcommand surface

waybill-cli/tests/
├── repo_report_census.rs            # US1 — reconciliation (SC-003, SC-013)
├── repo_report_ecosystems.rs        # US2 — naming + anti-staleness (R6)
├── repo_report_ambiguity.rs         # US3 — self-test on this repo (SC-001)
└── repo_report_schema.rs            # US4 — schema, redaction, determinism

waybill-cli/tests/fixtures/repo_report/
    └── …                            # synthetic repos for the US2/US3 shapes
```

**Structure Decision**: A new `waybill-cli/src/report/` module, mirroring the
existing sibling-module convention (`enrich/`, `generate/`, `parity/`). The
only edits outside it are two lines of plumbing in `walk_registry` to retain a
value the walker already computes — deliberately minimal, because that file is
on the hot path of every scan and is covered by byte-identity guarantees for
21 migrated readers.

The ecosystem table ships as a **data file** alongside its module, satisfying
FR-009's "editable without code changes" and keeping the staleness guard (R6)
a pure data-vs-registry comparison.

## Phase Plan

**Phase 0 — Research**: complete. See [research.md](./research.md). All seven
unknowns resolved empirically; zero new dependencies; one spec qualification
raised (FR-022b, see above).

**Phase 1 — Design & Contracts**: [data-model.md](./data-model.md),
[contracts/report-schema.md](./contracts/report-schema.md),
[quickstart.md](./quickstart.md).

**Phase 2 — Tasks**: produced by `/speckit.tasks`, not here.

### Delivery shape

US1 is a standalone MVP. The census alone — which readers claimed what, and
does it reconcile — answers the feature's primary goal, and US2/US3/US4 each
enrich a report that already exists rather than completing a partial one.

Recommended ordering, each independently shippable:

1. **US1** — census + reconciliation. The MVP.
2. **US2** — ecosystem attribution + the anti-staleness test.
3. **US3** — typed uncertainty, self-tested against this repository.
4. **US4** — schema publication, redaction mode, determinism guarantees.

US4 last is safe only because FR-019's unconditional guarantees (no absolute
paths, no file contents) are part of the report's construction from US1
onward. What US4 adds is the *opt-in stricter mode* and the published schema,
not the baseline safety.
