---
description: "Task list for m196 — Populate remaining 5 public-corpus target goldens via CI-runner regen dispatch (Q1 clarification)"
---

# Tasks: Populate Remaining Public-Corpus Goldens

**Input**: Design documents from `/specs/196-populate-corpus-goldens/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/regen-workflow.md, quickstart.md

**Tests**: No new test code — the m195 test harness IS the test infrastructure; m196 populates its fixture data.

**Organization**: 4 user stories all P1 or P2, with two hard prerequisites (postgres digest resolution + workflow dispatch input) captured as Foundational. US1 (all-6-targets-green) is the payoff; US3 (assertion drift reconciliation) is intertwined with US1's dispatch loop; US2 (postgres digest) unblocks the image-postgres16 target inside US1; US4 (byte-identity verify) is the SC-006 gate that runs after US1's goldens are committed.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (distinct files, no dependency on incomplete tasks)
- **[Story]**: US1 / US2 / US3 / US4
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **CI workflow**: `.github/workflows/public-corpus.yml`
- **Corpus manifest / assertions**: `mikebom-cli/tests/corpus_harness_195/{manifest.rs,layer1_assertions.rs}`
- **Golden fixtures**: `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`
- **Feature spec dir**: `specs/196-populate-corpus-goldens/`

---

## Phase 1: Setup

**Purpose**: Verify the m196 branch is checked out clean and the m195 corpus infrastructure is intact.

- [ ] T001 Confirm branch `196-populate-corpus-goldens` is checked out and `git status` shows only the specs/ dir untracked (plan.md, research.md, data-model.md, contracts/, quickstart.md — no src/ changes yet).
- [ ] T002 Confirm the m195 corpus harness compiles cleanly on this branch: `cargo test --test public_corpus --no-run` MUST succeed. Baseline check — nothing should have drifted between m195 merge and m196 start.

**Checkpoint**: Clean starting state confirmed. Ready for Foundational phase.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the workflow_dispatch `regen_goldens` input to `public-corpus.yml`. Every subsequent phase (US1, US3, US4) depends on this because they all dispatch the workflow with the new input.

⚠️ **CRITICAL**: T003 MUST land BEFORE any regen dispatch. Otherwise the dispatch has no way to signal regen mode.

- [ ] T003 In `.github/workflows/public-corpus.yml`, apply the three edits per data-model.md M3:
  - Add `regen_goldens: type: boolean, default: false` to `workflow_dispatch.inputs`.
  - Update the "Run public corpus" step's `env:` block to conditionally inject `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS: ${{ inputs.regen_goldens == true && '1' || '' }}` (empty string when unset — the `layer2_golden::update_goldens_gate()` helper checks for exact string `"1"`).
  - Add the new `Upload regenerated goldens` step at the end of the job, gated on `if: inputs.regen_goldens == true`, uploading `mikebom-cli/tests/fixtures/public_corpus/` as artifact name `corpus-goldens-regen`, retention 14 days. Use the same `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1` SHA-pin from m195.
- [ ] T004 Commit T003 as a standalone commit titled `ci(196): add regen_goldens dispatch input to public-corpus.yml`. Push to `196-populate-corpus-goldens` — the CI dispatch surface needs to be on the remote before any subsequent dispatch can invoke it.
- [ ] T005 Verify the workflow YAML syntactically validates via GitHub Actions' own parser: `gh workflow view public-corpus.yml --repo kusari-oss/mikebom --ref 196-populate-corpus-goldens` MUST succeed. If it doesn't, YAML edit likely broken — fix before proceeding.

**Checkpoint**: `regen_goldens=true` dispatch is available. All subsequent phases can use it.

---

## Phase 3: User Story 2 — Postgres:16 pinned by real Docker Hub digest (Priority: P1)

**Goal**: Replace the m195 placeholder digest with the real linux/amd64 digest so the image target can actually pull and scan.

**Independent Test**: `docker pull docker.io/library/postgres@<pinned-digest>` succeeds against the value in `manifest.rs` — no `docker manifest not found` error.

### Implementation for User Story 2

- [ ] T006 [US2] Resolve the linux/amd64 digest of postgres:16 locally:
  ```bash
  docker manifest inspect --verbose docker.io/library/postgres:16 \
    | jq -r '.[] | select(.Descriptor.platform.architecture == "amd64"
        and .Descriptor.platform.os == "linux") | .Descriptor.digest'
  ```
  Record the result (a `sha256:<64-hex>` string) in `specs/196-populate-corpus-goldens/scratch/postgres-digest.txt`.
- [ ] T007 [US2] In `mikebom-cli/tests/corpus_harness_195/manifest.rs`, locate the `image-postgres16` entry and replace `pinned.algo_hex` with the resolved digest from T006. Update the surrounding comment to reference m196 and the resolution command (per data-model.md M1 shape).
- [ ] T008 [US2] Verify the change by running `cargo build --test public_corpus` — the manifest is const-evaluated at compile time; a broken digest literal would fail compilation.
- [ ] T009 [US2] Commit T007 as `impl(196): pin real postgres:16 amd64 digest (US2)` and push.

**Checkpoint**: Docker Hub will accept the pull for the image-postgres16 target. Ready for regen dispatch to include it.

---

## Phase 4: User Stories 1 + 3 — Regen loop (assertion reconciliation + goldens generation) (Priority: P1)

**Goal**: Iterate the regen dispatch until all 5 non-cobra targets pass Layer 1, then commit the resulting 15 goldens. US3 assertion adjustments happen INSIDE this loop — the loop's discovery mechanism is emitted-SBOM inspection per research §R4.

**Joint Independent Test**: `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus` on the finalized branch reports 6 of 6 corpus tests `ok` when run against the committed goldens (verification mode, not regen).

### Iteration 0 — Baseline dispatch

- [ ] T010 [US1] Dispatch the workflow in regen mode against the current branch state:
  ```bash
  gh workflow run public-corpus.yml \
    --ref 196-populate-corpus-goldens \
    -f branch=196-populate-corpus-goldens \
    -f regen_goldens=true
  RUN_ID=$(gh run list --workflow=public-corpus.yml \
    --branch=196-populate-corpus-goldens --limit=1 \
    --json databaseId --jq '.[0].databaseId')
  gh run watch "$RUN_ID"
  ```
  Record the run ID + pass/fail status per target in `specs/196-populate-corpus-goldens/scratch/iteration-0.txt`.

### Iteration N (loop until Layer 1 passes for all targets)

- [ ] T011 [US3] For each Layer 1 failure surfaced in Tn's dispatch, download the `corpus-emitted-sboms` artifact:
  ```bash
  gh run download "$RUN_ID" --name corpus-emitted-sboms -D /tmp/iter-N/
  ```
  Inspect the failing target's emitted CDX / SPDX to identify the actual shape mikebom emits (not the m195-authored expected shape).
- [ ] T012 [US3] Adjust the corresponding `<target>_layer1` function in `mikebom-cli/tests/corpus_harness_195/layer1_assertions.rs` per data-model.md M2 rules:
  - MUST match observed output (not aspirational).
  - MUST NOT weaken the class-of-bug tripwire past m195 R8 seed intent.
  - MUST carry a doc-comment recording (a) what m195 assumed, (b) what mikebom emits, (c) why the adjustment preserves the tripwire.
  - Record each adjustment in `specs/196-populate-corpus-goldens/scratch/assertion-adjustments.txt` (list format: `<target> — <invariant> — <observed> vs <expected> — <resolution>`).
- [ ] T013 [US3] Commit each iteration's assertion adjustments as its own commit titled `impl(196): reconcile <target> Layer 1 with observed output (US3)`. Push. Return to T010 to re-dispatch until all Layer 1 pass.

### Terminal iteration — commit goldens (US1)

- [ ] T014 [US1] Once the regen dispatch succeeds with all 6 corpus tests `ok` (in regen mode, which forces goldens to be written whenever Layer 1 passes), download the goldens artifact into the fixtures tree in place:
  ```bash
  gh run download "$RUN_ID" \
    --name corpus-goldens-regen \
    -D mikebom-cli/tests/fixtures/public_corpus/
  ```
- [ ] T015 [US1] Verify FR-005 (additive-only — go-cobra fixtures MUST NOT change) per research §R5:
  ```bash
  git status mikebom-cli/tests/fixtures/public_corpus/
  git diff --stat mikebom-cli/tests/fixtures/public_corpus/go-cobra/
  ```
  The `go-cobra/` diff MUST be empty. If it isn't, DO NOT COMMIT — investigate the drift (regen mode overwrote cobra's goldens with different content, indicating either mikebom or the harness drifted). Record findings under `scratch/` and re-evaluate.
- [ ] T016 [US1] Confirm the 15 new goldens landed at the expected paths:
  ```bash
  ls mikebom-cli/tests/fixtures/public_corpus/{rust-ripgrep,npm-express,python-flask,maven-guice,image-postgres16}/{cdx,spdx-2.3,spdx-3}.json
  ```
  All 15 files MUST exist and be non-empty.
- [ ] T017 [US1] Commit the goldens as `impl(196): commit 15 goldens for 5 non-cobra corpus targets (US1)` with the run ID + workflow-run URL in the commit body for reproducibility.

**Checkpoint**: All 6 corpus targets have committed goldens; Layer 1 assertions match empirical output; FR-005 additive-only preserved.

---

## Phase 5: User Story 4 — Byte-identity verification (SC-003 / SC-006) (Priority: P2)

**Goal**: Prove the goldens are deterministic by re-dispatching regen and byte-comparing.

**Independent Test**: Two consecutive dispatches produce byte-identical artifacts (after masking).

### Implementation for User Story 4

- [ ] T018 [US4] Re-dispatch the regen workflow on the current branch head (post-T017):
  ```bash
  gh workflow run public-corpus.yml \
    --ref 196-populate-corpus-goldens \
    -f branch=196-populate-corpus-goldens \
    -f regen_goldens=true
  NEW_RUN_ID=$(gh run list --workflow=public-corpus.yml \
    --branch=196-populate-corpus-goldens --limit=1 \
    --json databaseId --jq '.[0].databaseId')
  gh run watch "$NEW_RUN_ID"
  ```
- [ ] T019 [US4] Download the new artifact into a comparison directory:
  ```bash
  gh run download "$NEW_RUN_ID" \
    --name corpus-goldens-regen \
    -D /tmp/regen-verify/
  ```
- [ ] T020 [US4] Byte-diff against the committed fixtures:
  ```bash
  diff -r mikebom-cli/tests/fixtures/public_corpus/ /tmp/regen-verify/
  ```
  MUST produce zero output. If differences appear, either (a) the m195 masking rules miss a non-deterministic field surfaced by one of the new targets (extend the mask in `layer2_golden.rs`), or (b) mikebom has a non-determinism regression (out of m196 scope; file a separate issue). Record findings under `scratch/`.
- [ ] T021 [US4] Record SC-003 / SC-006 verification result in `specs/196-populate-corpus-goldens/scratch/byte-identity-verification.txt`: run IDs, timestamps, verdict.

**Checkpoint**: Byte-identity proven across two dispatches. Corpus reproducibility invariant holds.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: pre-PR gate, agent-context refresh, PR body preparation.

- [ ] T022 Update CLAUDE.md agent-context via `.specify/scripts/bash/update-agent-context.sh claude` if not already run during /speckit-plan. Verify m196 appears in Recent Changes.
- [ ] T023 Verify SC-006 (pre-PR wall-clock delta ≤ 5s) with a **captured baseline** rather than eyeballing:
  ```bash
  # 1. Time post-m196 (current HEAD):
  time ./scripts/pre-pr.sh 2>&1 | tail -3
  # 2. Stash m196 changes and time the pre-m196 baseline:
  git stash push -m 'm196-scratch: measure pre-PR baseline'
  time ./scripts/pre-pr.sh 2>&1 | tail -3
  git stash pop
  ```
  Record both wall-clock timings + the computed delta in
  `specs/196-populate-corpus-goldens/scratch/pre-pr-timing.txt`. Delta MUST be
  ≤ 5s per SC-006 — since m196 adds zero library code, delta is expected
  to be ≈0s. If it isn't, investigate before proceeding.
- [ ] T023a Verify SC-001 (all 6 corpus targets `ok` in **VERIFY mode** — the
  closest CI-side equivalent to what nightly will do post-merge). Dispatch
  the workflow WITHOUT `regen_goldens` (the default `false`) against the
  finalized branch head:
  ```bash
  gh workflow run public-corpus.yml \
    --ref 196-populate-corpus-goldens \
    -f branch=196-populate-corpus-goldens
  VERIFY_RUN_ID=$(gh run list --workflow=public-corpus.yml \
    --branch=196-populate-corpus-goldens --limit=1 \
    --json databaseId --jq '.[0].databaseId')
  gh run watch "$VERIFY_RUN_ID"
  ```
  The dispatch runs Layer 1 assertions + Layer 2 byte-diff against the
  committed goldens — the exact behavior nightly will exercise. Every
  target MUST report `test ... ok`. Record the run ID + verdict in
  `specs/196-populate-corpus-goldens/scratch/verify-mode-run.txt`. This
  closes the SC-001 verification loop that T010 + T018 (both regen mode)
  did not — regen mode bypasses Layer 2 diff.
- [ ] T024 Open the PR (via `gh pr create`) with title `impl(196): populate remaining 5 public-corpus target goldens` and body summarizing: (a) 15 goldens added, (b) postgres digest resolved, (c) assertion adjustments made (link to `scratch/assertion-adjustments.txt`), (d) SC-001 verify-mode run ID from T023a, (e) SC-003 byte-identity run IDs from T018-T021, (f) SC-006 pre-PR delta from T023, (g) test plan checkboxes.

**Checkpoint**: PR opened; ready for review.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002. Independent — nothing blocked.
- **Foundational (Phase 2)**: T003 → T004 → T005. Depends on Setup. **BLOCKS Phase 4 (US1/US3)** because regen dispatch needs the new input on the remote.
- **US2 (Phase 3)**: T006 → T007 → T008 → T009. Depends on Setup only. Parallel with Foundational (different files entirely: manifest.rs vs workflow.yml).
- **US1 + US3 loop (Phase 4)**: T010 (dispatch) → T011/T012/T013 (iterate assertion adjustments) → T014 → T015 → T016 → T017 (commit goldens). Depends on Foundational + US2 both done.
- **US4 (Phase 5)**: T018 → T019 → T020 → T021. Depends on US1 goldens committed (Phase 4 done).
- **Polish (Phase 6)**: T022 → T023 → T024. Depends on all user stories.

### User Story Dependencies

- **US2 ⊥ Foundational**: independent; parallel-capable.
- **US1 depends on US2** (image-postgres16 target needs a real digest before it can pull) AND on Foundational (workflow input needs to exist).
- **US3 is intertwined with US1** — the assertion adjustments happen during US1's regen iteration loop. They can't be pre-planned; they emerge from observation.
- **US4 depends on US1** (byte-identity check needs goldens committed).

### Parallel Opportunities

- Phase 2 + Phase 3 in parallel (different files: workflow YAML vs manifest.rs Rust source).
- Within Phase 4, T011/T012/T013 iteration can parallelize across targets if a maintainer wants to reconcile multiple failing targets' assertions in the same commit (e.g., all 5 targets fail Layer 1 identically — one iteration adjusts them all). But typically sequential per iteration (dispatch → observe → adjust → re-dispatch).
- Phase 6 T022/T023 in parallel (docs vs test-runner).

---

## Parallel Example: Foundational + US2 in parallel

```bash
# Maintainer A works on the workflow input (Phase 2):
git checkout 196-populate-corpus-goldens
# Edit .github/workflows/public-corpus.yml (T003)
git commit -m "ci(196): add regen_goldens dispatch input"
# Verify (T005)

# Maintainer B (or the same maintainer, in a separate terminal) works on postgres digest (Phase 3):
# T006: resolve digest locally
docker manifest inspect --verbose docker.io/library/postgres:16 | ...
# T007: edit manifest.rs
git commit -m "impl(196): pin real postgres:16 amd64 digest"

# Both commits land on the branch independently; then merge and proceed to Phase 4.
```

---

## Implementation Strategy

### MVP (this milestone IS the MVP)

The milestone is intentionally narrow — there's no smaller MVP. Every task listed is required for SC-001 (all 6 targets green nightly). Skipping any leaves the corpus signal weaker than it should be.

### Single-PR delivery (matches m190 / m191 / m192 / m194 / m195 shape)

Land Phases 1–6 in a single PR titled `impl(196): populate remaining 5 public-corpus target goldens`. Commit granularity per phase (per T004, T009, T013 iterations, T017) preserves reviewer digestibility.

### Iteration realism

Phase 4 is a genuine loop — expect 1-4 iterations of T010/T011/T012/T013 before all Layer 1 assertions align with observed output. Budget ~1 hour of maintainer time per iteration (dispatch takes ~10-15 min wall-clock; inspection + adjustment ~30 min). If the iteration count exceeds 4, the m195 assertions were written from too-thin spec knowledge — file a follow-up issue rather than continuing to grind.

---

## Notes

- Total tasks: 25 across 6 phases (T023 extended + T023a added post-analyze to close SC-001 / SC-006 verification loops).
- US1: 4 tasks (T010, T014-T017). US2: 4 tasks (T006-T009). US3: 3 tasks (T011-T013, plus iteration count TBD). US4: 4 tasks (T018-T021). Setup / Foundational / Polish: 10 tasks (T023a added).
- **Zero new Cargo dependencies** (research §R2 audit).
- **Zero new `mikebom:*` annotations** (spec Assumption 3).
- **Zero library code changes** — everything happens in `mikebom-cli/tests/` + `.github/workflows/`.
- **FR-005 protection** (go-cobra additive-only) enforced at commit-time via T015 git-diff gate.
- **SC-003 / SC-006 byte-identity** verified via T018-T021 re-dispatch loop.
- **All golden generation on Linux CI** per Q1 clarification — no local-laptop regen anywhere in this task graph.
