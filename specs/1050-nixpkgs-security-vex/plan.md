# Implementation Plan: nixpkgs security declarations as VEX

**Branch**: `1050-nixpkgs-security-vex` | **Date**: 2026-09-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1050-nixpkgs-security-vex/spec.md`

## Summary

Read the security metadata nixpkgs declares about the packages a build
actually contains, and emit it as VEX beside the patch evidence milestone 1035
already produces — so a consumer can tell "nixpkgs says this is insecure" from
"this build patched it" from "nobody has said anything".

`meta` is not in the closure data (measured: zero of 1,275 derivations carry
it), so the approach is a second evaluation against the project's own pinned
nixpkgs, resolving each member's `pname` across an ordered list of package
sets and **verifying by output path** before trusting what it finds. That
verification is the load-bearing part: 9% of members resolve to an attribute
that builds something else, and accepting those would attach security claims
to the wrong components.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `regex` (CVE
extraction; already a direct dep), `tracing`, `anyhow`/`thiserror`. Reuses
milestone 1034's `nix::eval::invoke` subprocess helpers and
`DegradationReason`, and milestone 1035's `closure::derivation::store_basename`,
`EvidenceGrade`, and OpenVEX `subcomponents`. **Zero new Cargo dependencies.**
**Storage**: N/A — in-process per scan. Nothing persists.
**Testing**: `cargo test --workspace`; unit tests beside each module; an
integration fixture that deliberately permits an insecure package (R5).
**Target Platform**: Any host with `nix`; degrades cleanly without it.
**Project Type**: CLI (single Rust workspace).
**Performance Goals**: The declaration pass adds ≤ 20% to `--nix-closure`
wall clock (SC-006a). Measured at 0.6–1.1 s against a scan measured in tens of
seconds — an order of magnitude inside the budget.
**Constraints**: One evaluation for the whole closure, not one per member. No
network beyond what `--nix-closure` already does. No external advisory
lookups (FR-018).
**Scale/Scope**: ~380 distinct members on a measured closure; 376 distinct
names evaluated in one expression.

## Constitution Check

*GATE: passed before Phase 0; re-checked after Phase 1 design. No violations.*

| Principle | Bearing | Verdict |
|---|---|---|
| **I. Pure Rust, statically linked** | No new crates; `nix` is a subprocess as in m1034/m1035. | PASS |
| **III. Fail closed** | Trace-scoped; this is a scan-side enrichment. Degrades with a named reason (FR-019) rather than failing. | PASS |
| **IV. Type-driven correctness** | `AttributeResolution` makes "unchecked" a distinct state from "no declaration" at the type level rather than by convention; `GradedCve` already refuses to exist without its grade. | PASS |
| **V. Specification compliance** | Native-carrier audit in research R6. OpenVEX carries the statements natively; three signals have no native carrier and become parity-bridge rows with the audit recorded. | PASS |
| **VIII. Completeness** | Coverage is emitted as a number (FR-001d), not assumed. | PASS |
| **IX. Accuracy** | The whole point of FR-001b. Ambiguous matches are *rejected*, not silently included — 35 of 380 measured. Directly serves "ambiguous or low-confidence matches MUST be flagged rather than silently included". | PASS — strongest alignment |
| **X. Transparency** | Unchecked counts, withheld-reconciliation counts, and the evidence grade are all emitted. Absence is never left to mean two things. | PASS |
| **XI. Enrichment** | Names VEX as an enrichment target explicitly. Enrichment must not delay generation to failure — FR-019. | PASS |
| **XII. External-source enrichment** | Constraint 1 (no new components) — this introduces **none**, only statements about existing ones. Constraint 2 (provenance) — FR-007. Constraint 3 (degrade) — FR-019. Constraint 4 (trace authoritative) — declarations add context, never components. | PASS |
| **Boundary 1: no lockfile-based discovery** | nixpkgs is read for enrichment only; no component originates here. | PASS |
| **Boundary 4: no `.unwrap()` in production** | Enforced by the existing crate-root deny. | PASS |

**Post-design re-check**: the Phase 1 design adds no component, no crate, and
no new execution mode. The three new annotations each carry a Principle V
audit. Verdict unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1050-nixpkgs-security-vex/
├── plan.md              # This file
├── spec.md              # Requirements, 32 FR / 13 SC, 4 clarifications
├── research.md          # Phase 0 — R1..R7, all measured
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   ├── evaluation.md    # how declarations are reached
│   └── emission.md      # what is emitted, and where
├── measurements/        # committed probes + findings
└── checklists/
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nix/
├── closure/
│   ├── patches.rs            # EXTEND: EvidenceGrade gains NixpkgsDeclared
│   └── …                     # store_basename, emit, summary — reused as-is
└── declarations/             # NEW module
    ├── mod.rs                # orchestration, AttributeResolution, summary
    ├── resolve.rs            # pname → candidate across ordered package sets
    ├── evaluate.rs           # the single-expression eval + its two guards
    └── parse.rs              # Declaration, CVE extraction, prose split

waybill-cli/src/generate/
├── openvex/mod.rs            # EXTEND: declaration statements + reconciliation
├── cyclonedx/metadata.rs     # EXTEND: doc-scope annotations
└── spdx/{annotations,v3_annotations}.rs   # EXTEND: same, both formats

waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs   # new rows
docs/reference/sbom-format-mapping.md                        # C186..C189

waybill-cli/tests/
├── fixtures/nix_declarations/     # NEW: permits an insecure package (R5)
└── nixpkgs_declarations.rs        # NEW: integration
```

**Structure Decision**: a sibling `declarations/` module under `nix/`, not an
extension of `closure/`. The two degrade independently — a closure can resolve
while the declaration pass fails — and the spec requires that be visible
(FR-019). Folding them together would make one summary carry two failure
modes and invite the same conflation the unchecked-versus-no-declaration
distinction exists to prevent.

## Phasing

Ordered so each phase is independently verifiable, and so the riskiest
measured assumption is exercised first.

| Phase | Delivers | Gate |
|---|---|---|
| **1. Reach** | `declarations/` — evaluate, resolve, verify by path. No emission. | Coverage on moat matches R1: ~71% confirmed, ~9% rejected. If it does not, the plan is wrong before anything depends on it. |
| **2. Fixture** | A project that permits an insecure package, synthetic names. | The path R5 says no public project reaches is exercised at all. |
| **3. VEX** | CVE-bearing declarations → statements (US1), grade extended. | SC-001, SC-002, SC-002a. |
| **4. Reconcile** | FR-012 withholding + count (US2). | SC-005, SC-005a — both halves, since asserting only the suppression passes if the pedigree entry was dropped too. |
| **5. Prose + acceptance** | Per-component annotation (US3), acceptance record (US4). | SC-003. |
| **6. Parity + docs** | C186–C189 with extractors in the same change; docs; goldens. | Catalogue gate both directions; zero golden churn without the flag. |

Phase 1 before everything because R1's 71% is the number the feature's value
rests on, and it was measured with a throwaway script rather than the shipped
code. If the Rust path disagrees with the probe, that is worth knowing before
five phases are built on it.

## Risks

| Risk | Mitigation |
|---|---|
| The Rust implementation's coverage disagrees with the probe's 71%. | Phase 1 gates on reproducing it. The probe is committed, so the two are comparable directly. |
| The two eval guards are omitted and the pass dies on the first unfree package. | Both are in `contracts/evaluation.md` with the failure each one prevents, and in the committed probe's comments. |
| Store-path prefixes compared raw → 0% coverage, read as "mechanism broken". | R4 documents it; `store_basename` already exists and the contract mandates it. |
| The feature produces nothing on every real project and looks broken. | R5: structural, not a bug. Coverage annotation distinguishes "nothing to say" from "did not run"; quickstart leads with it. |
| Evaluation cost proves material on a larger closure than moat. | SC-006a sets the threshold that reopens FR-020a rather than absorbing the cost silently. |

## Complexity Tracking

No constitution violations. Nothing to justify.
