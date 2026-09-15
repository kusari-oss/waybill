---

description: "Task list for 866-go-graph-completeness"
---

# Tasks: Go scans assert dependency edges they never read, and then report the result as complete

**Input**: Design documents from `/specs/866-go-graph-completeness/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks ARE included. The spec requires them explicitly — FR-009 mandates build-time detection, and SC-006 requires the gate be proven by reverting the fix and observing failure rather than by inspection.

**Organization**: Grouped by user story. US1 and US2 are both P1 and ship together (research R5); neither is safe alone.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 / US4 per spec.md
- Exact file paths included

## Path Conventions

Rust workspace: `waybill-cli/src/`, `waybill-common/src/`, `waybill-cli/tests/`, `xtask/`. No new modules or crates — every edit lands in an existing file (plan.md § Structure Decision).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the before-state so every later claim has a baseline to move from.

- [X] T001 Rebuild the release binary at the current commit via `cargo build --release -p waybill --bin waybill` and record its SHA in `specs/866-go-graph-completeness/measurements/README.md`. A stale binary is how the first spec draft came to describe a product state that no longer existed
- [X] T002 [P] Capture the cold-cache baseline for `go-cobra` per `quickstart.md` §1; confirm `measurements/probe_edge_truth.py` exits 1 with 2 unbacked edges
- [X] T003 [P] Capture the warm-cache control for `go-cobra` per `quickstart.md` §2; confirm `measurements/probe_edge_truth.py` exits 0. This row must not move for the rest of the milestone
- [X] T004 [P] Capture the `kubernetes` cold-cache baseline per `quickstart.md` §4 and record the unbacked count in `specs/866-go-graph-completeness/measurements/README.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The declaring-source index is the authority for FR-001/FR-002 and is consumed by both US1 (emission) and US4 (the gate).

**⚠️ BLOCKS all user stories.**

- [ ] T005 [DEFERRED: superseded by T010 — see deviation note] Implement `DeclaringSourceIndex` per `data-model.md` in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`, parsing every `go.mod` in the scanned tree: `require (...)` blocks, single-line `require x v1`, and `// indirect` entries recorded but NOT emitted as direct edges — see the corrected contracts/edge-backing.md C-2
- [ ] T006 [DEFERRED: folded into T009; `replace` now requires a matching DIRECT require] Resolve `replace` directives on both sides in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` so a requirement declared against a pre-replace path backs an edge emitted against the post-replace path (contracts/edge-backing.md C-3)
- [ ] T007 [DEFERRED: not reached — no vendored corpus target measured yet] Add `vendor/modules.txt` as a backing source in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`, used when the tree is vendored
- [ ] T008 Unit-test the index in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` `#[cfg(test)]` (guard with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the crate-root deny): block and single-line requires parse; `// indirect` backs an edge; `replace` redirects both ways; **`go.sum` never backs an edge**; a `replace`-only entry with no matching `require` does NOT back an edge (the measured `k8s.io/api -> k8s.io/streaming` case)

**Checkpoint**: The index answers "does the tree declare X requires Y?" before any edge is removed.

---

## Phase 3: User Story 1 — Every emitted edge is one waybill actually read (Priority: P1)

**Goal**: No emitted dependency relationship lacks a declared requirement backing it. Satisfies FR-001, FR-002, FR-002a, FR-002b, FR-007, FR-007a, FR-012.

**Independent Test**: Scan a Go project with no reachable module cache; confirm every emitted relationship is declared in some manifest in the scanned tree. Requires no completeness change.

### US1a — Stop asserting unread edges

- [X] T009 [US1] Remove the unbacked-edge sources in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`. **Research R1 said one site; there are three** — all three removed:
  - the m091/m233 go.sum fallback augment (`~2036`)
  - the m233 `replace`-directive sibling block (`~2063`), which emitted an edge for a `replace` with no matching **direct** `require` (the measured `k8s.io/api -> k8s.io/streaming` case)
  - the Issue #251 residual-orphan backfill (`~2235`), which flat-attached every zero-incoming-edge component to the main module. This was the dominant source and the mechanism behind `orphan_count=0`
- [X] T010 [US1] Gate main-module `depends` so only backed requirements become edges (FR-001). **Deviation from data-model.md**: no separate `DeclaringSourceIndex` was needed — the requirer's own parsed `go.mod` (`doc`) is already in scope at every emission site, and `parse_go_mod` already yields `module_path`, `requires` with an `indirect` flag, and `replaces`. A tree-wide index is still required for the FR-009 build gate (T037), which has no `doc` in scope
- [ ] T011 [US1] Resolve the now-possibly-unused `ModuleGraphMap::gosum_fallback_paths_for` and `gosum_fallback_paths` in `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs` — remove if dead, or retain with a comment naming the remaining caller. Do not leave a dead public method whose doc comment describes edge augmentation that no longer happens
- [ ] T012 [US1] Preserve stranded components in the inventory per FR-007 in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` — removing an edge MUST NOT remove a component, and no relationship may be synthesised to the document root to make one reachable
- [ ] T013 [US1] Keep the three component states distinguishable per FR-007a across `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` and the emitters: parent-unknown, genuinely-no-dependencies, and edge-derived-by-a-weaker-tier. `waybill:orphan-reason` currently records edge provenance and sits on six of go-cobra's eight components, four of them on perfectly good edges — do not overload it for the first state

### US1b — Emit edge provenance (FR-002a / FR-002b)

**⚠️ MUST follow US1a.** `Relationship.provenance.source` currently names `go.mod` for the very edges US1a removes, so it asserts a falsehood today (research R2). Emitting it first would publish that falsehood.

- [ ] T014 [US1] Populate `Relationship.provenance.source` with the manifest path that genuinely declares the requirement, at `waybill-cli/src/scan_fs/mod.rs:~962`, replacing the blanket `entry.source_path`
- [ ] T015 [P] [US1] Emit provenance into the native SPDX 2.3 carrier — set `SpdxRelationship.comment` (already present, currently `None` at `waybill-cli/src/generate/spdx/relationships.rs:79`, `:161`, `:311`, `:347`) using the existing m071 `MikebomAnnotationCommentV1` envelope
- [ ] T016 [P] [US1] Emit provenance into the native SPDX 3 carrier — `Relationship.comment` in `waybill-cli/src/generate/spdx/v3_document.rs`
- [ ] T017 [US1] Emit the CycloneDX parity bridge in `waybill-cli/src/generate/cyclonedx/dependencies.rs`. CDX 1.6 `dependencies[]` has no `properties`, no `evidence`, and is not an annotation subject (research R3), so a `waybill:*` property is the Principle V carve-out here
- [ ] T018 [US1] Add the catalogue row to `docs/reference/sbom-format-mapping.md` with a justification clause naming the missing CDX native field, AND the matching entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` **in the same change** — otherwise `every_catalog_row_has_an_extractor` and `holistic_parity` both fail

### US1 tests

- [ ] T019 [P] [US1] Regression test in `waybill-cli/tests/`, gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` as `waybill-cli/tests/public_corpus.rs` already gates its corpus work, asserting a cold-cache `go-cobra` scan emits 5 golang edges with 0 unbacked, down from 7 with 2 unbacked (SC-001, contracts/edge-backing.md C-4)
- [ ] T020 [P] [US1] Assert the `kubernetes` arm of SC-001 (0 unbacked edges, down from 554 of 2984) in the **m770 quality-corpus lane** (`xtask/src/quality/`), NOT in `waybill-cli/tests/`. kubernetes is not a public-corpus target — there is no golden — and its 396 MB clone cannot sit in the default `cargo test --workspace` lane. cobra alone does not discharge SC-001: only the 39-module workspace exercises the `replace` handling from T006
- [ ] T021 [P] [US1] Control test in `waybill-cli/tests/`, gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` — **requires network and a Go toolchain** (`go mod download` warms the cache), so it must never run in the default lane — asserting the warm-cache `go-cobra` scan is unchanged at 5 edges, 0 unbacked, still declaring `complete` (SC-003, SC-004). If this moves, the change broke correct resolution instead of removing invented edges
- [ ] T022 [P] [US1] Test in `waybill-cli/tests/` that FR-012 holds: a module cache that is absent, empty, or unreachable yields identical output. All three arms are offline, so this one CAN run in the default lane — unlike T021, no cache needs warming. An operator must not get different declarations from the same project because of unrelated host state
- [ ] T023 [P] [US1] Test in `waybill-cli/tests/` asserting component count is unchanged (8 before, 8 after) and that `blackfriday` and `check.v1` remain present with no incoming edge (SC-005, contracts/edge-backing.md C-5)
- [ ] T024 [P] [US1] Test in `waybill-cli/tests/` asserting no root-originating relationship replaced a removed edge (SC-005)
- [ ] T025 [P] [US1] Test in `waybill-cli/tests/` resolving emitted provenance back to a real declaration in the scanned tree for a sample of edges — presence alone does not satisfy SC-003a

**Checkpoint**: Every emitted Go edge is backed and carries locatable provenance. The document may still declare itself `complete` — US2 fixes that.

---

## Phase 4: User Story 2 — The completeness declaration can be trusted (Priority: P1)

**Goal**: `complete` is never asserted over a graph whose gaps are known. Satisfies FR-003, FR-004, FR-005, FR-006.

**Independent Test**: Scan a Go project with no reachable module cache; confirm the document does not declare itself complete and states why.

**⚠️ Ships with US1, never alone.** US2 without US1 leaves invented edges in place; US1 without US2 leaves the document declaring `complete` over a graph whose edges were just removed — strictly worse for a consumer than today.

- [ ] T026 [US2] Verify the cascade predicted by research R4 in `waybill-cli/src/generate/graph_completeness/mod.rs`: with the unbacked edges gone, stranded components become genuine BFS orphans and `OrphanedComponentsDetected` fires unaided. If the verdict does NOT flip to `partial` on its own, stop and re-derive — the plan's central assumption is wrong
- [ ] T027 [US2] Implement the FR-003 independent check in `waybill-cli/src/generate/graph_completeness/mod.rs`: never emit `complete` while any per-ecosystem coverage signal reports `unknown` or `partial`. Independent of graph shape by design (FR-004), so it still holds if a future fallback attaches components to something real but wrong
- [ ] T028 [US2] Select or add the FR-005 reason code in `waybill-cli/src/generate/graph_completeness/reason_codes.rs`, distinguishing "the input carried no topology" from "topology existed but could not be resolved" — the two want opposite consumer responses (contracts/completeness-invariants.md I-5)
- [ ] T029 [US2] Leave `classify_transitive_edges_unresolvable` unchanged in `waybill-cli/src/generate/graph_completeness/mod.rs` and add a comment recording why: it keys on `design`/`analyzed` tiers while every go.sum-fallback component is `source` tier, so it never fires for this case, and making it fire would duplicate orphan detection

### US2 tests

- [ ] T030 [P] [US2] Invariant test I-1 in `waybill-cli/tests/`: no document declares `complete` while a per-ecosystem coverage signal reports `unknown` (SC-002)
- [ ] T031 [P] [US2] Invariant test I-2 in `waybill-cli/tests/`: no document declares `complete` while any component is unattached. Currently passes vacuously — assert it holds for the real reason
- [ ] T032 [P] [US2] Invariant test I-3 in `waybill-cli/tests/`: a fully-resolved warm-cache graph still declares `complete` (FR-006, SC-004). This is what stops "fixing" I-1 and I-2 by never emitting `complete` again
- [ ] T033 [P] [US2] Invariant test I-4 in `waybill-cli/tests/`: shape alone never blocks `complete` — a legitimately shallow graph with no unbacked edges and no coverage signal still earns it (SC-007)

- [X] T026a [US2] **(#871, folded in)** Split `assemblies` from `dependencies` in `waybill-cli/src/generate/cyclonedx/compositions.rs` (FR-015). They are different claims: enumerating an ecosystem's components says nothing about whether their edges were resolved. The `dependencies` claim is now per-ecosystem all-or-nothing, gated on every component in that ecosystem being reachable — per-component reachability is too weak, since a component can be reachable while its own outgoing edges were never resolved (`go-md2man` cold is reachable with no edges; warm proves it depends on `blackfriday`)
- [X] T026b [US2] **(#871, folded in)** Emit unresolved components under `aggregate: "unknown"` in `waybill-cli/src/generate/cyclonedx/compositions.rs` (FR-016) — CycloneDX's own vocabulary for real components whose graph position is undetermined, which removes the need for a `waybill:*` bridge here
- [X] T026c [US2] Thread `graph_completeness.reachable_set` into the compositions call at `waybill-cli/src/generate/cyclonedx/builder.rs:777`; `None` preserves pre-866 behaviour for the SPDX/OpenVEX callers that pass no ecosystems

**Checkpoint**: MVP complete. The graph is true and the document describes it honestly.

---

## Phase 5: User Story 3 — The degradation is diagnosable (Priority: P2)

**Goal**: A consumer reading the document alone learns why the graph is degraded and what would change it. Satisfies FR-008.

**Independent Test**: Scan with no reachable module cache; confirm the document alone explains cause and remedy.

- [ ] T034 [US3] Make the cause and remedy determinable from the emitted document alone per FR-008, in `waybill-cli/src/generate/graph_completeness/mod.rs` and the emitters. The remedy is real and already ships — a resolvable module graph, via a warm cache or m173's `--warm-go-cache` — so point at the condition an operator can change, not only the symptom
- [ ] T035 [P] [US3] Test in `waybill-cli/tests/` that a cold-cache document names the unresolved transitive edges and the remedy without reference to how the scan was invoked
- [ ] T036 [P] [US3] Test in `waybill-cli/tests/`, gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` — **requires network and a Go toolchain** for the warm arm — that SC-008 holds: given warm and cold documents for the same commit, the difference between them and its cause are determinable from the two documents alone

---

## Phase 6: User Story 4 — This cannot silently return (Priority: P3)

**Goal**: Reintroducing either defect fails the build. Satisfies FR-009, FR-010.

**Independent Test**: Revert the fix; confirm the project fails.

- [ ] T037 [US4] Add a build-time check in `waybill-cli/tests/` that fails on any relationship whose stated provenance does not contain it, naming the offending relationship (FR-009). **Evaluate each edge against the source it names**, not against the scanned tree unconditionally — a tree-only check would fail the deps.dev Maven enrichment, whose edges are correct and truthfully sourced (contracts/edge-backing.md C-1a). Must fail closed — no `continue-on-error`, per Constitution Principle III
- [ ] T038 [P] [US4] Add a build-time check in `waybill-cli/tests/` that fails on a document declaring `complete` while contradicting itself elsewhere (FR-009)
- [ ] T039 [US4] Teeth-check both gates per SC-006: temporarily revert the T009 removal in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` and confirm the T037 check fails; feed a hand-edited `complete`-with-`unknown` document to the T038 check and confirm it fails. Record both observed failures in `specs/866-go-graph-completeness/measurements/README.md`. A gate whose failure has never been observed is not known to work — three earlier attempts at measuring this defect returned "0 problems" for the wrong reason

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T040 Re-author the `go-cobra` and `go-kubernetes` `edges` and `max_depth` bounds in `xtask/corpus/quality-corpus.toml` (FR-011), measured with `$GOMODCACHE`, `$GOPATH` **and** `$HOME` all pointed at an empty directory. Edge counts fall — the intended outcome, not a regression. Bounds authored on a machine with a populated cache describe edges no clean runner can resolve, which is exactly how the originals came to be wrong (research R8, the #830 lesson)
- [X] T041 Regenerate the `go-cobra` and `pants-example-golang` public-corpus goldens by CI dispatch per `docs/development/refreshing-corpus-goldens.md`. **Freeze the fix set first** — any further emission-affecting change invalidates the run
- [ ] T042 [P] Execute the FR-014 cross-ecosystem measurement, starting with the two readers that have a documented fallback path: `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:1700` and `waybill-cli/src/scan_fs/package_db/gradle/mod.rs:273` (research R7). Record the outcome either way — SC-006a treats every non-Go ecosystem as *unknown*, not clean — and file a separate issue for anything found rather than absorbing it here (FR-013)
- [X] T043 [P] Update `specs/866-go-graph-completeness/measurements/README.md` with post-fix figures alongside the baselines (FR-010), so the before/after pair stays reproducible
- [X] T044 State the deliberate tradeoff in the PR body and as a close-out note in `specs/866-go-graph-completeness/tasks.md`: edge counts fall (`go-cobra` 7→5, `kubernetes` up to −554) because m091 added those edges to match trivy's go.sum-derived count, and this milestone reverses that (research R6). A reviewer comparing against trivy will otherwise read the drop as a regression
- [X] T045 Run `./scripts/pre-pr.sh` and confirm zero clippy errors and every suite `0 failed`. Enumerate the per-target `N passed; 0 failed` lines rather than grepping for failures
- [X] T046 [P] Verify the walker-audit gate separately — it is not in `scripts/pre-pr.sh` and trips CI even when local pre-PR is green
- [ ] T047 [P] Measure the FR-009 gate's cost against the 39-`go.mod` kubernetes target and record a budget in `specs/866-go-graph-completeness/plan.md` as a ratio against the scan it accompanies. plan.md currently promises a ceiling and sets none — either establish it here or delete the promise, but do not ship an unmeasured number

---

## Dependencies & Execution Order

```
Phase 1 Setup        T001-T004     baselines; T002-T004 parallel after T001
      |
Phase 2 Foundational T005-T008     BLOCKS EVERYTHING
      |
Phase 3 US1          T009-T025     US1a T009-T013 -> US1b T014-T018 -> tests T019-T025
Phase 4 US2          T026-T033     ships with US1; neither is safe alone
      |
Phase 5 US3          T034-T036     needs US2's reason code
      |
Phase 6 US4          T037-T039     needs US1+US2 to have something to gate
      |
Phase 7 Polish       T040-T047     T041 requires a frozen fix set
```

**Hard ordering constraints**

- **T005-T008 before everything.** The index is the authority for what counts as backed.
- **US1a before US1b** (T009-T013 before T014-T018). Provenance currently asserts a falsehood; emitting it before the removal publishes that falsehood (research R2).
- **T026 is a checkpoint, not a formality.** If the verdict does not flip to `partial` unaided, the plan's central assumption (research R4/R5) is wrong and the remaining US2 tasks need re-deriving.
- **T040 and T041 last.** Expectations are re-authored against final behaviour, and goldens regenerate only from a frozen fix set.

## Parallel Opportunities

- **Phase 1**: T002, T003, T004 after T001
- **US1b emitters**: T015 (SPDX 2.3) and T016 (SPDX 3) touch different files. T017 (CDX) and T018 (catalogue + extractor) are coupled and must land together
- **US1 tests**: T019-T025 all parallel
- **US2 tests**: T030-T033 all parallel
- **US3 tests**: T035, T036 parallel
- **Polish**: T042, T043, T046, T047 parallel

## Implementation Strategy

**MVP = Phases 1-4** (T001-T033). That is the whole correctness story: edges are true, provenance is emitted, and the completeness declaration is honest. Everything after is prevention, expectation maintenance, and scope closure.

**Do not ship a partial MVP.** US1 alone leaves the document declaring `complete` over a graph whose edges were just removed. US2 alone leaves the invented edges in place. The spec ranks both P1 for this reason.

**Smallest useful checkpoint**: after T026 the cascade is either confirmed or refuted. That is the highest-information point in the milestone and it arrives early — treat a surprise there as a reason to stop, not to push on.

---

## Close-out note (T044) — the deliberate tradeoff

**Edge counts fall, and that is the point.** `go-cobra` goes 7 → 5 golang
edges; `kubernetes` sheds up to 554. A reviewer comparing waybill against
a go.sum-derived tool will read that as a regression. It is not.

Milestone 091 added those edges deliberately, to match the module *count*
a go.sum enumeration produces. But `go.sum` is a hash list. It records
which modules are in the build's closure and nothing whatsoever about
which module requires which. Attaching that whole set to the main module
converts "these modules are somewhere in the graph" into "the main module
directly depends on each of these" — true transitively, false as stated,
and stated in the field consumers use for direct dependencies.

This milestone reverses that. The modules remain in inventory; only the
invented topology goes. Nothing is dropped from any SBOM — component
counts are unchanged on all eleven corpus targets.

What the corpus regeneration then showed is that the same falsehood had a
second home. With the fabricated edges gone, the SPDX 3 emitter's copy of
the issue-#236 root fallback — the only one of the three never gated —
began asserting every newly-stranded component as a direct dependency of
the document root. On the larger targets it had been doing this all
along, to files: `rust-ripgrep`'s root claimed `utils.sh` and
`ubuntu-install-packages`; `maven-guice`'s claimed `pom.xml` three times
over. Fixing it aligned all three formats on identical root out-edge
counts for every target.

**What a consumer should do about a lower count.** Read
`waybill:graph-completeness`. Cold-cache Go scans now say `partial` and
name the reason. The remedy ships already: a resolvable module graph —
a warm module cache, or `--warm-go-cache` from m173 — recovers the real
topology, and the warm control earns `complete`. A tool that reports the
higher number is not recovering more topology; it has no more topology
than waybill does.

### Still open

The invariant and build-gate tasks from US2/US3/US4 (T028-T039) and the
FR-014 cross-ecosystem measurement (T042) are not addressed by this
close-out. SC-006a's treatment of every non-Go ecosystem as *unknown*
still stands.
