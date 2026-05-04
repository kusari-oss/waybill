# Implementation Plan: Cross-format SBOM annotation parity

**Branch**: `071-annotation-parity-spdx23` | **Date**: 2026-05-04 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/071-annotation-parity-spdx23/spec.md`

## Summary

Close the SPDX 2.3 emitter parity gap that produced 11,130 of the 12,165 alpha.13 conformance findings. Every `mikebom:*` annotation that mikebom emits in any of CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 must appear in the other two with semantically equivalent values, except where an inherent format-spec asymmetry is registered in the parity catalog with documented rationale.

**Approach**: The core infrastructure already exists — `mikebom-cli/src/parity/extractors/` defines 68 catalog rows plus a `Directionality` enum (`SymmetricEqual` / `CdxSubsetOfSpdx` / `PresenceOnly` / `CdxOnly`), the SPDX 2.3 emitter has a `MikebomAnnotationCommentV1` JSON envelope shape under `Package.annotations[].comment`, and the SPDX 3 emitter has the equivalent under `Annotation.statement`. The work is **verify and close gaps**, not build-new-infrastructure: audit each of the 6 known problem keys (and the discovery pass for any others), wire them through the existing envelope mechanisms in the SPDX 2.3 emitter, add an `order_sensitive` field to `ParityExtractor` for the Q2 canonicalization rule, and add a pre-PR gate test that fails when any emitted `mikebom:*` key lacks a catalog row (Q1 hard-fail).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–070; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde_json` (JSON value walking + canonicalization), `quick-xml` n/a (SPDX 2.3 / SPDX 3 are JSON), `tracing`, `anyhow`. The existing `parity/extractors/` infrastructure (68 catalog rows, `Directionality` enum, `MikebomAnnotationCommentV1` envelope, `extract_mikebom_annotation_values` helper) IS the substrate. No new crates.
**Storage**: N/A — all parity comparison happens in-memory at test/CI time over emitted JSON documents.
**Testing**: Existing — `cargo +stable test --workspace`; the milestone adds (a) integration tests under `mikebom-cli/tests/` that exercise the parity-check subcommand end-to-end on fixture SBOMs, (b) a per-key unit test in `parity/extractors/spdx2.rs` that asserts the SPDX 2.3 envelope round-trips each of the 6 known keys, (c) a synthetic regression test (US4 acceptance #1) that introduces a one-format-only annotation and asserts the pre-PR gate rejects it.
**Target Platform**: Same as current mikebom — Linux + macOS user-space, no platform-specific work.
**Project Type**: Existing three-crate workspace per Constitution VI; the milestone is `mikebom-cli`-only.
**Performance Goals**: Pre-PR gate parity check completes in <30s on the existing fixture suite (per spec Assumption). No measurable runtime cost expected on `mikebom sbom scan` itself — adding annotation push calls in the SPDX 2.3 emitter is a few extra heap allocations per component.
**Constraints**: Must not regress the existing 27 byte-identity goldens or any of the ~80 existing parity rows. Must not break the SPDX 2.3 JSON schema validation (the existing `jsonschema = "0.46"` dev-dep validates emitted output against `spdx-2.3-json` schema).
**Scale/Scope**: 6 known problem keys + ~5 unknown-and-discoverable per spec FR-003 = ~11 keys to audit/fix. ~5 inherent-asymmetry catalog rows to re-audit per FR-011. Single `mikebom-cli` crate; ~3-5 source files modified (`generate/spdx/annotations.rs`, `parity/extractors/spdx2.rs`, `parity/extractors/common.rs`, `parity/extractors/mod.rs`, possibly `cli/parity_cmd.rs`).

## Constitution Check

Running through the v1.4.0 principles before Phase 0:

- **I. Pure Rust, Zero C** — ✅ no C added; pure Rust changes to existing modules.
- **II. eBPF-Only Observation** — ✅ untouched; this milestone only changes how already-discovered components are *serialized*, not how they're discovered.
- **III. Fail Closed** — ✅ the pre-PR gate hard-fail per Q1 clarification IS a fail-closed posture for parity drift; aligns with the principle.
- **IV. Type-Driven Correctness** — ✅ the new `order_sensitive: bool` field on `ParityExtractor` and any new directionality logic uses existing newtypes; no `.unwrap()` in production. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per project convention.
- **V. Specification Compliance** — ✅ this milestone IS the codification of Principle V's standards-native-precedence requirement at the cross-format-parity layer. FR-004 / FR-005 / FR-011 explicitly require auditing every non-symmetric catalog row and citing the standards-native superseding construct in both code comments and `docs/reference/sbom-format-mapping.md`. The `mikebom:lifecycle-scope` row (C42, currently `CdxOnly` because SPDX uses native dep-relationship types for lifecycle scope) is the canonical example the constitution itself names; this milestone re-audits it and any analogous cases.
- **VI. Three-Crate Architecture** — ✅ all changes confined to `mikebom-cli`.
- **VII. Test Isolation** — ✅ no eBPF involvement; new tests run unprivileged.
- **VIII. Completeness** — ✅ orthogonal; this milestone narrows the parity gap, doesn't change discovery.
- **IX. Accuracy** — ✅ orthogonal; same reason.
- **X. Transparency** — ✅ the parity catalog with rationale + the `docs/reference/sbom-format-mapping.md` published mapping ARE the transparency mechanism for the choice between native-field and `mikebom:*` annotation per format. FR-005 explicitly requires this surface.
- **XI. Enrichment** — ✅ orthogonal.
- **XII. External Data Source Enrichment** — ✅ orthogonal.

**Gates: PASS.** No deviations to record. The Complexity Tracking table at the bottom is empty.

## Project Structure

### Documentation (this feature)

```text
specs/071-annotation-parity-spdx23/
├── spec.md                  # complete (with 3 clarifications)
├── plan.md                  # this file
├── research.md              # Phase 0 output (next)
├── data-model.md            # Phase 1 output
├── quickstart.md            # Phase 1 output
├── contracts/
│   └── parity-catalog-row.md  # Phase 1 output (extractor contract + new order_sensitive field)
├── checklists/
│   └── requirements.md      # complete
└── tasks.md                 # Phase 2 output (later — /speckit.tasks)
```

### Source Code (repository root)

The milestone touches only `mikebom-cli`. Specific files (concrete paths, not options):

```text
mikebom-cli/
├── src/
│   ├── generate/
│   │   └── spdx/
│   │       └── annotations.rs        # ADD push() calls for keys missing in SPDX 2.3 emit:
│   │                                 #   mikebom:source-files, mikebom:cpe-candidates,
│   │                                 #   mikebom:deps-dev-match (audit), mikebom:npm-role
│   │                                 #   (audit), mikebom:sbom-tier (audit). Reuse
│   │                                 #   MikebomAnnotationCommentV1 envelope (no new shape).
│   ├── parity/
│   │   ├── extractors/
│   │   │   ├── common.rs             # ADD `order_sensitive: bool` field on ParityExtractor +
│   │   │   │                          #   canonicalize_for_compare() helper applying the
│   │   │   │                          #   Q2 default-with-override rule.
│   │   │   ├── mod.rs                # AUDIT 68 rows: any new keys emitted but missing here
│   │   │   │                          #   (FR-003 discovery); confirm 6 problem keys are
│   │   │   │                          #   SymmetricEqual; re-audit non-SymmetricEqual rows
│   │   │   │                          #   for rationale comments (FR-004 / FR-011).
│   │   │   ├── spdx2.rs              # WIRE per-key extractors to the (now-emitted) keys.
│   │   │   └── spdx3.rs              # AUDIT (likely no changes — SPDX 3 already emits
│   │   │                              #   per `Annotation.statement` and is in lockstep
│   │   │                              #   with CDX per the user's data).
│   │   └── catalog.rs                # ADD non-symmetric-row rationale entries (FR-004) so
│   │                                 #   they round-trip into the published markdown table.
│   └── cli/
│       └── parity_cmd.rs             # ADD --check-completeness flag (or extend the default
│                                      #   `mikebom parity-check` mode) to discover any
│                                      #   emitted mikebom:* key not catalogued (FR-006).
└── tests/
    ├── parity_completeness.rs        # NEW: pre-PR gate test (FR-006) that asserts no
    │                                  #   uncatalogued mikebom:* keys appear in any of the
    │                                  #   3 emitted formats across the 27 fixture SBOMs +
    │                                  #   that every SymmetricEqual row's per-format
    │                                  #   sets agree after canonicalization.
    └── parity_synthetic_drift.rs     # NEW: US4 regression test — constructs a synthetic
                                       #   SBOM where a mikebom:foo key appears only in
                                       #   CDX, asserts the parity-check fails with a
                                       #   clear error.

docs/
└── reference/
    └── sbom-format-mapping.md        # ADD section "Cross-format annotation parity catalog"
                                       # listing every non-SymmetricEqual catalog row with
                                       # rationale + standards-native superseding construct
                                       # (FR-005).
scripts/
└── pre-pr.sh                         # AUDIT: confirm `cargo +stable test --workspace`
                                       # already executes the new parity_completeness.rs
                                       # test (it will, by virtue of being a normal cargo
                                       # integration test). No script changes expected.
```

**Structure Decision**: Single-crate work confined to `mikebom-cli`. The existing `parity/extractors/` infrastructure is the target — extend, don't replace. The existing `MikebomAnnotationCommentV1` envelope shape (`Package.annotations[].comment` carrying a JSON object) is the SPDX 2.3 emission target — wire missing keys through it, don't introduce a parallel shape. The pre-PR gate enforcement is a normal `cargo test` integration test (so it runs in `./scripts/pre-pr.sh` automatically without any script changes).

## Phase 0: Outline & Research

**Output**: [research.md](research.md) — full content authored alongside this plan.

The Technical Context above contains zero `NEEDS CLARIFICATION` markers — the three spec clarifications already pinned the open questions (gate posture, canonicalization rule, measurement scope), and the existing parity infrastructure makes the architectural choices concrete. Research focuses on three operational unknowns:

1. **Per-key audit of the 6 known problem keys.** For each of `mikebom:source-files`, `mikebom:sbom-tier`, `mikebom:cpe-candidates`, `mikebom:deps-dev-match`, `mikebom:npm-role`, `mikebom:lifecycle-scope`: what's the current SPDX 2.3 emission state, the current SPDX 3 emission state, and the current parity-extractor-row `Directionality`? Some are CFI not because emission is missing, but because emission shape is wrong (e.g., key emitted in CDX but the SPDX 2.3 envelope decode fails because the value type differs). Resolved in research.md.

2. **Discovery pass for unknown keys.** Any `mikebom:*` key emitted by any of the three emitters that has no catalog row gets identified and added (FR-003). A grep across the three emitters (`generate/cyclonedx/`, `generate/spdx/`, `generate/spdx/v3_*`) names every literal `mikebom:` string and cross-references it against the 68 catalog rows. Resolved in research.md.

3. **Inherent-asymmetry audit (FR-011).** The currently-known case is `C42 mikebom:lifecycle-scope` (CDX-only, supplanted by SPDX dep-relationship types). The audit confirms whether milestones 007–070 introduced any *new* legitimate asymmetries. Resolved in research.md.

## Phase 1: Design & Contracts

**Outputs**: [data-model.md](data-model.md), [contracts/parity-catalog-row.md](contracts/parity-catalog-row.md), [quickstart.md](quickstart.md), and an agent-context update.

### 1. Data model (`data-model.md`)

Lists the entities the spec already enumerated (Annotation key, Parity catalog row, Directionality, Inherent asymmetry, CFI finding, Format) plus the concrete data shapes:

- `ParityExtractor` struct (existing + new `order_sensitive: bool` field)
- `Directionality` enum (existing — no change needed; the four variants already cover Q1/Q2/Q3 outcomes)
- `MikebomAnnotationCommentV1` envelope (existing) — unchanged shape, but documented here as the canonical SPDX 2.3 carrier
- `CatalogRowRationale` (new) — a struct (or static markdown derived from inline comments) that captures the rationale for non-`SymmetricEqual` rows in a form publishable to `docs/reference/sbom-format-mapping.md`

### 2. Contracts (`contracts/parity-catalog-row.md`)

The contract that future spec authors / reviewers / pre-PR gate consumers depend on. Documents:

- The shape and meaning of every `Directionality` variant (with the existing `CdxOnly` case as the worked example)
- The new `order_sensitive: bool` field semantics (default `false`; `true` disables array sorting in canonicalization)
- The canonicalization algorithm: lexicographic key sort, lexicographic array sort (default) or insertion order (`order_sensitive=true`), whitespace normalization
- The hard-fail rule: any emitted `mikebom:*` key not in the catalog aborts the pre-PR gate with a specific error message naming the key and the 3 format-presence states
- The format-mapping doc-sync rule: every non-`SymmetricEqual` row MUST appear in `docs/reference/sbom-format-mapping.md` with rationale; CI sync test enforces this

### 3. Quickstart (`quickstart.md`)

Three operator-facing recipes, each runnable end-to-end:

- **Recipe 1 (validate parity locally before opening a PR):** `mikebom sbom scan --path ./mikebom-cli` then `mikebom parity-check <path-to-cdx> <path-to-spdx2.3> <path-to-spdx3>` shows the pass/fail diff per row.
- **Recipe 2 (add a new annotation key in a future milestone):** code change in all three emitters + add a new ParityExtractor row + add `docs/reference/sbom-format-mapping.md` entry → pre-PR gate passes.
- **Recipe 3 (verify the alpha.13 → post-fix improvement):** run the external conformance harness against pre/post SBOMs, confirm the ≥95% CFI reduction.

### 4. Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after writing the artifacts. Adds the milestone's tech stack note to `CLAUDE.md`'s "Active Technologies" section.

## Re-evaluate Constitution Check

Post-design review of the artifacts above: still ✅ on all 12 principles. The plan does not introduce new `mikebom:*` annotations (FR-009 closes the existing 6 by emitting them symmetrically; the audit pass per FR-011 may *retire* one or two `mikebom:*` annotations in favor of standards-native fields if the audit surfaces such cases). The `order_sensitive` field is purely internal to the parity catalog and never appears in SBOM output. The pre-PR gate hard-fail aligns with Principle III. Principle V is materially strengthened by FR-004 / FR-005 / FR-011's documentation requirements.

**Gates: PASS post-design.** No new deviations.

## Complexity Tracking

*(empty — no constitution gate violations; the milestone is straight extension of existing infrastructure)*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| *(none)* | | |
