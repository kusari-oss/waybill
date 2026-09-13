# Implementation Plan: multi-main-module root override

**Branch**: `860-multi-main-module-override` | **Date**: 2026-09-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification for issue #863

## Summary

An operator-supplied root currently deletes every main-module component
from the SBOM. On a workspace that is most of the inventory — 16 of 61
components on maven-guice, 10 of 68 on rust-ripgrep, 4 of 109 on
python-flask including `pkg:pypi/flask@3.1.2` itself — and the dependency
edges pointing at those components are not removed with them, so
fourteen references across two targets resolve to nothing.

Retain the components instead, demoted, keeping their own edges, with
the root depending on each of them. One policy at every N.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain; no nightly).
**Primary Dependencies**: Existing only — `serde_json`, `tracing`. No new crates.
**Storage**: N/A — in-process during a single emission.
**Testing**: `cargo +stable test --workspace`, plus the public-corpus lane for the three affected targets.
**Target Platform**: All supported hosts; emission is host-independent.
**Project Type**: Single Rust workspace (three crates).
**Performance Goals**: None. This changes which components are emitted, not how many are scanned.
**Constraints**: Must not alter scans with zero main modules (SC-005); must not change root *selection*.
**Scale/Scope**: One shared helper, three emitter call sites, one parity catalog row, three corpus targets.

No NEEDS CLARIFICATION remain — the spec resolved five across two sessions.

## Constitution Check

| Principle | Assessment |
|-----------|------------|
| **I. Pure Rust, Statically Linked** | PASS — no new dependencies. |
| **III. Fail Closed** | PASS — the change removes a silent failure (components vanishing with no diagnostic). R5 adds an INFO line where a no-op notice used to be. |
| **IV. Type-Driven Correctness** | PASS — `DropOrDemoteResult` field renamed so its type name matches its inverted meaning rather than carrying a stale one. |
| **V. Specification Compliance** | PASS with obligation — C102's catalog row describes edge topology that FR-007 changes; the row MUST be updated in the same change (R2). The `every_catalog_row_has_an_extractor` gate will not catch a stale description, so this is a listed task, not a gated one. |
| **VIII. Completeness** | PASS, and the point — restores 30 components across three targets and removes 14 unresolvable references. |
| **IX. Accuracy** | PASS — retained components keep their ecosystem-derived identity unchanged (C-2.2). |
| **X. Transparency** | PASS — R5 replaces the removed no-op diagnostic with a retention diagnostic carrying the count. |
| **VII. Test Isolation** | PASS — synthetic tests for N=1 and FR-011; no network. |

**Gate result: PASS.** One obligation carried into tasks (C102 row).

### Supersession notice

This feature overrides two prior deliberate decisions. Both are recorded
in the spec with reasoning, and a reviewer who worked on milestone 149
should see them:

1. **m149's N>1 fall-through** (`root_selector.rs:525`) — the deferral
   this feature exists to close.
2. **m149 US1 Option A** (recorded 2026-06-29) — that a demoted entry
   has no outbound edges. FR-007 reverses it.

## Project Structure

### Source (existing files, no new modules)

```
waybill-cli/src/generate/
├── root_selector.rs                 # the policy — all behaviour change lands here
├── cyclonedx/builder.rs:591         # call site + root→module edges
├── spdx/document.rs:425             # call site + relationship emission
└── spdx/v3_document.rs:65, 318-324  # call site + the alias under C-6
waybill-cli/src/parity/extractors/mod.rs:438   # C102 registration (unchanged)
docs/reference/sbom-format-mapping.md:147      # C102 description (MUST change)
waybill-cli/tests/fixtures/public_corpus/      # 3 targets regenerate
```

## Phase 0 — Research

Complete. See [research.md](research.md). Six findings; the two that
shape the plan:

- **R2** — the parity row already exists and documents the behaviour
  being superseded, so the change is a doc edit, not a new row.
- **R3** — SPDX 3's PURL alias exists to serve re-anchoring. Removing
  re-anchoring may make it unnecessary, which would close a divergence
  m149 deferred. To be **verified, not assumed**.

## Phase 1 — Design

Complete: [data-model.md](data-model.md),
[contracts/root-override-policy.md](contracts/root-override-policy.md),
[quickstart.md](quickstart.md).

## Phase 2 — Implementation approach

Ordered so each step is independently verifiable:

1. **Helper first.** Change `apply_main_module_drop_or_demote` to retain
   and demote at every N; rename the result field. Unit-testable without
   touching an emitter.
2. **Root→module edges** at each of the three call sites, from the
   renamed set.
3. **FR-011 collision** handling, with a synthetic test — no corpus
   target covers it.
4. **N=1 convergence** test through the real emitter path, not a
   hand-assembled vector (R6).
5. **C-6 verification** — determine whether the SPDX 3 alias is still
   needed; remove or re-document accordingly.
6. **C102 row** updated to match emitted reality.
7. **Goldens** regenerated through CI for the three affected targets,
   diffs attributed, eight unaffected targets proven byte-identical.

## Risks

| Risk | Mitigation |
|------|------------|
| The eight zero-main-module targets drift unexpectedly | SC-005 asserts byte-identity; the corpus diff makes any drift visible before merge |
| Removing the SPDX 3 alias breaks a path this feature does not touch | C-6.2 requires verification before removal; keep and re-document if in doubt |
| A regression test that passes without the fix | R6 — teeth-check every new test by reverting the change, per the milestone-856 lesson |
| Golden churn hides a real regression | Three targets, attributed per the documented procedure |
