# Implementation Plan: Graph-completeness reachability signal for downstream analysis tools

**Branch**: `177-graph-reachability-signal` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/177-graph-reachability-signal/spec.md`

## Summary

**Primary requirement**: extend the m158 `mikebom:graph-completeness-reason` closed vocabulary with one new code — `transitive-edges-unresolvable: <ecosystem-list>` — that fires whenever the scan emits ≥1 design-tier OR analyzed-tier component that lacks a same-package source-tier-or-higher counterpart. Flips `mikebom:graph-completeness` from `"complete"` → `"partial"` on affected scans so downstream reachability tools can machine-check the signal before running.

**Technical approach**: **surgical extension of an existing subsystem**. The m158 graph-completeness pass at `mikebom-cli/src/generate/graph_completeness/mod.rs::compute_graph_completeness` already runs at emit-time, classifies gaps into reason codes, and threads the result through all three format emitters. Adding a new code means:

1. **One new `ReasonCode` enum variant** in `reason_codes.rs` — `TransitiveEdgesUnresolvable { ecosystems: Vec<String> }`. Uses the existing `MultiEcosystemPartialRoot` shape precedent (Vec<String> of PURL-type-canonical ecosystem names, sorted, deduplicated).

2. **New classification function** invoked from `compute_graph_completeness` after the existing `MultiEcosystemPartialRoot` + `OrphanedComponentsDetected` classifiers. Predicate: iterate `components`, identify each entry with `sbom_tier ∈ {Some("design"), Some("analyzed")}`, look up whether ANY other component with the same PURL type + name has `sbom_tier ∈ {Some("source"), Some("deployed"), Some("build")}`; if not, add its PURL type to the ecosystem set. If the set is non-empty, push `TransitiveEdgesUnresolvable { ecosystems: sorted_dedup(set) }` onto `reason_codes`.

3. **Wire-code name** = `transitive-edges-unresolvable` (chosen at authoring time per FR-001; matches the spec's example + fits the existing kebab-case-name convention). Detail template: `transitive-edges-unresolvable: <comma-separated ecosystem list>`. No count in the detail per research §R3 disposition — the ecosystem list is the actionable signal for reachability consumers; component count would be diagnostic noise (differs from `EdgeResolutionDegraded` which counts DROPPED edges — this code counts affected ecosystems where the whole per-package subgraph is unwalkable).

4. **Docs updates** — two files:
   - **`docs/reference/reading-a-mikebom-sbom.md` §3.4** — the existing `mikebom:graph-completeness + mikebom:graph-completeness-reason` subsection at line 494 gets a new sub-paragraph explaining the transitive-edges-unresolvable code, its reachability-consumer contract, and a jq recipe for machine-checking. Cross-reference to m175 design-tier subsection (compose orthogonally: m175 = operator UX, m177 = machine attestation).
   - **`docs/reference/sbom-format-mapping.md`** — the C111 catalog row already documents `mikebom:graph-completeness-reason` at Section C. Update the closed-vocabulary listing to enumerate all 9 codes (was 8). Constitution Principle V audit: this is a vocabulary extension of an existing annotation, NOT a new `mikebom:*` construct. No re-audit needed.

5. **Ripple**: existing golden fixtures containing design-tier or analyzed-tier components without same-package source-tier-or-higher peers will flip `mikebom:graph-completeness` from `"complete"` → `"partial"` and gain the new reason code. Bounded delta per SC-006/SC-007. Fixtures likely affected: `pip.cdx.json` (design-tier from requirements.txt), possibly `composer.cdx.json` if composer emits design-tier fixtures, possibly others.

**Cross-format parity**: no new parity extractor needed. `graph-completeness-reason` (C111) is already a `SymmetricEqual` extractor from m158; adding a new code inside the existing value doesn't change extractor shape. Verified by inspection.

**Advisory-log interaction**: NONE. m175's advisory-log fires on operator-facing terms (`"design-tier components detected: N"`); m177's signal is machine-readable and doesn't fire an advisory. They compose orthogonally per Assumptions.

**Blast radius**: ~40 lines in `reason_codes.rs` (new variant + `to_reason_string` arm + unit tests), ~50 lines in `graph_completeness/mod.rs` (new classifier function + call site), ~40 lines in `docs/reference/reading-a-mikebom-sbom.md`, ~5 lines in `docs/reference/sbom-format-mapping.md`, ~200 lines in a new integration test file at `mikebom-cli/tests/reachability_signal.rs` (7 tests covering US1/US2/US3 acceptance criteria). Golden delta: ~4-6 fixtures with bounded per-fixture change.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–176; local + CI both on 1.97 post-m175). No nightly required.

**Primary Dependencies**: Existing only — `std::collections::HashMap` + `std::collections::HashSet` (already used pervasively in graph_completeness), `mikebom_common::resolution::ResolvedComponent` (existing type; `sbom_tier: Option<String>` field at `resolution.rs:98`), `mikebom_common::types::purl::Purl` (existing type; `ecosystem()` method already used at `graph_completeness/mod.rs:243`). **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — pure emission-time classification; no persistence.

**Testing**: `cargo test` — 1 new integration test at `mikebom-cli/tests/reachability_signal.rs` covering the 3 US acceptance predicates + edge cases (deployed-tier peer, analyzed-tier only, composition with existing codes, offline orthogonality, empty scan). Plus 3-5 unit tests inline in `reason_codes.rs` for `to_reason_string` output of the new variant (matches existing test-coverage precedent).

**Target Platform**: All hosts mikebom builds on — Linux, macOS, Windows.

**Project Type**: cli (mikebom sbom-generation CLI).

**Performance Goals**: N/A — the classifier is `O(N)` over components with a `HashMap<(purl_type, name), tier>` lookup; happens once per scan at emit-time (same site as the existing m158 pass).

**Constraints**: SC-006 gate — fully-resolved golden fixtures stay `"complete"`. SC-007 gate — the ONLY permitted deltas on affected goldens are the two annotations (`mikebom:graph-completeness` value + `mikebom:graph-completeness-reason` addition). No other bytes drift.

**Scale/Scope**: Small. 2 code files touched, 2 docs files touched, 1 new integration test file, ~4-6 golden fixtures regenerated with bounded per-fixture delta.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new Cargo dependencies. Pure Rust addition.
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched. mikebom does NOT run reachability itself; only surfaces the reliability signal.
- **III. Fail Closed**: ✅ The classifier is caution-first — under Q1 caution-first from m158, unclassifiable gaps fall to `unknown` rather than lying. The new code adds a specific classification path but doesn't weaken the `unknown` fallback.
- **IV. Type-Driven Correctness**: ✅ New `ReasonCode` enum variant uses `Vec<String>` for the ecosystems list (existing `MultiEcosystemPartialRoot` precedent). No new domain types. No `.unwrap()` in production code.
- **V. Specification Compliance / Standards-native precedence**: ✅ **NO NEW `mikebom:*` ANNOTATION**. m177 extends the existing `mikebom:graph-completeness-reason` (C111) closed vocabulary with one new code. Constitution Principle V audit is inherited from m158's original KEEP-NO-NATIVE audit for C111 — CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 have no native "why the emitted dep graph is unreliable for reachability" annotation. No re-audit needed. Adding a new code to a closed vocabulary is a governance event per m158's "closed vocabulary is additive" contract, documented via CHANGELOG on merge.
- **VI. Three-Crate Architecture**: ✅ Change contained to `mikebom-cli` (graph_completeness pass + docs). No `mikebom-common` or `mikebom-ebpf` changes. `ResolvedComponent.sbom_tier` field already exists (m005-era).
- **VII. Test Isolation**: ✅ Integration test uses `assert_cmd` + per-test tempdir; no shared state.
- **VIII. Completeness**: ✅ **Advances Principle VIII.** Post-177, the graph-completeness signal correctly reflects reachability-consumer trust — pre-177 was silently misleading on design-tier scans.
- **IX. Accuracy**: ✅ **Advances Principle IX** most directly. Pre-177 mikebom silently claimed `"complete"` on constraint-only scans — a false positive from the reachability-consumer perspective. Post-177 the signal is accurate.
- **X. Transparency**: ✅ **Directly serves Principle X.** The signal transparently tells reachability consumers when the graph is unreliable, letting them refuse or downgrade analysis rather than produce silent false negatives.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A.

**Strict Boundaries check**:
- **New subprocess**: ✅ None.
- **New network access**: ✅ None.
- **New filesystem writes**: ✅ None. Purely emission-time classifier extension.
- **New `mikebom:*` annotation namespaces**: ✅ **Zero.** Vocabulary extension of C111 only.
- **New Cargo dependencies**: ✅ Zero.
- **Strict Boundary §5 (file-tier no-duplicates)**: N/A — file-tier walker unaffected.

**Verdict**: All principles pass. Zero violations. Milestone directly advances Principles VIII / IX / X (Completeness / Accuracy / Transparency) by fixing a silent false-positive that misled reachability consumers on constraint-only scans.

## Project Structure

### Documentation (this feature)

```text
specs/177-graph-reachability-signal/
├── spec.md              # Feature specification (already written + Q1/Q2 clarified)
├── plan.md              # This file
├── research.md          # Phase 0 — same-package identity, ecosystem-canonical naming, composition semantics
├── data-model.md        # Phase 1 — new ReasonCode variant + classifier function contract + wire shape
├── quickstart.md        # Phase 1 — 3-scenario verification recipe (US1 machine check, US2 constraint-only, US3 polyglot)
├── contracts/           # Phase 1 — reason-code wire-format contract + docs-anchor contract
├── checklists/          # Requirements checklist (spec-phase output — 16/16 PASS)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── generate/
│       └── graph_completeness/
│           ├── reason_codes.rs             # ~40 lines: new TransitiveEdgesUnresolvable variant + to_reason_string arm + inline unit tests
│           └── mod.rs                      # ~50 lines: new classifier function + call site in compute_graph_completeness after MultiEcosystemPartialRoot classification
└── tests/
    └── reachability_signal.rs              # NEW ~200 lines: 7 US1/US2/US3 integration tests

docs/reference/
├── reading-a-mikebom-sbom.md               # ~40 lines: extend the C111 subsection at line 494 with the new code + jq recipe + m175 compose note
└── sbom-format-mapping.md                  # ~5 lines: update the C111 row to enumerate all 9 codes (was 8)

# NO changes to:
mikebom-common/                             # sbom_tier field + Purl.ecosystem() already exist
mikebom-cli/src/scan_fs/                    # Zero reader changes
mikebom-cli/src/cli/                        # Zero CLI changes
mikebom-cli/src/parity/                     # C111 extractor unchanged (SymmetricEqual on the value string)

mikebom-cli/tests/fixtures/golden/          # 4-6 fixtures regenerate with bounded delta (graph-completeness + reason values only)
```

**Structure Decision**: pure classifier extension. One new enum variant + one new classifier function + one call-site edit + two docs updates + one new integration test file. No new subsystem introduced. The m158 graph_completeness scaffolding does all the heavy lifting; m177 is a small, well-scoped extension.

## Complexity Tracking

No constitution violations to justify. The plan is a straight-line vocabulary extension with zero new detection subsystems, zero SBOM shape changes, and zero new `mikebom:*` annotations. Golden regeneration is bounded and semantically intentional (fixing a pre-existing silent false-positive, not a breaking change).
