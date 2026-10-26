# Implementation Plan: transitive propagation of non-runtime lifecycle scope

**Branch**: `1269-transitive-dev-scope` | **Date**: 2026-10-25 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1269-transitive-dev-scope/spec.md`

## Summary

Readers assign dev/optional/build/test scope from what their lockfile
tells them, and nothing propagates it across the graph, so a transitive
dependency of dev tooling is emitted with a real coordinate, a real
version and nothing marking it non-production. Five corpus targets fail
invariant I3 for this reason today (`python-flask` 51,
`haskell-language-server` 26, `haskell-security-advisories` 13,
`nix-closure-moat` 9, `rust-ripgrep` 2), spanning uv/pypi, cabal, nix
and cargo.

The fix is one emit-time pass: a BFS establishing which components are
runtime-reachable over declared edges (absolutely immune), then a
strongest-first BFS from each declared non-runtime scope's members
assigning that scope to unscoped, non-immune components. Strictly
additive — a reader-assigned scope is never changed — so an ecosystem
whose lockfile already stamps every transitive is byte-identical by
construction. Components the pass marked carry one new annotation
(C62, a third value on an existing open enum — corrected at
implementation time from a planned new row C199; see
`contracts/scope-derivation-annotation.md`) so a consumer can tell
inference from declaration.

Research changed two things the spec had assumed. FR-002a (anchoring
edges must not confer immunity) needs no provenance filtering: every
anchoring call site is inside a format builder, downstream of the pass,
so the property holds by placement and is enforced by an assertion.
And edge re-typing does not require moving
`apply_lifecycle_scope_to_edges` — it is idempotent by design, so the
caller simply runs it again.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `std::collections::{HashMap, HashSet, VecDeque, BTreeMap}`, `serde_json` (annotation value), `tracing` (one INFO summary). **Zero new Cargo dependencies** (FR-016).
**Storage**: N/A — the pass is a pure in-memory transform over the component and relationship slices; nothing persists beyond the emitted SBOM.
**Testing**: `cargo test --workspace --no-fail-fast` + `cargo clippy --workspace --all-targets`, via `./scripts/pre-pr.sh` under `scripts/gate.sh`, no `+toolchain` argument (#1189). Corpus invariant lane and goldens via `public-corpus.yml` dispatched with `-f branch=` (#1223).
**Target Platform**: All host platforms waybill supports; no platform-specific code.
**Project Type**: CLI / library — single Rust workspace, three crates.
**Performance Goals**: None specified, deliberately. The largest corpus target is 399 components; the pass is one immunity BFS plus at most four seeded BFS passes, each `O(V+E)`. A quoted number here would describe nothing measured (research R10).
**Constraints**: Strictly additive (FR-003); byte-identical output where no component's scope changes (FR-013); no network, no subprocess, no filesystem writes.
**Scale/Scope**: 17 corpus targets, 45–399 components each; 60 reader files touch `lifecycle_scope` today and none of them change.

## Constitution Check

*GATE: evaluated before Phase 0, re-evaluated after Phase 1. Constitution v4.0.1.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS. Zero new dependencies; std collections only. |
| **III. Fail Closed** | PASS, and load-bearing. P2 in the algorithm contract is an assertion, not a tolerance: if an anchoring edge ever reaches the pass, it fails rather than silently immunizing the population the feature exists to mark. FR-012 likewise declines to act where immunity cannot be established, rather than guessing. |
| **IV. Type-Driven Correctness** | PASS. The lattice is an exhaustive `match` in `LifecycleScope::strength()`, so a sixth variant fails to compile. Explicitly *not* `#[derive(Ord)]` (binds order to declaration order) and *not* a `matches!` predicate — the documented local failure mode (#1246, #892). |
| **V. Specification Compliance** | PASS with a KEEP-NO-NATIVE finding for C62, audited against the vendored schemas in `waybill-cli/tests/fixtures/schemas/` with quotations recorded (research R6). CDX `evidence.identity[].field` cannot describe a scope claim — `scope` is absent from its enum; SPDX 2.3 relationships carry only `comment`; SPDX 3's `RelationshipCompleteness` grades a relationship set, not a derivation. The scope *itself* continues to use the native constructs (`component.scope`, `*_DEPENDENCY_OF`, `LifecycleScopedRelationship.scope`) — FR-006 keeps propagation on the existing variants precisely so this stays true. |
| **VI. Three-Crate Architecture** | PASS. `waybill-common` gains one accessor on an existing type; the pass lives in `waybill-cli`. |
| **VII. Test Isolation** | PASS. No env-var mutation, no shared fixtures, no network. The positive control is a hand-built in-memory graph. |
| **VIII. Completeness** | PASS. Nothing is dropped. The pass adds classification; it never removes a component. |
| **IX. Accuracy** | PASS, and this is the principle the feature serves in both directions. Today's output over-reports the runtime surface (dev tooling reads as shipping). The failure mode of a careless fix is the inverse — demoting a component that ships — which is why FR-002 immunity is absolute, FR-002b withholds propagation from heuristic edges, and SC-003 demands a mutation-demonstrated positive control. |
| **X. Transparency** | PASS. C62's `transitive` value is required by this principle, not merely permitted by it: a scope waybill inferred and a scope a reader read are otherwise indistinguishable, and a consumer cannot assess data it cannot tell apart. |
| **XI / XII. Enrichment** | N/A. No external data source; no network. |

**Gate result**: PASS, no violations, Complexity Tracking not required.

## Project Structure

### Documentation (this feature)

```text
specs/1269-transitive-dev-scope/
├── plan.md                              # This file
├── spec.md                              # 20 FRs, 8 SCs, 4 clarifications
├── research.md                           # Phase 0 — R1..R10
├── data-model.md                          # Phase 1
├── quickstart.md                          # Phase 1 — five checks, cheapest first
├── checklists/requirements.md             # spec quality
├── contracts/
│   ├── propagation-algorithm.md           # pre/postconditions, termination, degenerate inputs
│   └── c199-annotation.md                 # wire shape in three formats + native audit
└── tasks.md                               # Phase 2 — /speckit.tasks, NOT created here
```

### Source Code (repository root)

```text
waybill-common/src/
└── resolution.rs                   # LifecycleScope::strength() — the lattice (FR-004)

waybill-cli/src/
├── resolve/
│   └── scope_propagation.rs        # NEW — the pass (FR-001..FR-005, FR-011, FR-012)
├── scan_fs/
│   └── mod.rs                      # apply_lifecycle_scope_to_edges -> pub(crate) (FR-010)
├── cli/
│   └── scan_cmd.rs                 # call site: after the m1035 nix-closure merge
│                                   #   (T038), with --exclude-scope moved BELOW it
│                                   #   as apply_exclude_scope_filter (T043)
└── parity/extractors/
    ├── mod.rs                      # (no change — C62 is already registered)
    ├── cdx.rs                      # c199_cdx
    ├── spdx2.rs                    # c199_spdx23
    └── spdx3.rs                    # c199_spdx3

docs/reference/
└── sbom-format-mapping.md          # C62 enum entry (FR-009, FR-017)

waybill-cli/tests/corpus_harness_195/
└── invariant-declarations.toml     # five I3 entries DELETED (FR-014)
```

**Structure Decision**: the pass is a new module under
`waybill-cli/src/resolve/`, beside `reconciler.rs`, because it is a
whole-graph transform over the resolved set rather than a reader
concern or an emitter concern. It is invoked once from `scan_cmd.rs` at
the point identified in research R1 — after the component set is final
and before the three format builders, which is what makes the formats
agree without three implementations. `waybill-ebpf` is untouched.

## Phase ordering

Four slices, each independently verifiable, mapped to the spec's
stories. The ordering is forced by one thing: **US2's guard must exist
before US1's propagation is switched on**, because a pass that
over-propagates passes every US1 check while demoting components that
ship.

| phase | content | stories | gate |
|---|---|---|---|
| 1 | `LifecycleScope::strength()` + the immunity BFS + the mixed-reachability positive control, with its mutation | US2 (P1) | the control fails when the immunity check is deleted |
| 2 | Phase B propagation, the C62 marker, the re-typing call | US1, US3 (P1/P2) | check 1 in quickstart reads 0 on four targets |
| 3 | C62 enum entry + the native audit record (the three extractors already exist) | US3 (P2) | `every_catalog_row_has_an_extractor`, `holistic_parity` |
| 4 | Delete the five I3 declarations; refresh goldens via the corpus lane; write the per-ecosystem record | US1, US4 | corpus lane green with no declaration re-added; `npm-express` byte-identical |

Golden regeneration belongs to phase 4 and to the corpus lane, never to
a local machine.

## Risks

| risk | why it matters | mitigation |
|---|---|---|
| Over-propagation demotes a shipping component | Under-reporting. Invisible in orphan counts, invisible in I3 counts, strictly worse than the bug | FR-002 absolute immunity; phase 1 before phase 2; SC-003's mutation-demonstrated control |
| Anchoring moves upstream later | Would silently immunize the orphaned non-runtime closure and quietly un-fix the feature | P2 assertion in the algorithm contract fails loudly |
| A heuristic cross-ecosystem edge propagates a scope | Demotion with no declared relationship behind it | FR-002b excludes both `cross-ecosystem-bridge` data types from both traversals |
| Golden churn hides a second change | "Churn to regenerate" is how a regression gets committed | Every changed golden must be explained by a propagated scope or a C62 derivation marker (SC-007); normalized, sorted diffs |
| `--exclude-scope` behaviour change goes unremarked | An operator's output legitimately gets smaller | Changelog entry with the measured per-target numbers, not the predicted behaviour: the prediction was wrong on `haskell-language-server` (measurements/t043) |
| The filter's position is asserted rather than measured | A position claim reads like a code fact and is an empirical one; it was wrong twice here | `apply_exclude_scope_filter` logs `dropped_inferred`, which reads 0 if the call moves back above the pass; `measurements/t043-exclude-scope-ordering.md` holds the before/after and the probe |
