# Tasks: a binary match that merges into another reader's component stays visible on it (#1665)

**Input**: `specs/1646-merged-binary-evidence/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-003 to SC-006).
- **Written red.** Every case in `contracts/wire.md` that expects a change (B-1, B-2, B-3, F-1, F-2) is written before the change that produces it, and must fail on the log-only commit.
- **Guards.** R-1 (record into record) passes on `main` trivially. Its teeth come from mutation M6.

**Paths**:
- `RC` = `waybill-common/src/resolution.rs`
- `SM` = `waybill-cli/src/scan_fs/mod.rs` (the conversion site, ~line 1248)
- `DD` = `waybill-cli/src/resolve/deduplicator.rs` (S1 ~139–370, S2 `fold_declared_not_cached` ~516)
- `CE` = `waybill-cli/src/generate/cyclonedx/evidence.rs`
- `S2A` = `waybill-cli/src/generate/spdx/annotations.rs`; `S3A` = `waybill-cli/src/generate/spdx/v3_annotations.rs`
- `IT` = `waybill-cli/tests/merged_binary_evidence.rs` (new)
- `MS` = `specs/1646-merged-binary-evidence/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R9. Cases are `contracts/wire.md`'s. Mutations are R8's M1–M6.

**Story order.** US2 (the technique) is implemented before US1 (the absorbed method), because `absorb_method` keys on `BinaryAnalysis`, which US2 sets. US1 remains the defect and the priority.

## Phase 1: Setup

- [ ] T001 Build `main`'s binary at `e1148818` into the scratchpad as `waybill-main-e1148818`. Before building, confirm `git diff e1148818 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.
- [ ] T002 Create `IT`, copying the harness from `waybill-cli/tests/library_identity.rs` (`tree`, `lib`, `pc`, `cmake_version`, `scan_with_log` with parity and unique `bom-ref`s, `events`), and adding:
  - `scan_with_log` returns all three documents and validates the CycloneDX one with `tests/common/cdx_schema.rs`;
  - `cdx_methods(doc, name) -> Vec<(String, f64)>`, from `evidence.identity[0].methods`;
  - `spdx_d1(doc, name) -> Value`, the parsed `evidence.identity` annotation payload, for both SPDX 2.3 and SPDX 3;
  - `WAYBILL_MERGED_EVIDENCE_BIN`, to run a case against T001's binary.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [ ] T003 In `RC`, add `ResolutionTechnique::BinaryAnalysis`, `pub struct IdentityMethod { technique, confidence }`, and `ResolutionEvidence.also: Vec<IdentityMethod>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` (R1, R2).
  - Give every `ResolutionEvidence { … }` literal `also: Vec::new()`. The compiler enumerates them; there are about 70.
  - Add the `BinaryAnalysis` arm to the three technique `match`es: `binary-analysis` in `CE`, `S2A` and `S3A`.
  - Nothing sets the new variant yet.
  - Unit test: a serialized `ResolutionEvidence` with an empty `also` is byte-identical to before (no `also` key).
- [ ] T004 In `DD`, add `absorb_method(best: &ResolutionEvidence, best_kind, other: &ResolutionEvidence, other_kind) -> Vec<IdentityMethod>` (R3). It returns what would be absorbed: the techniques of `other` and `other.also`, gated by the binary condition and deduplicated by technique, in variant order.
  - Call it at S1 and S2. On this commit only the `binary_merge` census event is emitted (`contracts/wire.md`), and `best` is not modified.
  - Unit tests on constructed components: banner into `.pc` absorbs `BinaryAnalysis`; a record into a record absorbs nothing; a second `BinaryAnalysis` is not repeated; `also` carries over through a chain of merges.
- [ ] T005 In `SM`, emit the `technique_change` and `binary_derived_kind` census events (R6) for every converted entry whose `evidence_kind` is binary-derived. Grep every `technique: ResolutionTechnique::PackageDatabase` site in `scan_fs/` that can receive a binary entry, and record the list in `MS/control.md`.
- [ ] T006 In `CE`, `S2A` and `S3A`, emit `also` (R4):
  - CycloneDX: `methods` is the primary method, then `also`;
  - SPDX: the flat keys plus a `methods` array, only when `also` is non-empty.

  This is inert on this commit, because `also` is always empty. Unit tests on constructed components cover a single method (payload unchanged) and two methods (the shapes in data-model "Wire").
- [ ] T007 Write every case of `contracts/wire.md` in `IT`, with its expected result:
  - B-1, B-2, B-3, F-1 and F-2 are red on this commit;
  - R-1, P-1, S-1 and C-1 pass.

  C-1 must see exactly one `binary_merge` (site `S1`, winner mechanism `pkg-config`, loser kind `embedded-version-string`) and one `technique_change`. That is the positive control.

  Record the red/green table in `MS/control.md`, run with both this commit's binary and T001's.
- [ ] T008 Run `IT` with the red cases unignored, and record in `MS/control.md` that exactly the planned cases fail, each by name. Mark them `#[ignore = "spec 1646: red until T0NN"]`, naming the task that turns each green. Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Commit the log-only change.
- [ ] T009 Census (FR-001, R6). In `FR`:
  - extend `census/versions.py` to parse `technique_change`, `binary_derived_kind` and `binary_merge`;
  - add to the `control` job a tree with B-1's shape (banner plus `.pc`) and F-1's (fingerprint alone), and assert their events;
  - make the assertion accept the log-only and the wired states (spec 1645's lesson).

  Dispatch `census-versions.yml` with `--ref main -f waybill_ref=<T008's full SHA>`, and verify every `refs.txt`. Dispatch the public corpus with `-f branch=1646-merged-binary-evidence`, and confirm 166 of 166 at T008's commit.

  Record in `MS/census.md`, per target:
  - `technique_change` counts by kind;
  - `binary_derived_kind` counts by kind;
  - every `binary_merge` with its site and both sides.

  **Checkpoint**: if a count contradicts FR-002 to FR-008, amend the spec first. The expected counts are:
  - TheRock: 6 `binary_merge` (expat, gmp, liblzma, llvm, ncurses, zlib);
  - TheRock: 7 libraries with a `technique_change`, possibly more events where a library has several files.

  Then file the follow-up issue for the other binary-derived kinds, with their measured counts (clarification 2).

## Phase 3: User Story 2 — a version read from a binary is labelled binary analysis (P2, done first)

**Goal**: banner and symbol matches report `binary-analysis`.
**Independent test**: B-3 and F-1 pass.

- [ ] T010 [US2] In `SM`, and at every site T005 listed, set `technique: BinaryAnalysis` when `evidence_kind` is `embedded-version-string` or `symbol-fingerprint`. Remove the `technique_change` event.
  - Remove the `#[ignore]` from B-3 and F-1.
  - Regenerate spec 1643's B-1 golden with `WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS=1`, then diff it with masked IDs and sorted lines. The only change must be the technique of the three banner components: `manifest-analysis` → `binary-analysis` in CycloneDX, `package-database` → `binary-analysis` in SPDX. Record the diff in `MS/control.md`.

## Phase 4: User Story 1 — a merged component says its library was also read from the binary (P1) 🎯 MVP

**Goal**: every merge with a binary match lists its `binary-analysis` method.
**Independent test**: B-1, B-2 and F-2 pass; R-1 still passes.

- [ ] T011 [US1] In `DD`, apply `absorb_method`'s result to `best.also` at S1 and S2, and remove the `binary_merge` event. Remove the `#[ignore]` from B-1, B-2 and F-2.
  - B-1 asserts that the CycloneDX methods are `manifest-analysis` then `binary-analysis`, and that both SPDX D1 payloads keep their flat `package-database` and add `methods`.
  - F-2 asserts exactly one `binary-analysis` and that `fingerprint-symbols-matched` is kept.
  - R-1 asserts that SPDX D1 has no `methods` and that C56 is as on `main`.
- [ ] T012 [US1] Mutations M1–M6 (R8). Copy spec 1645's `measurements/mutate.py` to `MS/mutate.py`, and write each mutation as an exact-string replacement. Record the killing tests in `MS/mutations.md`. Every mutation must be killed. Afterwards, `diff -r` the source against a copy taken before the run, and rebuild.

## Phase 5: User Story 3 — nothing else changes (P2)

**Goal**: only identity evidence changes.
**Independent test**: the public corpus, the quality corpus and TheRock, each traced against `main`.

- [ ] T013 [US3] Public corpus at the final code commit (`-f branch=1646-merged-binary-evidence`), with the summary's commit confirmed.
  - Trace every changed line. Each must be an identity method or technique of a component a binary match touched.
  - Regenerate with `regen_goldens=true`, and confirm the regenerated files are byte-identical (`cmp`) to the traced output.
  - Commit them, and confirm with a run that passes 166 of 166.

  Record all of it in `MS/corpus.md`.
- [ ] T014 [US3] Quality corpus (`census-versions.yml`) and TheRock (`measure-full`, base `e1148818` against the final code commit), at full SHAs, with `refs.txt` verified. Record in `MS/therock.md`:
  - every component whose evidence changed, traced. Expected: the 6 merged libraries gain `binary-analysis`; bzip2 and any other unmerged binary match change technique;
  - that components, PURLs, versions, CPEs and edges are identical to `main`, with the counts compared;
  - `waybill sbom parity-check` on both arms;
  - SC-007's timing: one warm-up per arm, 3 interleaved runs, the median ratio, and min/median/max.

## Phase 6: Polish

- [ ] T015 [P] Docs:
  - `docs/reference/sbom-format-mapping.md` D1 row: the `methods` array in SPDX when there are two or more, and the new `binary-analysis` technique;
  - `docs/reference/binary-identity.md`: what a merged component records.
- [ ] T016 [P] `CHANGELOG.md`: a Fixed entry citing #1665. Its TheRock and corpus sentences come from T013 and T014's measurements.
- [ ] T017 Run the full gate on the final commit, and record in `MS/gate-log.md` the per-target totals and the `>>> all pre-PR checks passed.` line. Also record that `git diff e1148818 -- Cargo.lock '*Cargo.toml'` is empty (FR-009).
- [ ] T018 Close-out. For every task above, check that its claim is true of the committed tree and measurements. Where the implementation differs from the task text, note it inline rather than tick a false claim. Then tick it.

## Dependencies

- **Setup**: T001 → T007. T002 → T007.
- **Foundational**: T003 → T004, T005 and T006 (they use the new type and variant) → T007 → T008 → T009.
- **US2** (T010) needs T009's checkpoint.
- **US1**: T011 needs T010. T012 needs T011.
- **US3** (T013, T014) needs every code change.
- **Polish**: T015 and T016 any time after T011. T017 and T018 last.

## Parallel opportunities

- T004 (`DD`), T005 (`SM`) and T006 (`CE`, `S2A`, `S3A`) touch different files once T003 lands.
- T015 with T016.
- The CI dispatches (T009, T013, T014) run while local work continues.

## Implementation strategy

1. Setup, then the log-only foundational phase, with the census read and the follow-up issue filed.
2. US2 (the technique), then US1 (the absorbed method, the MVP fix for #1665).
3. US3 traces every change against `main` before the PR.
