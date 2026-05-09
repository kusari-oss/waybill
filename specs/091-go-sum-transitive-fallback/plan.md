# Implementation Plan: Go reader go.sum-based transitive fallback

**Branch**: `091-go-sum-transitive-fallback` | **Date**: 2026-05-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/091-go-sum-transitive-fallback/spec.md`

## Summary

Insert a new step 5 — `ResolutionStep::GoSumFallback` — into milestone-055's `GraphResolver` ladder at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`. When the post-step-3 state is "most go.sum modules are in `ctx.go_sum_modules` but absent from the resolver map" (the offline-and-cache-empty signature), step 5 augments the project's root-module entry with edges to every unique go.sum (module, version) pair, and tags those modules' source as `GoSumFallback`. Existing step 4 (empty-fallthrough) becomes step 6, kept as-is for the rare case where step 5's go.sum-closure-claim leaves entries with neither parent nor any provenance attribution.

The per-module `source: ResolutionStep` field is the existing transparency carrier (Constitution Principle X). Every transitive component already emits a `mikebom:resolver-step` property derived from this field; step 5's new variant adds `go-sum-fallback` to that property's value space. CDX `Component.evidence.identity[].methods[]` carries the same value via a "manifest-analysis" technique with an explicit `confidence < 0.85` reflecting the lower fidelity. SPDX 2.3 `package.annotations[]` and SPDX 3 `software_Package.evidence` carry the same in their respective shapes — exact field-name selection runs as Phase 0 research §1 per Constitution Principle V's "audit native first" rule.

`parse_go_sum` already lives at `legacy.rs:353` — the new step 5 reuses it without a new parser. The `ctx.go_sum_modules` field on `WorkspaceContext` already enumerates every (module, version) pair in `go.sum` for use by step 4's empty-fallthrough; step 5 reuses the same enumeration to drive root-edge augmentation.

Expected outcome on the cri-tools transitive-parity audit fixture: edge count rises from 31 to ≥130 (≥90% of trivy's 142). The milestone-083 `transitive_parity_go.rs` regression test bumps its baseline + adds at least one representative `pkg:golang/<root> → pkg:golang/<go-sum-only-transitive>` edge per quickstart Recipe 3 of milestone 087.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–090; no nightly required for this user-space-only Go-reader extension).
**Primary Dependencies**: existing only — `parse_go_sum` at `legacy.rs:353`, `WorkspaceContext.go_sum_modules` at `graph_resolver.rs`, `ResolutionStep` enum at `graph_resolver.rs:64`. **No new Cargo dependencies.**
**Storage**: N/A — purely in-process per-scan resolution. Mirrors milestones 002–055.
**Testing**: existing test suite. The milestone-083 `transitive_parity_go.rs` regression test gets a baseline bump; the milestone-055 `scan_go.rs` tests for the cache-populated path stay unchanged (FR-005 invariant).
**Target Platform**: All platforms supported by mikebom (Linux, macOS).
**Project Type**: Single project — Rust workspace internal-library extension.
**Performance Goals**: Step 5 wall-time bound: ≤10 ms on a 262-line `go.sum` (the cri-tools fixture). The work is one HashMap insert per go.sum entry — sub-millisecond in practice. Mikebom scan wall-time stays within ±5% of pre-091 baseline.
**Constraints**: Constitution Principle V (Specification Compliance — standards-native fields take precedence). Constitution Principle X (Transparency — provenance must be machine-readable). FR-005 (no regression for cache-populated path). Goldens for the milestone-013 `golang/simple-module` fixture (a thin fixture with a tiny `go.sum`) MAY regenerate IF the simple-module fixture's go.sum modules now flow through step 5 instead of step 4-empty-fallthrough. If so, the diff scope MUST be limited to the per-component provenance discriminator field (no PURL changes, no count-of-components changes).
**Scale/Scope**: ~50 LOC in `graph_resolver.rs` (new step 5 method, new `ResolutionStep` variant, summary-counter wiring). ~10 LOC of provenance-property emission in the per-format generation modules (CDX/SPDX 2.3/SPDX 3). ~5 LOC of test-baseline bump in `transitive_parity_go.rs`. Total ~70 LOC + 2 unit tests (step 5 happy-path + go.sum-absent fallthrough).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ PASS | No new C-linking deps. `parse_go_sum` is pure Rust. |
| II. eBPF-Only Observation | ✅ PASS | Not applicable — this is enrichment via lockfile parsing, NOT discovery. Per Principle XII, lockfile-based enrichment of already-discovered components is permitted. The components themselves come from the existing scan_fs walk; step 5 only adds dependency-relationship edges between them. |
| III. Fail Closed | ✅ PASS | `parse_go_sum` returns `Vec<GoSumEntry>` (silently skips malformed lines per existing milestone-055 contract); step 5 adds zero edges if the parse returns empty, falling through cleanly to step 6. No silent gap. |
| IV. Type-Driven Correctness | ✅ PASS | `ResolutionStep` is a Rust enum (newtype-equivalent); `GoSumFallback` adds a typed variant. No `String`-typed regressions. No new `.unwrap()` in production code. |
| V. Specification Compliance | ✅ PASS | Phase 0 research §1 audits CDX 1.6 + SPDX 2.3 + SPDX 3 for native per-component provenance fields BEFORE introducing any `mikebom:*` property. The existing `mikebom:resolver-step` property (already used by milestone 055) is the canonical carrier; new variant's value `go-sum-fallback` is additive. Native CDX `Component.evidence.identity[].methods[].technique` is the per-format equivalent. |
| V — Standards-native precedence | ✅ PASS | Constitution requires the audit; research §1 performs it. Existing `mikebom:resolver-step` property is grandfathered (introduced in milestone 055 with documented justification at `docs/reference/sbom-format-mapping.md`); this milestone reuses without inventing new fields. |
| VI. Three-Crate Architecture | ✅ PASS | No new crates. Changes scoped to `mikebom-cli/src/scan_fs/package_db/golang/`. |
| VII. Test Isolation | ✅ PASS | All tests run unprivileged. No new eBPF dependencies. |
| VIII. Completeness | ✅ PASS | This milestone INCREASES completeness by ≥4× on offline-cache-empty Go scans (31 → ~130 edges). Aligns with the principle's intent. |
| IX. Accuracy | ✅ PASS | go.sum content is the authoritative record of what Go's build system fetched; emitting edges from go.sum entries does NOT introduce phantom components — every component was already in mikebom's emitted set, just without an inbound edge. The per-component provenance annotation explicitly flags the lower-fidelity discovery path. |
| X. Transparency | ✅ PASS | Per-component `mikebom:resolver-step = go-sum-fallback` annotation makes the fallback-path discovery explicit. Operators querying the SBOM can distinguish step-5 components from step-1/2/3 components by reading a single field. |
| XI. Enrichment | ✅ PASS | go.sum-driven edge emission is enrichment (Principle XII) of already-discovered Go-module components. |
| XII. External Data Source Enrichment | ✅ PASS | `go.sum` is a lockfile; lockfile-based enrichment of components-observed-in-the-scan is explicitly permitted. Constraint 1 (no new components) holds — every component was already present in mikebom's scan output via go.sum-modules enumeration. Constraint 2 (provenance annotation) is FR-002. |

**Strict Boundaries**:
- ✅ No lockfile-based dependency discovery — components come from the scan; step 5 only adds RELATIONSHIPS.
- ✅ No MITM proxy — unchanged.
- ✅ No C code — unchanged.
- ✅ No `.unwrap()` in production — unchanged.

**Pre-PR Verification (mandatory)**: standard `./scripts/pre-pr.sh` gate. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` must report clean.

**Gate verdict**: ✅ all gates pass. No constitution amendments required.

## Project Structure

### Documentation (this feature)

```text
specs/091-go-sum-transitive-fallback/
├── plan.md              # This file
├── research.md          # Phase 0 output (native-field audit + step-5 dispatch decision + edge attribution semantics)
├── data-model.md        # Phase 1 output (ResolutionStep::GoSumFallback variant + LadderSummary extension)
├── quickstart.md        # Phase 1 output (maintainer recipes: reproduce 31-edge baseline, apply step-5, verify ≥130, regen baseline)
├── contracts/
│   └── go-sum-fallback.md  # The new ladder step + per-component provenance contract
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already complete)
└── tasks.md             # Phase 2 output (/speckit.tasks command — NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── golang/
│               ├── graph_resolver.rs      # MODIFIED: ResolutionStep::GoSumFallback variant; step5_go_sum_fallback method; LadderSummary.gosum_fallback_count counter
│               └── legacy.rs              # POSSIBLY MODIFIED: edge-emission closure may need a tweak if root-augmentation lives there instead of in graph_resolver.rs
├── tests/
│   └── transitive_parity_go.rs            # MODIFIED: edge-count baseline bump 31 → ≥130; new representative `clap_derive →`-style edge for go-sum-only transitives
└── (per-format emission modules)          # POSSIBLY MODIFIED: provenance-annotation field name per Phase 0 §1; existing mikebom:resolver-step property may already cover the case

specs/083-transitive-correctness/
└── research.md                             # MODIFIED: §8 — Ecosystem: Go audit row updated to mark gap closed
```

**Structure Decision**: Single-project Rust workspace, internal-library extension. PR diff target: ~70 LOC across 2–3 source files + 1 test-file baseline bump + 1 audit-doc update. No new crates, no new dependencies, no fixture changes (the audit fixture lives in the post-090 `mikebom-test-fixtures` repo and is unchanged).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitution violations. Complexity tracking N/A.

The implementation's interesting wrinkle is the dispatch-location decision: whether step 5 lives in `GraphResolver::resolve()` (alongside steps 1–4, with the existing per-module `source` tagging) or in `legacy.rs::read()` (post-resolver, augmenting the root's edge list directly). Both work; research §2 picks one.
