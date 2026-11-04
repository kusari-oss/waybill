# Implementation Plan: a closure member's attributes are read wherever nix put them

**Branch**: `1278-closure-structured-attrs` | **Date**: 2026-11-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1278-closure-structured-attrs/spec.md`

## Summary

`nix derivation show` puts a derivation's attributes in one of two places: a
flat `env` map of strings, or a typed `structuredAttrs` object when nixpkgs
sets `__structuredAttrs = true`. waybill reads `env` only. For a derivation
using the typed form every dependency field reads as absent, and absent is
indistinguishable from unreferenced.

The fix is confined to one seam — the four accessors on the parsed derivation
record — which corrects role classification, component emission, patch
collection and the nixpkgs declaration pass at once (FR-007).

Phase 0 changed the disclosure design. The incompleteness fact has native
carriers in two of the three formats, **and the current output natively claims
the opposite**: the `nix-closure-moat` golden asserts CycloneDX
`aggregate: "complete"` over the root's dependencies — "no further
dependencies are known to exist" — in a document whose closure skipped 554
declarations. So the fact goes native (CDX `compositions`, SPDX 3
`Relationship.completeness`, both through the existing shared helper so they
cannot drift), and only the detail no format expresses goes in a new
`waybill:` annotation. See research R1 and R2.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No
nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: existing only — `serde`/`serde_json` (both attribute
shapes are already in the parsed document), `tracing`. **Zero new Cargo
dependencies**; `serde_json::Value` already models the typed object.
**Storage**: N/A — all state in-process per scan, as every closure milestone
since m1034.
**Testing**: `cargo test --workspace --no-fail-fast` plus `cargo clippy
--workspace --all-targets`, via `./scripts/gate.sh --expect '>>> all pre-PR
checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain`
argument** (#1189). Corpus goldens are `#[ignore]`d locally and refreshed only
through `public-corpus.yml` dispatch.
**Target Platform**: all supported hosts. The closure path is opt-in behind
`--nix-closure` and needs `nix` present; the measurements were taken on
aarch64-darwin evaluating both the darwin and the x86_64-linux attribute.
**Project Type**: CLI, single Rust workspace, three crates.
**Performance Goals**: none new. The widening reads a field already parsed
into memory; no additional subprocess, I/O or network.
**Constraints**: emitted output must change **only** where the spec's criteria
say it does (SC-009). Local golden generation is forbidden (FR-010).
**Scale/Scope**: the pinned corpus closure holds 1,749 derivations of which
421 declare dependencies only in the typed form, carrying 554 dependency
entries, 28 patch entries and 11 identities the reader cannot currently see.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| I. Pure Rust, Statically Linked | **PASS** | Zero new dependencies. Both shapes are already in the parsed document. |
| III. Fail Closed | **PASS, with the scan-mode clause doing the work** | FR-013 declines to gate on an unrecognized format version, which would look like a fail-closed violation if III's first paragraph were the whole rule. It is not: III scopes fail-closed to trace mode and states that *in scan mode, Principle X governs — evidence that could not be read is reported, never filled in by guesswork.* Reporting is therefore the constitutional path, and refusing the tier over a version number is not. Recorded here rather than in Complexity Tracking because it is compliance, not an exception. |
| IV. Type-Driven Correctness | **PASS, with an obligation** | The disclosure must be a typed record, not a `String` assembled at the emission site, and no `.unwrap()` in production. The typed attribute object arrives as `serde_json::Value`, so FR-004's "any JSON shape yields no paths" is a total function over it, not a cast that can panic. |
| V. Specification Compliance | **PASS — and it reshaped the design** | The audit is research R1, measured against the vendored schemas, not recalled: CDX 1.6 `aggregateType` has `incomplete`; SPDX 3.0.1 `Relationship.completeness` has `incomplete`; SPDX 2.3 has no such field at all. Native carriers take the fact; the annotation takes only the detail and, for SPDX 2.3, the fact as well because there is nowhere native to put it. |
| VI. Three-Crate Architecture | **PASS** | Everything lands in `waybill-cli`. No change to `waybill-common` or `waybill-ebpf`. |
| VII. Test Isolation | **PASS** | No env-var mutation, no shared fixtures written. The typed-form fixtures are new files; every existing flat-form fixture is untouched and asserted byte-identical (SC-009). |
| VIII. Completeness | **PASS — this is the principle being repaired** | 554 dependency entries, 28 patch entries and 11 identities are currently dropped silently. |
| IX. Accuracy | **PASS** | No component is invented. The 13 newly emitted identities were referenced all along; the fix stops discarding them. |
| X. Transparency | **PASS — this is the other principle being repaired** | X requires structured metadata whenever completeness cannot be guaranteed, *using spec-native mechanisms rather than ad-hoc extensions where possible*. Today a partially-read closure is disclosed nowhere and natively claimed complete. |
| XII.3 External enrichment disclosure | **PASS** | Degraded enrichment is disclosed in the document, not only logged. #1115 was that defect for total closure degradation; FR-009 extends the same rule to partial. |

**Gate result: PASS. No unjustified violations, so Complexity Tracking stays
empty.** The one entry a reader might expect there — FR-013's refusal to
fail closed on an unknown format version — is compliance with Principle III's
scan-mode clause, not a deviation from it.

## Project Structure

### Documentation (this feature)

```text
specs/1278-closure-structured-attrs/
├── plan.md                        # This file
├── spec.md                        # Feature specification
├── research.md                    # Phase 0 — R1..R7
├── data-model.md                  # Phase 1
├── quickstart.md                  # Phase 1
├── contracts/
│   ├── attribute-resolution.md    # The reader-side contract
│   └── read-outcome.md            # The disclosure's wire contract
├── checklists/requirements.md     # Spec quality checklist
├── measurements/                  # Probes + the invocation that produced them
│   ├── README.md
│   ├── role-census-probe.py
│   └── source-identity-probe.py
└── tasks.md                       # Phase 2 — /speckit.tasks, NOT this command
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/closure/
├── derivation.rs     # THE SEAM. paths_in / pname / declared_pname / version
│                     #   gain the typed-form fallback; `version` field captured
├── classify.rs       # unchanged logic; sees the widened reads (FR-001)
├── patches.rs        # unchanged logic; sees the widened reads (FR-002)
├── emit.rs           # unchanged logic; sees the widened reads (FR-003)
└── mod.rs            # threads the read outcome out to emission

waybill-cli/src/generate/
├── cyclonedx/compositions.rs   # dependency_claims gains the partial read as
│                               #   a demotion input (research R2)
└── spdx/                       # reads the same answer via the shared helper

waybill-cli/src/resolve/
└── closure_role_scope.rs       # NOT widened -- read to confirm FR-008 already
                                #   follows from m1277's rule; see research R6

waybill-cli/src/parity/extractors/   # the new row's three extractors
docs/reference/sbom-format-mapping.md # the new row (C204; next free number)

waybill-cli/tests/
├── closure_structured_attrs.rs              # new integration tests
├── fixtures/nix_closure_structured_attrs/   # new typed-form fixtures
├── fixtures/public_corpus/nix-closure-moat/ # refreshed by dispatch only
└── corpus_harness_195/
    ├── expectations.rs                 # corrected census, components, patches
    ├── manifest.rs                     # the mislabelled-census comment
    └── invariant-declarations.toml     # reconciled after the refresh (FR-017)
```

**Structure Decision**: no new modules. The feature is a widening at one
existing seam plus a disclosure threaded through the existing emission path.
`src/resolve/closure_role_scope.rs` is listed because a task reads it, not
because it is widened — the distinction matters, because research R6's
confinement argument is what SC-009's byte-identity claim rests on.
The strongest argument for the seam is FR-007: four consumers must not each
grow their own resolution, because two that resolve differently would disagree
about the same derivation.

## Phase 0 — Outline & Research

**Complete.** `research.md` resolves R1 through R7. Two findings changed the
plan rather than confirming it:

- **R1**: the incompleteness fact has native carriers, verified against the
  vendored schemas, and the current output natively claims the opposite. This
  moves the fact out of the new annotation and into `compositions` and SPDX 3
  `Relationship.completeness`, leaving the annotation the detail.
- **R2**: the demotion lever already exists (`dependency_claims`, shared by CDX
  and SPDX 3 since m1069), but the false claim in the golden is the *root*
  record, which that helper derives from trace integrity rather than from the
  ecosystem path. Both claims need the new input.

No `NEEDS CLARIFICATION` markers remain in Technical Context.

## Phase 1 — Design & Contracts

**Complete.** `data-model.md` carries the entities and the validation rules
each requirement imposes; `contracts/attribute-resolution.md` fixes the
reader-side resolution rule including the precedence and the
not-a-string-list case; `contracts/read-outcome.md` fixes the disclosure's wire
shape, its emission gating, and the native fields it demotes;
`quickstart.md` is the reproduction and verification recipe.

### Post-design Constitution re-check

**Still PASS.** The design added no dependency, no module and no subprocess.
The one thing it added that the pre-Phase-0 reading did not anticipate —
touching the native `compositions` and SPDX 3 completeness fields — moves the
design *toward* Principle V rather than away from it, and routes both formats
through the one helper that already keeps them in step.

## Phase 2 — Task planning approach

Not executed here; `/speckit.tasks` produces `tasks.md`. The intended shape,
so the task breakdown is not invented from scratch:

- **Setup**: new typed-form fixtures, built from the real shapes in the
  measured closure rather than hand-invented, including the awkward ones
  (empty list, boolean-valued field, nested object, single string).
- **Foundational, blocking everything**: the seam in `derivation.rs`, the
  format version captured, and the read-outcome record produced. Nothing
  observable changes until the outcome is threaded out.
- **US1 (P1)**: role correctness and the scope retraction. Ends with the nine
  named components asserted.
- **US2 (P2)**: patch evidence.
- **US3 (P2)**: typed-only identities.
- **US4 (P3)**: the disclosure — native demotion, the new row, three
  extractors, parity — plus the guard and its mutation test.
- **Polish**: the corpus golden refresh by dispatch, the census record under
  FR-011, and the mislabelled-census correction.

Two sequencing constraints worth stating now, because getting them wrong costs
a golden refresh each: the catalog row and its extractors land together (a
documented row nothing emits fails the catalog's own completeness test), and
the refresh comes last, after every output-changing task.
