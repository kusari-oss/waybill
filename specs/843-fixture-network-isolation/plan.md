# Implementation Plan: A scan of this repository must not depend on network reachability

**Branch**: `843-fixture-network-isolation` | **Date**: 2026-09-12 | **Spec**: [spec.md](./spec.md)
**Input**: Issue #843

## Summary

Scanning this repository spends ~4.1 seconds of a ~5.3 second floor
reaching the Go module proxy for fixture modules that can never
resolve, and the cost varies with network conditions badly enough that
measurements taken on it are unreliable. Three wrong performance
figures during milestone 839 came from this, and two more during this
feature's own clarification.

The fix is to give each fixture module a local `replace` directive, so
the toolchain answers from inside the tree instead of reaching out.
Measured: 0.01s either way, whether the replacement target exists (the
module resolves) or does not (it fails locally). No production code
changes.

Research overturned three assumptions on the way, all recorded rather
than quietly corrected: cargo is not a contributor, renaming the
fixture domain does not help, and the golden suite was never affected
because it already runs `--offline`.

## Technical Context

**Language/Version**: Rust stable — but no Rust changes are expected.
The work is fixture manifests (`go.mod`), one guard test, and docs.
**Primary Dependencies**: None new. The Go toolchain is already an
assumed prerequisite for development.
**Storage**: N/A.
**Testing**: `cargo +stable test --workspace`, plus a new guard that
fails when a fixture module can reach the network.
**Target Platform**: Developer machines and CI runners. Nothing
platform-specific.
**Project Type**: Test-fixture and developer-experience work. Not a
product feature; no emitted-format change.
**Performance Goals**: SC-001/SC-002 — two consecutive scans within 20%
of each other, and within 20% of a no-network scan. Today: 0.50s
offline against 5.26s networked, i.e. 10× apart.
**Constraints**: FR-009 — real-module resolution behaviour unchanged.
FR-006 — self-containment must survive `git archive`, which is how the
benchmark and corpus harnesses obtain the tree.
**Scale/Scope**: 21 of 27 Go fixture manifests declare unresolvable
modules; 2 `go.work` files; 3 test files reference the fixtures by path.

## Constitution Check

*GATE: must pass before Phase 0. Re-checked after Phase 1.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS. No dependency change; nothing links. |
| **II. eBPF-Only Observation** | N/A. |
| **III. Fail Closed** | PASS, and improved. A fixture whose local target is missing fails immediately and visibly (FR-007) instead of degrading into a network lookup that also fails — the current behaviour, which hides the breakage behind a slow timeout. |
| **IV. Type-Driven Correctness** | N/A. No Rust changes expected. |
| **V. Specification Compliance** | PASS. No emitted format changes. |
| **VI. Three-Crate Architecture** | PASS. Fixtures under `waybill-cli/tests/`. |
| **VII. Test Isolation** | PASS, and this is the principle the feature serves. A test whose result depends on network reachability is not isolated; that is precisely the defect. |
| **VIII–XII** | N/A. No emission or enrichment behaviour changes. |

**Strict Boundaries**: none engaged.

**Gate result: PASS.** Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/843-fixture-network-isolation/
├── spec.md          # complete; 2 clarifications + a measurement correction
├── plan.md          # this file
├── research.md      # R1–R6, complete
├── data-model.md    # fixture module, replacement, resolution attempt
├── quickstart.md    # how to add a Go fixture without reintroducing this
└── contracts/
    └── fixture-isolation-guard.md   # what the regression guard must detect
```

### Source (repository root)

```text
waybill-cli/tests/fixtures/**/go.mod      # MODIFIED — add `replace` directives
waybill-cli/tests/fixtures/**/go.work     # REVIEWED — 2 files; `use` already
                                          #   makes members mutually resolvable
waybill-cli/tests/fixture_network_guard.rs # NEW — FR-008 regression guard
docs/development/                          # NEW — the FR-005 record and the
                                          #   how-to-add-a-fixture note
```

**No production source changes are planned.** FR-009a permits one if a
measurement demands it; the measurements do not. If implementation
finds otherwise, that is a finding to report, not a licence to widen.

## Phase sequencing

1. **Classify every Go fixture** as incidental or deliberate, and record
   it (FR-005). Research found the deliberate set empty today; the
   classification is still written down so the next contributor does
   not re-derive it.
2. **Add `replace` directives**, preferring a *missing* target wherever
   a golden covers the fixture (R5) — that keeps the failure and the
   annotation while removing the network attempt, so no golden churns.
   Use a real local target only where a test wants resolution to
   succeed.
3. **Re-measure the floor**, three samples per arm, against SC-001 and
   SC-002. This is where the work is validated, and it comes before the
   guard so the guard is written against an achieved state rather than
   an intended one.
4. **Add the regression guard** (FR-008), and prove it fails by
   reintroducing a network-reachable fixture.
5. **Decompose or document the residual** (FR-001b). Research attributes
   it to subprocess volume — 80 `go` invocations, 27 of them `go
   version`. Confirm and file separately rather than absorbing it here.
6. **Write the how-to** so the next Go fixture is born self-contained.

Step 3 before step 4 is deliberate. A guard written first would be
tuned until it passed, which is how a guard ends up asserting what the
code does rather than what it should.

## Complexity Tracking

No constitutional violations. Table intentionally empty.
