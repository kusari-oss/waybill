---
description: "Task list for 1280-nix-closure-edges"
---

# Tasks: the nix closure tier emits its dependency graph

**Input**: Design documents in `/specs/1280-nix-closure-edges/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`
**Issue**: #1346 (follow-up to milestone 1279)

**Tests are included.** SC-004 requires determinism asserted by a test rather
than by inspection, SC-007 requires a cycle fixture nix could not produce,
SC-009 requires a mutation proving the `inputs.drvs` read is load-bearing, and
the repository's gate is the acceptance criterion for every change here.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable — different file, no dependency on an incomplete task
- **[Story]**: US1 … US3, on user-story tasks only

## Path Conventions

Single Rust workspace, three crates. Everything is in `waybill-cli`.

- Reader: `waybill-cli/src/scan_fs/package_db/nix/closure/`
- Pipeline: `waybill-cli/src/cli/scan_cmd.rs`
- Orphan reasons: `waybill-cli/src/generate/orphan_reason.rs`
- Tests: `waybill-cli/tests/`, corpus harness at `waybill-cli/tests/corpus_harness_195/`

---

## Phase 1: Setup (fixtures)

**Purpose**: fixtures covering every row of the `StopRule` table in
`data-model.md`, plus the two cases nix cannot produce. A fixture set covering
only "package depends on package" would leave FR-010's whole point untested.

**⚠️ No fixture may be evaluated by a real `nix`.** On 2026-11-04 that rewrote
a committed `flake.lock` and broke 11 tests in an unrelated file. These are
JSON documents handed to a stub, and the stub is the only `nix` on `PATH`
while a test scan runs.

- [X] T001 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/through_fetch.json`: a root consuming package `A`, where `A` reaches package `B` **only** through a derivation declaring no `pname` (a source fetch). The P1 case — without the pass-through rule, `B` is orphaned
- [X] T002 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/stop_at_package.json`: a member reachable only through a derivation that declares a `pname` and is **not** emitted (no declared-attribute reference, no patches). The FR-011 residual-orphan case, which is the row every "is it connected" test misses
- [X] T003 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/anchor.json`: a root derivation that is **not** itself an emitted component, consuming two packages that are. Asserts FR-004 — without the anchor, measured, every component stays orphaned
- [X] T004 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/cycle.json`: two derivations whose `inputs.drvs` name each other. nix cannot emit this; FR-006's guard must not depend on that
- [X] T005 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/dangling_input.json`: an `inputs.drvs` entry naming a `.drv` absent from the closure's own map (FR-005), which nix does emit
- [X] T006 [P] Add `waybill-cli/tests/fixtures/nix_closure_edges/variants.json`: two derivations collapsing to one `(pname, version)` with overlapping dependencies, so the deduplication in FR-003 has something to deduplicate

---

## Phase 2: Foundational — parse the field and project it

**Purpose**: the reader gains the graph; nothing observable changes. Keeping
this phase free of emission is what lets US1's edge census be attributed to
the projection rather than to the parse.

- [X] T007 Add `inputs.drvs` to the parsed derivation in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`: the **keys only**, as a set of `.drv` paths. Do **not** parse `inputs.srcs` — a `src` is a store path, not a derivation, so it names no node in this graph (`data-model.md`)
- [X] T008 [P] Unit-test the parse in `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`'s test module against T005's fixture: an entry naming a `.drv` outside the closure is dropped, and a derivation with no `inputs` key yields an empty set rather than failing. Use the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard the crate requires
- [X] T009 Create `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs` with the projection per `contracts/projection.md`: one memoized post-order pass, keyed on `.drv` paths, memoizing per derivation the set of nearest stopping descendants. A walk restarted per component is 381 traversals of a 10,342-edge graph (research R2); the memoized pass is O(V+E)
- [X] T010 Implement the `StopRule` in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs` exactly as `data-model.md` tabulates it — stop on a derivation declaring a `pname` **whether or not it is emitted**, pass through one declaring none. The middle row is FR-011's entire population
- [X] T011 Implement the cycle guard in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`: mark before descending, treat an already-marked node as a leaf (FR-006). Bounded by the visited set, not by recursion depth
- [X] T012 Identify the root in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs` as the derivation no other derivation's `inputs.drvs` names, and return `None` when there is none rather than guessing (`data-model.md`'s `ProjectedGraph.root`)
- [X] T013 Carry the projected graph on `ClassifiedClosure` in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs`, alongside the roles and read outcome, with no emitter consuming it yet
- [X] T014 [P] Unit-test the projection in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs` against T001, T002, T004 and T006: the pass-through case connects, the stop-at-package case does not, the cycle terminates, and variant edges deduplicate. Assert on `.drv`-keyed edges here — the PURL-level assertions belong in the integration suite, and this layer must be provable without a scan
- [X] T015 [P] Unit-test determinism in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`: projecting the same closure twice yields an identical edge sequence (SC-004's unit half)

**Checkpoint**: the projection exists and is proven in isolation. No document changes.

---

## Phase 3: User Story 1 — a consumer can see what the closure depends on (Priority: P1) 🎯 MVP

**Goal**: the closure's components are connected to the root and to each other.

**Independent test**: scan a flake with `--nix-closure` and assert the emitted
document contains dependency relationships whose endpoints are closure
components, in all three formats.

### Tests for US1

- [X] T016 [P] [US1] Add the harness to `waybill-cli/tests/closure_edges.rs`, modelled on `closure_dep_fields.rs`: stub `nix` on a per-test `PATH`, per-test `tempfile` output dir, a seeded nixpkgs cache, and helpers reading CDX `dependencies[].dependsOn`, SPDX 2.3 `relationships[]` and SPDX 3 `Relationship` elements
- [X] T017 [US1] Add the control every assertion below needs, in `waybill-cli/tests/closure_edges.rs`, which is also **SC-001**'s assertion: the fixture holds the derivations it claims **and** at least one closure component has an incoming edge, where today none does. A projection producing nothing satisfies every "not wrongly connected" assertion vacuously, and a scan that emitted no closure components satisfies them twice over
- [X] T018 [P] [US1] Test contract property P1 in `waybill-cli/tests/closure_edges.rs` against T001: a member reachable only through a non-package derivation has a path from the root
- [X] T019 [P] [US1] Test contract property P4 in `waybill-cli/tests/closure_edges.rs` against T003: the root derivation's projected edges attach to the **document root component**, and no component is emitted for the root derivation itself (FR-004, FR-004a)
- [X] T020 [P] [US1] Test FR-003 in `waybill-cli/tests/closure_edges.rs` against T006: variant-merged components emit one edge per pair, not one per derivation
- [X] T021 [P] [US1] Test FR-007a in `waybill-cli/tests/closure_edges.rs`: an edge whose target is `build-tooling` carries the **same** relationship type as one whose target is `artifact-input`. The role must not select a type
- [X] T022 [P] [US1] Test FR-007 in `waybill-cli/tests/closure_edges.rs`: the same edge set appears in all three formats through each one's native vocabulary, and **no** `waybill:` annotation carries a dependency edge
- [X] T023 [P] [US1] Test FR-009 in `waybill-cli/tests/closure_edges.rs`: a scan **without** `--nix-closure` emits byte-identical output to one before this change

### Implementation for US1

- [X] T024 [US1] Resolve `.drv`-keyed edges to PURL-keyed `Relationship` values in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`, against a `(name, version) -> purl` map built from the **final** component set. One conversion, one place
- [X] T025 [US1] Wire the edges into `relationships` in `waybill-cli/src/cli/scan_cmd.rs` (**FR-001**), **after** the milestone-1275 identity pass (`closure_identity::reclassify`, around line 4790), because the identity→PURL join needs the final component set. **Research R1's stronger reason is retracted** — it claimed an earlier placement would dangle for 104 of 381 components, and `closure_identity.rs:662` already rewrites relationship endpoints in the same step by the #980/#981 convention. The placement stands on the join, not on a hazard
- [X] T026 [US1] Assert the property, not the placement, in `waybill-cli/tests/closure_edges.rs`: every emitted relationship's endpoints resolve to a component present in the document. **This task survives R1's retraction unchanged, which is the point of having written it this way** — it holds whatever the sequencing and whatever the rename convention, where a test pinning the call site would have encoded a wrong reason
- [X] T027 [US1] Use `RelationshipType::DependsOn` for every edge in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`, and record in the module doc why the role does not select a type: SPDX 2.3 could express `BUILD_TOOL_OF` where CycloneDX cannot, so mapping role to type would make the three formats stop carrying one graph
- [X] T028 [US1] Verify `waybill-cli/src/generate/spdx/relationships.rs` needs no change for m1277's 103 `BUILD_TOOL_OF` edges (FR-007b). If this task edits that file, the confinement argument has broken and SC-010 needs re-checking
- [X] T029 [US1] Record the emitted edge count in the C184 document record in `waybill-cli/src/scan_fs/package_db/nix/closure/summary.rs` (FR-008), so a closure that contributed no edges is distinguishable from one never read — and extend the C184 catalogue row in `docs/reference/sbom-format-mapping.md` to document the new key
- [X] T030 [US1] Extend the C184 parity extractors in `waybill-cli/src/parity/extractors/` **in the same change as T029** — a documented row whose shape changed without its extractor following fails `holistic_parity`

### Remediation added by `/speckit.analyze` (finding E1)

- [X] T057 [US1] Test **SC-008**'s untested half in `waybill-cli/tests/closure_edges.rs`: a scan whose stub `nix` fails records **no** edge-count key in C184, rather than a zero. The C184 row already makes this distinction for every other field it carries — "a zero would read as 'the closure held nothing'" — so a count appearing as 0 on a degraded closure would contradict the row T029 extends

**Checkpoint**: US1 is independently shippable. The graph exists in all three formats.

---

## Phase 4: User Story 2 — the graph stops claiming an incompleteness it caused (Priority: P2)

**Goal**: orphans fall to the genuine residue, and each residual one says why.

**Independent test**: compare the orphan count and the completeness reason
before and after, on one closure, with no other change.

### Tests for US2

- [X] T031 [P] [US2] Test SC-002 in `waybill-cli/tests/closure_edges.rs` against T001: a component connected by the projection is **not** reported as an orphan
- [X] T032 [P] [US2] Test FR-011 in `waybill-cli/tests/closure_edges.rs` against T002: the residual orphan carries `waybill:orphan-reason = reachable-only-through-unemitted-package`, and a component nothing depends on at all does **not** carry that code — the two must stay distinguishable, which is the whole point
- [X] T033 [P] [US2] Test the FR-011 code's exclusions from `contracts/orphan-reason.md` in `waybill-cli/tests/closure_edges.rs`: it appears on no component lacking `waybill:closure-role`, and on no connected component

### Implementation for US2

- [X] T034 [US2] Add the `OrphanReasonCode` variant in `waybill-cli/src/generate/orphan_reason.rs` with wire value `reachable-only-through-unemitted-package`, its `as_str` arm, and its `OrphanReasonCounts` field. The literal is frozen on landing — the enum's own comment says consumers key on these
- [X] T035 [US2] Classify the residual population in `waybill-cli/src/generate/orphan_reason.rs`: an orphan carrying `waybill:closure-role` whose every path runs through a `pname`-declaring derivation this scan did not emit. Measured uniform across all 21 such components on both systems (`contracts/orphan-reason.md`)
- [X] T036 [US2] Confirm `waybill-cli/src/generate/graph_completeness/` needs no change beyond consuming the new edges — the BFS already seeds from the document root, so connected components stop being orphans without a new mechanism. Record that in the module doc as the reason no edge-specific reachability pass exists

### Remediation added by `/speckit.analyze` (finding D1)

- [X] T056 [P] [US2] Test **SC-003** in `waybill-cli/tests/closure_edges.rs`, which no other task covers, in two parts: (1) the `orphaned-components-detected` clause is **absent** from `waybill:graph-completeness-reason` for a closure whose components are all connected; and (2) the CycloneDX `compositions[].aggregate` and SPDX 3 `Relationship.completeness` carriers **agree with each other** and with whatever verdict remains. The two render from one shared helper (`generate/cyclonedx/compositions.rs::dependency_claims`, shared since m1069), so disagreement is a defect rather than a difference — and it is the Principle V half of the criterion, which a test asserting only the orphan count does not reach. Assert the clause's absence against a fixture that **carries** the clause before the change, or the assertion is vacuous

**Checkpoint**: the orphan population is the genuine residue and explains itself.

---

## Phase 5: User Story 3 — the roles become a filter rather than a gate (Priority: P3)

**Goal**: the C182 roles still annotate every endpoint, so a consumer can
partition the graph by role. Needs no new mechanism; stating it keeps the
roles from being mistaken for this feature's job.

- [X] T037 [P] [US3] Test SC-005 in `waybill-cli/tests/closure_edges.rs`: the C182 role census is **identical** before and after the edges exist. A move means `inputs.drvs` has leaked into classification, which FR-002 forbids
- [X] T038 [P] [US3] Test in `waybill-cli/tests/closure_edges.rs` that both endpoints of every emitted closure edge still carry their `waybill:closure-role` annotation, so the graph is partitionable by role without waybill having pre-partitioned it
- [X] T039 [US3] Record in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`'s module doc that the projection never reads a declared dependency attribute, and that this separation is what SC-005 asserts rather than assumes

---

## Phase 6: Polish & cross-cutting

- [X] T040 Test SC-007 in `waybill-cli/tests/closure_edges.rs` against T004: a closure containing a cycle terminates and emits a document. Assert on a **completed scan**, not on the absence of a hang — a test that hangs does not fail, it times out, and a CI timeout reads as infrastructure
- [X] T041 Test SC-009 in `waybill-cli/tests/closure_edges.rs` as a **mutation**: with the `inputs.drvs` read disabled, the US1 tests go red. A graph source no test is sensitive to is not evidence that it is being read
- [X] T042 [P] Test FR-012 in `waybill-cli/src/scan_fs/package_db/nix/closure/graph.rs`'s test module: the projection visits each derivation once. Assert on a visit counter rather than on wall-clock — both measured closures are small enough that 381 full traversals would pass a timing assertion (research R2)
- [X] T043 Update the `nix-closure-moat` expectations in `waybill-cli/tests/corpus_harness_195/expectations.rs`: closure components carry incoming edges where today none does; the five residual orphans carry the FR-011 code; m1277's `BUILD_TOOL_OF` count is still 103 (SC-010); and the role census is unchanged (SC-005)
- [X] T044 [P] Record the before/after edge and orphan census per system in `specs/1280-nix-closure-edges/measurements/README.md`, labelled by system — the two closures populate different attributes and an unlabelled figure is the defect #1348's research shipped
- [X] T045 Run the local gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with **no `+toolchain`** (#1189). Assert on `GATE_RESULT`, the expect line, and the per-target `test result: ok. N passed; 0 failed` lines
- [X] T046 Merge `origin/main` into the branch **before** dispatching the corpus refresh. A branch cut before `48a5115e` cannot print the `corpus-regen-golden: … CHANGED` verdicts, so the run reports `success` and cannot tell you which goldens moved (milestone 1279 finding F12)
- [X] T047 Commit the refresh prediction **before** dispatching it, in `specs/1280-nix-closure-edges/measurements/refresh-prediction.md`: edges 0 → 3,429, closure orphans 352 → 5, the five named, the role census unchanged, `BUILD_TOOL_OF` still 103, and which rows are least certain
- [X] T048 Refresh the corpus goldens through the `public-corpus.yml` workflow dispatch. **Never** set `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` locally
- [X] T049 Score the T047 prediction against the refresh diff in `specs/1280-nix-closure-edges/measurements/refresh-prediction.md`, including any miss and what it corrected
- [X] T050 Confirm the regen log reports `corpus-regen-golden: … CHANGED` for `nix-closure-moat` and **zero** for every other target (SC-006) — a directory upload contributes bytes for targets that never wrote
- [X] T051 Check every target under `waybill-cli/tests/fixtures/public_corpus/` for movement outside `nix-closure-moat`, enumerated **by file** rather than by target name. Note that `image-postgres16` is already stale on main for an unrelated reason (#1358) and will differ; that is not this feature's movement
- [X] T052 Reconcile `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml` with the refreshed goldens: retire what the fix makes stale, declare any new offender with its reason and count
- [X] T053 [P] Dispatch a compare-mode corpus run (`regen_goldens=false`) and confirm the regen count is **0**, so the run is known to have compared rather than rewritten. This is the only execution of the expectations T043 adds — locally the target prints `skipping nix-closure-moat` and reports `ok` in 0.00s
- [X] T054 [P] Record anything this milestone corrected about its own reasoning in `specs/1280-nix-closure-edges/measurements/findings.md`
### Remediation added by `/speckit.analyze` (finding G1)

- [X] T058 Record and assert the `--tier` interaction, in `waybill-cli/src/cli/scan_cmd.rs`'s closure block and in `waybill-cli/tests/closure_edges.rs`: `apply_tier_filter` runs at line 4121 and the closure contributions land at 4728 and 4789, so **closure components and these edges are both added after the filter and are not subject to it**. That is pre-existing for components — `--tier source-only --nix-closure` already emits build-tier closure components — and the edges inherit it exactly rather than introducing it. Assert the behaviour that exists (such a scan still carries closure components and their edges, with no dangling endpoint, since both ends arrive after the filter) and record in a comment that it is inherited rather than chosen. **Do not "fix" it here**: re-running the filter over closure contributions would change which components a `--tier` scan emits, which is a behaviour change to milestone 1035 and has its own issue, #1360
- [ ] T055 Run the `quickstart.md` validation end to end, including the "see the defect" step as a **before** control and all eight traps

---

## Dependencies

```
Phase 1 (T001-T006, all [P])
    ↓
Phase 2 (T007 → T008; T009 → T010 → T011 → T012 → T013; T014, T015 [P])
    ↓
Phase 3 / US1  ← MVP boundary
    T016 → T017 → {T018..T023 [P]}
    T024 → T025 → T026, T057
    T027, T028, T029 → T030
    ↓
Phase 4 / US2 (needs US1's edges to exist)
    T034 → T035 → {T031..T033 [P]}, T036, T056
    ↓
Phase 5 / US3 (needs US1; asserts an absence, so it cannot run earlier)
    T037, T038 [P] → T039
    ↓
Phase 6 (T040..T042 and T058 in any order; then T043 → T045 → T046 → T047 → T048 → T049 → T050 → T051 → T052 → T053)
```

**US2 depends on US1** — there is no orphan reduction to assert until edges
exist. **US3 depends on US1** for the same reason inverted: it asserts the role
census did *not* move, which is only meaningful once something could have moved
it.

## Parallel opportunities

- **Phase 1**: all six fixtures are independent files (T001–T006).
- **Phase 2**: T014 and T015 once T013 lands; T008 alongside T009–T013.
- **US1**: T018–T023 are six independent assertions over one harness.
- **US2**: T031–T033 once T034–T035 land.
- **Polish**: T042 and T044 are independent of the corpus sequence; T053 and
  T054 run alongside each other.

## Implementation strategy

**MVP is US1 alone** — T001 through T030. That delivers the graph in all three
formats, which is the whole of what a consumer asking "what does this depend
on" needs. US2's orphan reasons and US3's filter assertions are both
consequences worth having and neither blocks shipping the graph.

**T025 was billed as the one task to get right first, and that was based on a
finding since retracted** (research R1). Placing edge construction before the
identity pass would **not** have broken the graph: `apply_renames` already
rewrites endpoints alongside identities, by a convention this milestone's
research failed to check for. The placement still stands — the identity→PURL
join needs the final component set — but it is a convenience, not a trap.

**The task that actually matters is T026**, and it mattered before the
retraction too: it asserts that every emitted relationship's endpoints resolve
to a present component, which holds whatever the sequencing and whatever the
rename convention. A test written to pin T025's call site would have encoded
the wrong reason and still passed.
