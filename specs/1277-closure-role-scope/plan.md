# Implementation Plan: a closure member that only builds the artifact must not read as part of it

**Branch**: `1277-closure-role-scope` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification at `specs/1277-closure-role-scope/spec.md`

## Summary

97 of 362 closure members on `nix-closure-moat` are in a role that does not
ship — 80 `build-tooling`, 17 `unreferenced` — and none carries a native
CycloneDX `scope`, so the schema's `"default": "required"` makes a consumer
filtering for what is in the artifact see every nix build tool as shipped.
This feature assigns `scope: excluded` from the C182 role, exempts the
packages the pinned compiler supplies, and records which mechanism
classified each scoped component.

The design follows **milestone 112**, not milestone 052. Routing through
`LifecycleScope::Build` would get all three native carriers for free, but it
is gated on `--include-dev` so the feature would not fire by default, and it
would make `--exclude-scope build` drop the 97 components — destroying the
reason the `unreferenced` ones are emitted at all. m112's
`BuildInclusion::NotNeeded` already solved both carve-outs for an identical
kind of claim, with the rationale that a build-graph fact is "not a
lifecycle-scope preference the operator can toggle". See research R1–R2.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (annotation values, document construction), `std::collections::{BTreeSet, HashSet}`, `tracing` (one INFO summary). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; the classification is emitted into the SBOM and never read back.
**Testing**: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, no `+toolchain` argument (#1189). Corpus goldens refreshed through `public-corpus.yml` only, never locally (rule zero).
**Target Platform**: user space, all hosts. The `--nix-closure` path needs a host `nix` at scan time; the feature adds no new subprocess.
**Project Type**: single Rust workspace (three crates).
**Performance Goals**: no measurable change. The assignment is one pass over an already-materialised component slice, reading an annotation already on each component.
**Constraints**: every scan that does not enable `--nix-closure` must be byte-identical (FR-008). One corpus target moves.
**Scale/Scope**: 97 of 362 components on one corpus target; 16 targets are a byte-identity control; the FR-005 exemption's population is measured **zero**, so it needs a synthetic fixture.

### Unknowns

- **FR-004a — do the role classifier's seven field names cover the dependency keys real derivations carry?** Unmeasurable in-tree: every fixture carries only those seven, so a grep over them is circular, and a real closure needs a host `nix`. Scheduled as an `#[ignore]`d env-gated probe (research R7). SC-009 asks for the count to be known, not to be zero.
- **R6 — whether the native carriers in SPDX 2.3 and SPDX 3 are in scope.** Both exist and the spec names only CycloneDX. This needs a decision that is a spec change, not a plan detail; see Complexity Tracking.

## Constitution Check

Constitution v4.0.2.

| Principle | Status | Note |
|---|---|---|
| I. Pure Rust, Statically Linked | **PASS** | Zero new dependencies; no C, no new subprocess. |
| II. eBPF-Only Observation | **N/A** | User-space only; `waybill-ebpf` untouched. |
| III. Fail Closed | **PASS** | FR-007: a degraded closure tier or an absent role assigns no scope. An absent role is never treated as `build-tooling`. Mirrors m1276's `None` boot set — unearned exclusion is as dangerous as unearned immunity. |
| IV. Type-Driven Correctness | **PASS** | The role reaches the assignment as the existing `DerivationRole` wire string; the new signal is a closed enum, matched exhaustively, per `feedback_enum_allowlist_falls_out_of_date` (#892 survived ~2 years behind a `matches!` allowlist). |
| V. Specification Compliance | **PASS with a flagged gap** | The classification uses the **native** CDX field, not an annotation. The FR-009 provenance annotation is audited against all three vendored schemas and none has a slot for it (research R5), so it falls under the "finer-grained information the standard does not express" carve-out and must be documented in `docs/reference/sbom-format-mapping.md` with that audit. **The gap**: SPDX 2.3 `BUILD_TOOL_OF` and SPDX 3 `LifecycleScopedRelationship` are native carriers the spec does not name. See Complexity Tracking — this is the one item needing a decision before `/speckit.tasks`. |
| VI. Three-Crate Architecture | **PASS** | New signal on `waybill_common::resolution::ResolvedComponent`; the pass and the emitters in `waybill-cli`. |
| VII. Test Isolation | **PASS** | No test reads the host's `~/.cache/waybill/`. The FR-004a probe is `#[ignore]`d and env-gated, the same posture as m1275's T005/T009. Fixtures use `waybill-fixture-*` names only (#285, #640). |
| VIII. Completeness | **PASS** | No component is dropped. `excluded` keeps all 97 in the document — which is the point: m1035 measured `jq` and `lua` in the `unreferenced` role carrying 6 of one project's 18 CVEs, and R1 rejects the design that would have made `--exclude-scope` able to delete them. |
| IX. Accuracy | **PASS** | FR-002 forbids ever asserting `required` from the role, because `ARTIFACT_FIELDS` includes `buildInputs` and nixpkgs' Haskell builder routes test dependencies through it — `hspec` is `artifact-input` and does not ship. The feature may only add a non-runtime scope. |
| X. Transparency | **PASS** | FR-009 records which mechanism classified each scoped component, and emits nothing where nothing set a scope: absence means "nothing set it", not "set by default". |
| XI. Enrichment | **N/A** | No enrichment; the role is already read from nix's own fields. |
| XII. External Data Source Enrichment | **N/A** | No network. |

**Gate result**: PASS to proceed to Phase 1. One item (R6) is recorded in
Complexity Tracking as needing the user's decision rather than silently
widened or silently dropped.

## Project Structure

### Documentation (this feature)

```text
specs/1277-closure-role-scope/
├── spec.md
├── plan.md                     # this file
├── research.md                 # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/
│   └── role-scope.md           # the role → scope mapping, as a table with its exemption
├── checklists/
│   └── requirements.md
├── measurements/
│   ├── baseline.md             # the pre-feature counts the SCs rest on
│   └── roles-and-scopes.py     # the harness, reproducible offline
└── tasks.md                    # /speckit.tasks
```

### Source Code

```text
waybill-common/src/resolution.rs
    # the new per-component signal + its exhaustive match

waybill-cli/src/
├── resolve/
│   └── closure_role_scope.rs           # NEW — the pass; reads C182 + the boot set
├── cli/scan_cmd.rs                     # one call site, beside propagate_non_runtime_scope
├── generate/
│   ├── cyclonedx/builder.rs            # native scope, unconditional (m112 shape)
│   ├── spdx/annotations.rs             # the FR-009 provenance annotation
│   └── spdx/v3_annotations.rs          # same
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs   # the new catalog row

waybill-cli/tests/
├── closure_role_scope.rs               # NEW — US1/US2/US3, with the FR-005 fixture
└── corpus_harness_195/invariants/checks/
    └── closure_role_scope.rs           # NEW — the invariant asserting FR-001

docs/reference/sbom-format-mapping.md   # the new row + its Principle V audit
docs/reference/sbom-invariants.md       # the new invariant's row + diagnostic
```

## Complexity Tracking

| Item | Why it is here | What it needs |
|---|---|---|
| **R6 — SPDX native carriers** | SPDX 2.3 `BUILD_TOOL_OF` and SPDX 3 `LifecycleScopedRelationship` + `LifecycleScopeType/build` both exist, and Principle V says a native construct MUST be the primary signal where one exists. The spec names only CycloneDX. Emitting new relationships for 97 components changes the relationship graph, which feeds graph completeness, orphan reasons and the I-series invariants — observable output, so widening is a **spec change** by the project's own test, not a plan detail. | A decision. Recommended: keep the spec as written, emit CDX-native plus the FR-009 annotation exactly as m112 does for the same class of claim, and file a follow-up issue citing Principle V so the gap is recorded. Alternatives in research R6. |
| **The exemption cannot be verified on the corpus** | FR-005's population is measured zero, so a corpus assertion would pass whether the rule works or not. | A synthetic fixture engineered so the rule is the only thing preventing the scope, per SC-007. This is m1275's F7 — the test that spec called most important could not fail, because two independent rules protected the same case. |
| **FR-004a cannot run in the ordinary suite** | The probe needs a real closure, which needs a host `nix`; the in-tree fixtures cannot answer it because they were written from the field lists under test. | An `#[ignore]`d env-gated instrument (m1275 T005/T009 posture), with the closure produced where `nix` already runs in CI. |
