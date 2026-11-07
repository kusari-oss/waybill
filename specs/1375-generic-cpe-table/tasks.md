# Tasks: the curated CPE table

**Feature**: `1375-generic-cpe-table` | **Issue**: #1381
**Input**: [spec.md](./spec.md), [plan.md](./plan.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/table.md](./contracts/table.md), [quickstart.md](./quickstart.md)

**Tests**: the enforcement lands before the data, so a wrong row cannot be
added even momentarily. That inverts the story priority and is deliberate.

**The dispositions are already decided.** `research.md` R4 holds all 51, with
the titles that justify each. These tasks transcribe that decision; they do
not re-make it, and a task that finds R4 wrong should say so rather than
quietly differ from it.

---

## Phase 1: Setup and baseline

- [ ] T001 Capture the pre-change document by scanning the pinned moat clone with `--nix-closure --no-clearly-defined`. **Copy the clone before scanning** — evaluating a flake in place can re-lock it, which broke 11 tests on 2026-11-04 — and verify with `md5 -q` on `flake.lock` either side
- [ ] T002 [P] Record the pre-change set of `{name, cpe, cpes}` for every component **that already carries a CPE**, in `measurements/README.md`. SC-005 diffs against this set; a count would stay equal while membership changed
- [ ] T003 [P] Record the pre-change C209 value — expected `78 / 166 / 194 / 539 / 0` on aarch64-darwin — so the ±N delta is measured against a captured figure rather than a remembered one
- [ ] T004 [P] Record which committed goldens contain any of the 48 accepted names as a `pkg:generic` component. **This is the predicted golden blast radius**, and unlike the last two milestones it is *not* every document: C209 is universal but its value only moves where an affected component exists

---

## Phase 2: Foundational — the enforcement, before the data

**Blocking, and ordered ahead of US1 on purpose.** US1 carries the value and
is also the step that can introduce a silent wrong row, so the guard goes in
first and the 48 rows are added into a file that already refuses an
unobserved pair.

- [ ] T005 (FR-001, SC-001) Add the `NVD_DICTIONARY_OBSERVED` entries for every new pair in `waybill-cli/src/generate/cpe.rs`, transcribed from `measurements/nvd-vendor-discovery.tsv`, **before** any table row that uses them
- [ ] T006 Confirm `every_table_pair_exists_in_the_nvd_dictionary` still passes with the observations added and no rows yet — the observations are a superset of the table, which the test permits in that direction
- [ ] T007 **Prove the guard has teeth**: add a row whose pair has no observation, confirm the test fails with the message naming the pair, then remove it. A guard nobody has seen fail is a guard nobody knows works

---

## Phase 3: User Story 2 — a wrong row cannot be added silently (P2)

**Goal**: the enforcement is in place and demonstrated before the data lands.

**Independent test**: adding a table row without a recorded observation fails
the suite, with a message that names the pair and the script to run.

- [ ] T008 [US2] (FR-006) Add a test in `waybill-cli/src/generate/cpe.rs` asserting the **vendor is NVD's, not the component name**, for the least-guessable rows: `bash`→`gnu`, `expat`→`libexpat`, `libxml2`→`xmlsoft`, `brotli`→`google`, `gawk`→`fossies`. These are the ones a future edit is most likely to "correct" to `(name, name)`
- [ ] T009 [P] [US2] Add a test asserting `GENERIC_LIBRARY_CPES` has no duplicate slug, so a second `bash` row cannot shadow the first unnoticed
- [ ] T009a [US2] (FR-002, SC-002) Add a test asserting **every slug in the table appears in the committed `measurements/nvd-titles.txt`**, so a row cannot exist without the title evidence that justified it. The *judgement* is not machine-checkable — `ninja` would satisfy any string test — but its **traceability** is, and this closes the loop the dictionary observation leaves open: `every_table_pair_exists_in_the_nvd_dictionary` proves a pair is in NVD, this proves someone looked at what it is

---

## Phase 4: User Story 1 — a scanner can match a closure's C libraries (P1)

**Goal**: the 48 accepted names emit the vendor NVD files them under.

**Independent test**: scan the pinned closure; components whose names have a
verified row carry a CPE with the discovered vendor.

- [ ] T010 [US1] Add the **33 single-vendor rows** to `GENERIC_LIBRARY_CPES` per `research.md` R4, in the order R4 lists them so a reviewer can follow the dispositions: the 15 `gnu` tools, the 7 own-name vendors, the 5 `*_project` vendors, and `expat`, `libxml2`, `libxslt`, `compiler-rt`, `ngtcp2`, `gawk`
- [ ] T011 [US1] (FR-004) Add the **9 multi-vendor rows**, each emitting a candidate per legitimate vendor: `c-ares`, `file` (3 vendors), `jq`, `less`, `libarchive`, `libuv`, `ncurses`, `unzip`, `xz`
- [ ] T012 [US1] Add the **6 partial rows**, each dropping exactly one vendor, with the dropped vendor named in a comment: `clang` (−`arm`), `patch` (−`tanium`), `flex` (−`apache`), `brotli` (−`andybalholm`), `bzip2` (−`bzip2_project`), `libidn2` (−`gnu`). **The dropped vendor is the point of each row**; without the comment the next reader sees an incomplete row rather than a decided one
- [ ] T013 [US1] Verify against the pinned closure that **49 components** gain a CPE and that each carries the vendor R4 assigned — not a count alone, the actual vendor per component
- [ ] T014 [US1] Verify the C209 delta is exactly `with_candidates 78 → 127` and `unaddressable 194 → 145`, with `addressable_without_cpe`, `excluded_members` and `excluded_files` **unchanged** and the five still summing to 977 (FR-008, SC-004) — quickstart Control 3

---

## Phase 5: User Story 3 — the rejections stay rejected (P3)

**Goal**: a future contributor meets the reasoning before re-adding `ninja`.

- [ ] T015 [US3] (FR-003, SC-003) Record the three rejections in `waybill-cli/src/generate/cpe.rs` **beside the table**, each with the title that justified it: `ninja` (126 entries, all other products), `cmake` (only vendor is an npm package; `kitware:cmake` = 0), `libxcrypt` (one entry, an automotive vendor). **In the source, not only in the spec** — a contributor adding `ninja` is editing that file
- [ ] T016 [US3] Add a test asserting `ninja`, `cmake` and `libxcrypt` yield **no** CPE, with a **control** proving `synthesize_cpes` produces candidates at all — otherwise the assertions pass just as well when the function is broken outright
- [ ] T017 [P] [US3] Record in `measurements/README.md` the disposition of **all 186 names** — 48 accepted, 3 rejected, 53 nixpkgs construction names structurally absent, 82 canonical names NVD does not carry — so none is left in an unknown state and the population is answered once (FR-007, SC-006)

---

## Phase 6: Polish and verification

- [ ] T018 Verify **no existing CPE changed**: diff the T002 set against the post-change set and confirm additions only, no modified lines (FR-005, SC-005) — quickstart Control 4
- [ ] T019 [P] Spot-check the emitted strings for the least-guessable vendors against quickstart Control 1: `bash`→`gnu:bash`, `expat`→`libexpat:expat`, `libxml2`→`xmlsoft:libxml2`
- [ ] T020 **Mutation-check the suite**, recording which tests go red in `measurements/README.md`. Change one row's vendor to the component name → T008 must fail. Delete an observation → T006 must fail. Add a `ninja` row → T016 must fail. Remove the control from T016 and break `synthesize_cpes` → T016 must still fail, or the control is not doing its job. **Revert each mutation with `cp` from a backup, never `git checkout`**: on an uncommitted file that deletes the work under test while the gate still passes
- [ ] T021 [P] Update whichever doc records the curated table's coverage to state 60 rows and the three deliberate absences
- [ ] T022 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` — no `+toolchain` (#1189). Assert on the positive signal and the per-target `test result: ok. N passed; 0 failed` lines. **Enumerate the `m1375_*` tests the run executed** and compare against this file; a missing test is invisible in a pass count
- [ ] T023 Regenerate any local goldens that move, using **all six** writer test files — `cdx_regression`, `spdx_regression`, `spdx3_regression`, `pkg_alias_binding_us1`, `oci_pull_backward_compat`, `optional_dep_classification` — and verify the moved set matches T004's prediction. **A golden moving on a fixture containing none of the 48 names is a finding**, not a refresh
- [ ] T024 Refresh the corpus goldens through `public-corpus.yml` workflow dispatch with `regen_goldens=true` — never locally — and verify the run's reported scanned commit is this branch's tip rather than assuming it
- [ ] T025 Account for **every** differing corpus file from the artifact rather than the run summary (#1286), keep `image-postgres16` out of this feature's diff (#1372), and record the **x86_64-linux** delta, which is a different closure and must be measured rather than scaled from the darwin figures
- [ ] T026 [P] Update issue #1381 with the delivered figures for both hosts, and with the disposition summary so the 51-candidate question is answered in the issue rather than only in this directory

---

## Dependencies

```text
Phase 1 (T001-T004)  baseline + predicted blast radius
        │
Phase 2 (T005-T007)  the enforcement, and proof it fails
        │
Phase 3 (T008-T009)  US2 — the vendor and duplicate guards
        │
Phase 4 (T010-T014)  US1 — the 48 rows                      ← the value
        │
Phase 5 (T015-T017)  US3 — the rejections, recorded
        │
Phase 6 (T018-T026)  verification and goldens
```

**US2 precedes US1 although US1 is P1.** The guard is what stops a wrong row,
so it lands before the rows do. US3 is independent of both and could ship
alone, but is ordered after US1 because the rejections read best beside the
table they are absent from.

**T010–T012 are sequential**, not parallel: they edit the same `const` block.

## Parallel opportunities

- **T002, T003, T004** — three independent captures.
- **T009** alongside T008 — different concerns, same file, independent.
- **T017** alongside T015/T016 — a measurements file, not source.
- **T019, T021, T026** — spot-checks, docs, the issue.

## MVP

**Phase 1 + Phase 2 + Phase 3 + Phase 4** (T001–T014). That is the 49
components made matchable, with the guard that keeps them right. US3 makes the
rejections durable, which matters for the next contributor rather than for
this document's correctness.

## Task count

| phase | tasks |
| --- | --- |
| Setup / baseline | 4 |
| Foundational — enforcement | 3 |
| US2 (P2) | 3 |
| US1 (P1) | 5 |
| US3 (P3) | 3 |
| Polish | 9 |
| **total** | **27** |
