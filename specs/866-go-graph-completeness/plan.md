# Implementation Plan: Go scans assert dependency edges they never read, and then report the result as complete

**Branch**: `866-go-graph-completeness` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/866-go-graph-completeness/spec.md`

## Summary

When the Go module graph cannot be resolved, the go.sum fallback appends
every unreached module to the main module's `depends`
(`golang/legacy.rs:~2036`). Those appended entries become `DependsOn`
relationships that no `go.mod` declares — 2 of 7 on `go-cobra`, 554 of
2984 on `kubernetes`. Because they attach every component to the graph,
the reachability check finds zero orphans, no reason code fires, and the
document declares itself `complete`.

The approach is to stop appending them, emit the provenance that is
already carried in memory but discarded at serialization, and let the
existing orphan machinery report the resulting gap. Removing the
invented edges is what makes the completeness signal correct; it is not
a separate fix (research R4, R5).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (relationship + annotation emission), `tracing`, `anyhow`. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan, matching every reader milestone since 002.
**Testing**: `cargo +stable clippy --workspace --all-targets` + `cargo +stable test --workspace` (the mandatory pre-PR pair), plus the two committed probes under `measurements/`, the public-corpus lane, and the m770 quality corpus.
**Target Platform**: Linux / macOS / Windows, user-space only.
**Project Type**: Rust CLI + library workspace (`waybill-cli`, `waybill-common`, `waybill-ebpf`, `xtask`).
**Performance Goals**: No measurable scan-time change expected for the scan itself — the edit removes work from a loop rather than adding any. The FR-009 build-time gate is the only new cost: it parses every `go.mod` in the scanned tree (39 for kubernetes, the in-corpus maximum). **No ceiling is set here, because none has been measured** — T047 establishes one against the 39-module target, and the budget is expressed as a ratio against the scan it accompanies rather than an absolute quoted from a guess.
**Constraints**: Offline-safe; no network introduced. No new subprocess calls. Emission must stay deterministic — edge ordering is already lex-sorted at `graph_resolver.rs:320` precisely because HashMap iteration order surfaced as SPDX ID drift in goldens.
**Scale/Scope**: Upper bound measured in-corpus is `kubernetes` at 824 components / 2984 golang edges / 39 `go.mod` files.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| **I. Pure Rust, Statically Linked** | PASS | Zero new dependencies at any layer; no linkage change. |
| **III. Fail Closed** | PASS | The FR-009 gate must fail the build on an unbacked edge, not warn. No `continue-on-error`. |
| **IV. Type-Driven Correctness** | PASS | No new `unwrap` in production paths; test modules carry the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard per the crate-root deny. |
| **V. Specification Compliance** | **PASS with obligation** | Native-field audit completed in research R3: SPDX 2.3 `relationships[].comment` and SPDX 3 `Relationship.comment` are native and MUST be used; CycloneDX 1.6 has no per-edge slot, so a parity-bridging `waybill:*` property is permitted **only** with a `docs/reference/sbom-format-mapping.md` row naming the missing native field, plus a matching `EXTRACTORS` entry in the same change. |
| **VIII. Completeness** | PASS | No component is removed — FR-007 keeps stranded components in the inventory and FR-007a keeps them distinguishable. Only unbacked *relationships* are withdrawn. See Complexity Tracking for the tension this creates with edge counts. |
| **IX. Accuracy** | **This feature implements it** | "waybill MUST minimize false positives — components listed in the SBOM that were not actually used." Unbacked edges are the relationship-level form of exactly that defect. |
| **X. Transparency** | PASS | FR-003, FR-005 and FR-008 supply the structured metadata the principle requires when accuracy or completeness cannot be guaranteed; R3 satisfies its "spec-native mechanisms rather than ad-hoc extensions where possible" clause. |

No unjustified violations. The one tension is recorded in Complexity
Tracking rather than waved through.

## Project Structure

### Documentation (this feature)

```text
specs/866-go-graph-completeness/
├── plan.md              # This file
├── spec.md              # Feature spec (3 clarifications resolved)
├── research.md          # Phase 0 — R1..R8
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/           # Phase 1
│   ├── edge-backing.md
│   └── completeness-invariants.md
├── checklists/
│   └── requirements.md
├── measurements/        # Committed probes + the evidence behind every figure
│   ├── probe_edge_truth.py
│   ├── probe_completeness.py
│   └── README.md
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/
├── scan_fs/package_db/golang/
│   ├── legacy.rs                  # R1 — the augment loop; the primary edit
│   └── graph_resolver.rs          # gosum_fallback_paths_for; may become unused
├── scan_fs/mod.rs                 # :952-966 — depends -> Relationship, sets provenance
├── generate/
│   ├── graph_completeness/mod.rs  # verdict + classify_transitive_edges_unresolvable
│   ├── graph_completeness/reason_codes.rs  # FR-005 reason vocabulary (T028)
│   ├── cyclonedx/dependencies.rs  # CDX dependencies[] — no native edge slot (R3)
│   ├── spdx/relationships.rs      # :79 `comment` — the native SPDX 2.3 carrier
│   └── spdx/v3_document.rs        # SPDX 3 Relationship elements
└── parity/extractors/             # new catalogue row requires a matching extractor

waybill-common/src/
└── resolution.rs                  # Relationship.provenance — exists, never emitted

waybill-cli/tests/                 # regression + invariant tests
xtask/corpus/quality-corpus.toml   # R8 — go-cobra + go-kubernetes bounds
waybill-cli/tests/fixtures/public_corpus/{go-cobra,pants-example-golang}/
docs/reference/sbom-format-mapping.md
```

**Structure Decision**: No new modules or crates. Every edit lands in an
existing file listed above. The feature is a removal plus an emission,
not new subsystem.

## Implementation Sequence

Phase numbering below is **identical to tasks.md** — the two documents
index the same phases, so "Phase 3" means one thing across the feature.

Derived from research R5 (the fix cascades) and the spec's two-P1
ordering.

**Phase 1 — Setup (T001-T004)**
Capture the before-state: cold baseline, warm control, kubernetes
baseline. Every later claim moves from a recorded number.

**Phase 2 — Foundational (T005-T008)**
`DeclaringSourceIndex` — the authority for what counts as backed.
Blocks every user story.

**Phase 3 — US1: every emitted edge is one waybill actually read (T009-T025)**
Two ordered halves in one phase:

- *US1a (T009-T013)* — remove the augment at `legacy.rs:~2036`; define
  "declared requirement" per FR-002 including `// indirect`, `replace`
  redirection and `vendor/modules.txt`. Expected: `go-cobra` 7 → 5
  golang edges, `blackfriday` and `check.v1` stranded.
- *US1b (T014-T018)* — emit edge provenance. Native `comment` for both
  SPDX formats, parity-bridge property for CDX, plus the catalogue row
  and its extractor.

**US1b is in the MVP, and must stay there.** It was initially sequenced
as a separate later phase on the grounds that it is the largest and
least certain piece. That was wrong: backed edges without stated backing
ask a consumer to take the correction on trust, and the provenance field
is precisely what makes "every edge is backed" checkable after the
document leaves the build. Shipping the correctness fix without it would
replace one unverifiable claim with another.

US1a must still precede US1b — `Relationship.provenance.source` names
`go.mod` today for the very edges US1a removes, so emitting provenance
first would publish a falsehood the product currently only holds in
memory (research R2).

**Phase 4 — US2: the completeness declaration can be trusted (T026-T033)**
Confirm the cascade (R4), then add FR-003's independent coverage-signal
check so the invariant holds even when graph shape does not reveal the
gap. Ships with Phase 3; neither is safe alone.

**Phase 5 — US3: diagnosability (T034-T036)**
Cause and remedy determinable from the document alone.

**Phase 6 — US4: this cannot silently return (T037-T039)**
Build-time gates for both defect classes, each teeth-checked by
observing it fail.

**Phase 7 — Polish (T040-T046)**
Re-author the m770 bounds and regenerate the two affected goldens
(freeze the fix set first), execute the FR-014 cross-ecosystem
measurement, and state the trivy tradeoff (R6) in the PR.

**MVP = Phases 1-4 (T001-T033)**: true edges, stated provenance, honest
declaration.

## Complexity Tracking

> Filled because the Constitution Check surfaces one real tension.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Emitted edge counts fall — `go-cobra` 7→5, `kubernetes` up to −554 — which reads as reduced graph completeness (Principle VIII) | Principle IX (Accuracy) is the governing principle for a *relationship that was never read from any input*. The removed edges are false positives, not lost findings; no component leaves the document (FR-007). m091 added them explicitly to match trivy's go.sum-derived count (R6), and this milestone reverses that tradeoff deliberately | Keeping the edges and marking them inferred was offered and rejected during clarification: it preserves traversability but leaves waybill asserting direct dependencies that the scanned project does not declare, in an artifact whose value is that its claims are checkable. Reviewers comparing edge counts against trivy must be told this drop is intentional — R6 requires it be stated in the PR and the milestone record |
| A `waybill:*` property for CycloneDX edge provenance, where SPDX uses a native field | CycloneDX 1.6 `dependencies[]` has no `properties`, no `evidence`, and is not an annotation subject (R3) — there is nothing native to use | Omitting provenance on CDX entirely would make the format's edges unverifiable while SPDX's are verifiable, breaking the cross-format parity the catalogue exists to enforce. Principle V's parity-bridge carve-out covers exactly this case, conditional on the documented justification row |
