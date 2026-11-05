# Implementation Plan: the nix closure tier emits its dependency graph

**Branch**: `1280-nix-closure-edges` | **Date**: 2026-11-05 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1280-nix-closure-edges/spec.md`

## Summary

The closure tier emits components and no dependency edges. `nix derivation
show` records the build graph in `inputs.drvs`; waybill parses the *declared*
attributes instead and uses them only to assign a role. This reads
`inputs.drvs`, projects it onto the components the tier already emits — through
non-package derivations, stopping at packages (FR-010) — anchors it on the
document root (FR-004), and emits one uniform dependency edge per projected
pair (FR-007a).

Measured on the pinned x86_64-linux closure: 0 → 3,429 edges, and closure
orphans 352 → 5, with the five carrying a reason that names their cause.

**The finding that shapes the whole plan**: the milestone-1275 identity pass
rewrites **104 of 381** closure components' PURLs from `pkg:generic/…` to
`pkg:hackage/…` *after* `emit::components` returns. Edges built inside the
emitter would therefore reference endpoints that no longer exist for 27% of the
graph — and would pass every fixture test, because no fixture runs that pass.
Edge construction is placed after it, and keyed on derivation paths until then.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by
`rust-toolchain.toml` (1.98.1). No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: existing only — `serde_json` (the `inputs.drvs` map
is already in the parsed document), `std::collections::{BTreeMap, BTreeSet,
HashMap, HashSet}`, `tracing`. **Zero new Cargo dependencies.** No subprocess,
no network, no new flag.
**Storage**: N/A — the projection is an in-process transform over the closure
already in memory, dropped when emission finishes.
**Testing**: `cargo test --workspace --no-fail-fast` plus
`cargo clippy --workspace --all-targets`, run through
`./scripts/pre-pr.sh` with **no `+toolchain`** (#1189). Corpus goldens via
`public-corpus.yml` dispatch, never locally.
**Target Platform**: user-space, all three host OSes. The tier itself needs a
`nix` on `PATH`; this feature adds no requirement beyond that.
**Project Type**: single Rust workspace, three crates; all work in
`waybill-cli`.
**Performance Goals**: the projection stays linear in closure size (FR-012).
The naive shape is 381 full traversals of a 10,342-edge graph; a single
memoized post-order pass is O(V + E) plus the cost of unioning per-node sets,
whose maximum measured size is 47.
**Constraints**: no change to the C182 role census (SC-005), no change to
m1277's 103 `BUILD_TOOL_OF` edges (SC-010), byte-identical goldens for every
non-closure target (SC-006), and byte-identical output for a scan without
`--nix-closure` (FR-009).
**Scale/Scope**: 1,749 derivations and 381 emitted components on the pinned
x86_64-linux closure; 1,275 and 388 on aarch64-darwin. One new orphan reason
code, one extended document-scope record, no new catalogue row — a dependency
edge is native in all three formats.

## Constitution Check

| Principle | Assessment |
|---|---|
| **I. Pure Rust, statically linked** | PASS — no new dependency, no C, no new linkage. |
| **II. eBPF-only observation** | N/A — user-space filesystem/subprocess tier. |
| **III. Fail closed** | PASS — a closure that cannot be read already degrades through `DegradationReason`; a closure missing `inputs.drvs` degrades to today's behaviour (no edges) rather than guessing. |
| **IV. Type-driven correctness** | PASS — edges are the existing `Relationship` with the existing `RelationshipType::DependsOn`; no new stringly-typed field. `clippy::unwrap_used` is denied in this crate, so test modules carry the `cfg_attr` guard. |
| **V. Specification compliance** | **PASS, and it is the point.** A dependency edge is the one construct all three formats model natively: CycloneDX `dependencies[].dependsOn`, SPDX 2.3 `DEPENDS_ON`, SPDX 3 `dependsOn`. FR-007 forbids a `waybill:` annotation, and **no catalogue row is added** — this is native data, not a parity bridge. |
| **VI. Three-crate architecture** | PASS — reader-side logic in `waybill-cli`, no new type in `waybill-common` (`Relationship` and `RelationshipType` already exist). |
| **VII. Test isolation** | PASS — the integration suite uses a stub `nix` on a per-test `PATH` and a per-test `tempfile` output dir, as the m1277/m1278/m1279 suites do. **No fixture is evaluated by a real `nix`** (#1350's second finding). |
| **VIII. Completeness** | **The principle this feature serves.** 381 components currently assert nothing about how they relate; after this, 376 of 381 are reachable from the root and the remaining 5 say why they are not (FR-011). |
| **IX. Accuracy** | PASS, and the reason FR-010 chose projection (c) over (b): full transitive projection would emit 16,187 edges, most of them transitive-closure artefacts asserting `A depends on C` where nix said `A → B → C`. Accuracy is why 5 orphans are preferred to 0. |
| **X. Transparency** | PASS — FR-008 records the edge count in C184, FR-011 gives each residual orphan a reason, and the existing completeness carriers continue to report what is not known. |
| **XI / XII. Enrichment** | N/A — no external data source; `inputs.drvs` is in the document the tier already reads. |

**Gate result: PASS, no violations, nothing to justify in Complexity Tracking.**

## Project Structure

### Documentation (this feature)

```
specs/1280-nix-closure-edges/
├── spec.md                 # 3 clarifications resolved, 0 markers
├── plan.md                 # this file
├── research.md             # Phase 0 — 5 findings, 2 of which changed the design
├── data-model.md           # Phase 1 — the projection's entities and invariants
├── quickstart.md           # Phase 1 — reproduce, verify, and the traps
├── contracts/
│   ├── projection.md       # the FR-010 rule, stated as properties
│   └── orphan-reason.md    # the FR-011 code's wire contract
├── checklists/
│   └── requirements.md     # 16/16
└── measurements/
    ├── README.md           # both systems, four projections, the residual cause
    └── input-graph-probe.py
```

### Source Code (repository root)

```
waybill-cli/src/
├── scan_fs/package_db/nix/closure/
│   ├── derivation.rs       # EXTEND: parse `inputs.drvs` into the raw closure
│   ├── graph.rs            # NEW: the memoized projection (FR-010, FR-012)
│   ├── emit.rs             # UNCHANGED in behaviour; exports the drv→identity map
│   └── mod.rs              # carry the projected graph on ClassifiedClosure
├── cli/scan_cmd.rs         # place edge construction AFTER the m1275 identity pass
├── generate/
│   └── orphan_reason.rs    # EXTEND: one new OrphanReasonCode (FR-011)
└── ...

waybill-cli/tests/
├── closure_edges.rs        # NEW: the integration suite, stub `nix`
├── fixtures/nix_closure_edges/   # NEW: projection, cycle, anchor, residual-orphan
└── corpus_harness_195/expectations.rs   # EXTEND: nix-closure-moat
```

**Structure Decision**: the projection gets its own module rather than growing
`emit.rs`, because the two run at different times — `emit::components` before
the identity pass, the projection after it — and a single module whose halves
must not be called together is how the PURL-rewrite hazard gets reintroduced.

## Complexity Tracking

No constitutional deviations. One design constraint worth recording here
because it is not visible from the requirements:

| Constraint | Why it exists |
|---|---|
| Edge construction is sequenced after the m1275 identity pass and keyed on `.drv` paths until then | The identity pass rewrites 104 of 381 component PURLs. Edges keyed on PURLs at emit time dangle for 27% of the graph, and no fixture would catch it because fixtures do not run that pass. Research R1. |
