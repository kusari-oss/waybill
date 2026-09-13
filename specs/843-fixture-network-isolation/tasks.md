# Tasks: A scan of this repository must not depend on network reachability

**Feature**: `843-fixture-network-isolation` · **Issue**: #843
**Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Format: `[ID] [P?] [Story] Description`

`[P]` = parallelisable. `[USn]` = the user story served. Setup,
Foundational and Polish carry no story label.

## Path Conventions

Repository-relative. Fixture manifests live under
`waybill-cli/tests/fixtures/`; the guard under `waybill-cli/tests/`.

## Measurement rule, applying to every task below

**Every timing figure is a median of at least three runs.** This is the
feature's own subject matter: the floor swings widely, and single-sample
comparisons against it produced two wrong attributions during this
feature's clarification and three during milestone 839. Driving the runs
from a script rather than a shell loop is also advised — shell
word-splitting silently produced empty results three times during
research.

---

## Phase 1: Setup

- [ ] T001 Record the pre-change floor into `specs/843-fixture-network-isolation/baseline.md`: median of three for `--offline`, for network-on with enrichment disabled, and for network-on with the Go proxy refused. Research measured 0.50s / 5.26s / 1.17s; re-establish them on the machine doing the work, because SC-001 and SC-002 are ratios against this and a figure from another host is not a baseline.
- [ ] T002 Record the current `go` subprocess inventory into the same file by shimming the binary as research R2 did — counts per subcommand, not timings (the shim inflates its own timings). Research found 80 invocations: 27 `go version`, 26 `go mod graph`, 17 `go list all`, 10 `go mod why`. This is the denominator for SC-003.

---

## Phase 2: Foundational (blocking prerequisites)

- [ ] T003 Enumerate every Go fixture manifest under `waybill-cli/tests/fixtures/` and classify each as `incidental` or `deliberate`, recording the result in `docs/development/go-fixture-inventory.md` (FR-005). Research found 21 of 27 declare unresolvable modules and **none** is deliberate; record the classification anyway so the next contributor reads it rather than re-deriving it.
- [ ] T004 For each manifest classified in T003, record which tests consume it. Research found only three files reference these fixtures by path — `waybill-cli/tests/mod_why_scaling.rs`, `waybill-cli/tests/goroot_skip.rs`, `waybill-cli/tests/pants_go_reader.rs` — and a first attempt that searched by fixture *name* returned 25 files because `workspace_mode` is also a type in this codebase. Search by path.
- [ ] T005 Record for each manifest whether a committed golden covers it, in the same inventory. This decides the replacement shape in T007: a golden-covered fixture takes a missing target so its `waybill:go-transitive-coverage` annotation does not change.

**Checkpoint**: the inventory answers "what is this fixture for, who uses it, and does a golden watch it" for every Go manifest in the tree. Nothing below should require re-deriving any of that.

---

## Phase 3: User Story 1 — A contributor measuring performance gets a number they can trust (P1)

**Goal**: The scan floor becomes a property of the repository rather than of the network.

**Independent test**: Scan this repository twice with enrichment disabled; the wall times agree closely, and agree with a scan taken with no network at all.

- [ ] T006 [US1] Add a local `replace` directive for every `require` in the manifests under `waybill-cli/tests/fixtures/golang/` and `waybill-cli/tests/fixtures/golden_inputs/golang/`, using relative paths only (FR-006 — absolute paths pass locally and break after `git archive`).
- [ ] T007 [US1] Do the same for `waybill-cli/tests/fixtures/pants_go/`, `waybill-cli/tests/fixtures/goroot_stub/app/go.mod` and `waybill-cli/tests/fixtures/project_discovery/polyglot_nested_independent/services/worker/go.mod`. Prefer a **missing** target where T005 recorded a golden: it keeps the failure and its annotations, so nothing churns. Use a real local target only where a test wants resolution to succeed.
- [ ] T008 [P] [US1] Verify each modified manifest in isolation: `cd <fixture> && time go mod graph` completes in ~10ms, whether it prints a graph or a local `no such file or directory`. A result of a second or more means it is still reaching the proxy.
- [ ] T009 [US1] **Measurement checkpoint.** Re-measure the three arms from T001 and record them beside the originals. SC-001 (two consecutive scans within 20%), SC-002 (networked within 20% of no-network), SC-003 (zero fixture-attributable network requests, from a fresh shim run against T002's inventory).
- [ ] T010 [US1] Confirm the goldens are byte-identical: run `cargo +stable test --workspace` and check `git status` reports no modified files under `waybill-cli/tests/fixtures/golden/`. A changed golden means T007 chose a real target where a missing one was required.

**Checkpoint**: US1 is independently shippable here.

---

## Phase 4: User Story 2 — Fixtures that exist to test unresolvable modules keep doing so (P1)

**Goal**: The cheapest fix does not pass by deleting coverage of waybill's own failure paths.

**Independent test**: Every test asserting on unresolved, fallback or degraded-resolution behaviour still fails when that behaviour is deliberately broken.

- [ ] T011 [US2] Run the three consuming tests identified in T004 and confirm they pass unchanged: `cargo +stable test --test mod_why_scaling --test goroot_skip --test pants_go_reader`.
- [ ] T012 [US2] For each fixture T003 classified as `deliberate`, verify it still fails to resolve — locally, not over the network. Research found this set empty; if T003 finds it non-empty, this task is where the missing-target shape is applied and verified.
- [ ] T013 [US2] Verify the Go resolution ladder still reports degraded coverage where it did before: compare `waybill:go-transitive-coverage`, `-reason` and `-fallback-count` in a fresh scan against the committed golden values (`unknown`, `offline-mode: …`, `11`). Any change means a fixture that was meant to stay unresolvable now resolves.

---

## Phase 5: User Story 3 — A contributor scanning locally does not wait on the internet (P2)

**Goal**: Ordinary local use stops paying for lookups that cannot succeed.

**Independent test**: Scan this repository on a machine with no network; nothing waits on a timeout.

- [ ] T014 [US3] Scan the repository with the network genuinely unavailable — not `--offline`, which masks the question by disabling the code path — and confirm no operation stalls and the component count matches the networked scan. Record the result in `baseline.md`.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T015 Add the regression guard at `waybill-cli/tests/fixture_network_guard.rs` per `contracts/fixture-isolation-guard.md`: fail when a Go fixture manifest declares a `require` with no matching local `replace`, naming the offending manifest and what to do (G-1.2). It must not need the network to run (G-2.1), must not assert on wall time (G-2.2, see #849), and must reject absolute replacement paths (G-2.3).
- [ ] T016 Prove the guard works (G-4.1): add a manifest with an unreplaced `require`, confirm the guard fails and names it, remove it, confirm the guard passes. Assert by doing, not by inspection.
- [ ] T017 [P] Publish `specs/843-fixture-network-isolation/quickstart.md` to `docs/development/adding-go-fixtures.md`, and link it from the docs index so it is found before the next fixture is written rather than after.
- [ ] T018 [P] File the `go version` finding as its own issue: the scan invokes it **27 times** per run as a per-workspace capability probe (T002). Independent of this feature, and cheap to cache.
- [ ] T019 Confirm or revise the FR-001b residual figure with the post-change inventory, and record it in `baseline.md`. Research attributes it to subprocess volume rather than network; if the post-change residual is materially different from ~0.67s, say so rather than carrying the old number forward.
- [ ] T020 Run the full pre-PR gate — `cargo +stable clippy --workspace --all-targets` and `cargo +stable test --workspace`, both clean — and enumerate the per-suite results rather than grepping for failures.

---

## Dependencies & Execution Order

```
Setup (T001-T002)          ← the denominators everything is judged against
   └─▶ Foundational (T003-T005)   ← blocks all fixture edits
          ├─▶ US1 (T006-T010)     the fix and its proof
          │      └─▶ US2 (T011-T013)  needs US1's edits in place to verify
          │             └─▶ US3 (T014)
          └─────────────────▶ Polish (T015-T020)
```

### User story dependencies

- **US2 depends on US1** — it verifies that US1's edits preserved what had to be preserved. It cannot run first.
- **US3 depends on US1** for its result to be meaningful, though it is a distinct observation (no network at all, versus `--offline`).
- **T015/T016 depend on US1 completing**, deliberately. A guard written before the fix would be tuned until it passed, which is how a guard ends up asserting what the code does rather than what it should.

### Parallel opportunities

- T008 verifies each manifest independently — parallel across fixtures.
- T017 and T018 are independent of everything after Phase 2.
- T006 and T007 touch disjoint directories and could be split, though the second depends on T005's golden classification to choose its replacement shape.

---

## Implementation strategy

**MVP**: Phases 1–3. The floor becomes stable and the measured problem is fixed. Everything after protects that result rather than extending it.

**Then** Phase 4, which is short only because research found the deliberate set empty — if T003 disagrees, this is where the work lands.

**Then** Phases 5–6. The guard is last on purpose.

---

## Traceability

| Requirement | Tasks |
|---|---|
| FR-001 / FR-001a no network for the tree | T006, T007, T009 |
| FR-001b residual decomposed | T002, T019 |
| FR-002 floor independent of network | T009, T014 |
| FR-003 deliberate fixtures preserved | T011, T012, T013 |
| FR-004 incidental fixtures self-contained | T006, T007 |
| FR-005 classification recorded | T003, T004, T005 |
| FR-006 survives extraction | T006, T007, T015 |
| FR-007 broken reference fails visibly | T015, T016 |
| FR-008 new fixtures detected | T015, T016 |
| FR-009 / FR-009a production unchanged | T010, T013, T020 |
| FR-010 deliberate fixtures fail locally | T008, T012 |

| Criterion | Verified by |
|---|---|
| SC-001 two scans within 20% | T009 |
| SC-002 no-network parity | T009, T014 |
| SC-003 zero fixture network requests | T009 |
| SC-004 fallback tests still detect regressions | T011, T013 |
| SC-005 new fixture detected | T016 |
