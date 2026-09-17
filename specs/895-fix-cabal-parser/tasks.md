# Tasks: Trustworthy `.cabal` dependency parsing

**Input**: Design documents from `/specs/895-fix-cabal-parser/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cabal-parsing.md, quickstart.md

**Tests**: Test tasks are included and are not optional here. SC-008 requires
each new test to fail against the current implementation *for its own reason*,
and contract A-8 requires accuracy be asserted against the scanned file rather
than against the parser's own report. Both are properties of the tests
themselves, so they are tasks.

**Organization**: Grouped by user story. Phase A (US1-US3 plus Phase 6) is
self-contained and can ship alone; Phase B adds the corpus target (research R6).

## Format: `[ID] [P?] [Story] Description`

- **[P]** — parallelisable: different file, no dependency on an incomplete task
- **[US1] / [US2] / [US3]** — the user story the task serves

---

## Phase 1: Setup

**Purpose**: The before-side and the fixture every story asserts against.

- [X] T001 Build a release binary from the merge-base and keep it at `target/release/waybill-baseline` — every teeth-check and before/after comparison in this feature uses it, and re-resolving `waybill` on `$PATH` picks up an unrelated install
- [X] T002 [P] Materialise the #891 reproducer per `quickstart.md` §2 and confirm it reproduces all four malformed identifiers against the baseline. If it does not, stop — the rest of this feature would be measuring the wrong thing
- [X] T003 [P] Record the baseline measurement in `specs/895-fix-cabal-parser/measurements/README.md`: the four emitted identifiers, the count of emitted names absent from the `.cabal` file, and the count of declared names the baseline fails to emit (SC-001, SC-002, SC-002a)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared assertion machinery. Every story compares emitted
names against the scanned file, and none of them should re-implement that.

**⚠️ CRITICAL**: No user story work begins until T005 exists — otherwise each
story invents its own accuracy check and contract A-8 is asserted three
different ways.

- [X] T004 Create the integration test target `waybill-cli/tests/haskell_cabal_parsing_m895.rs` with the two-layout reproducer written inline via `tempfile`, using `waybill-fixture-*` names per the fixture policy
- [X] T005 Add a helper in that target that extracts every dependency name declared in a `.cabal` file and compares it against the emitted component names, returning **both** the emitted-but-not-declared set (contract A-8, accuracy) and the declared-but-not-emitted set (completeness). The second direction is what the fifth defect needs and what a one-directional check would miss
- [X] T006 Add a helper returning each emitted component's identifier, declared constraint record, and lifecycle scope, so US2 and US3 assert on parsed values rather than on substring matches against a PURL

**Checkpoint**: Both accuracy and completeness are measurable and shared. Stories can now proceed in parallel.

---

## Phase 3: User Story 1 — Nothing is invented, nothing is dropped (Priority: P1) 🎯 MVP

**Goal**: Every emitted component name is a dependency name declared in the scanned file, and every declared name is emitted.

**Independent Test**: Scan a `.cabal` file whose dependency lists are followed by ordinary fields and comments, repeat `build-depends:` under comment headings, and nest one inside a conditional; both sets from T005 are empty.

### Tests for User Story 1

- [X] T007 [P] [US1] Add a test in `waybill-cli/tests/haskell_cabal_parsing_m895.rs` asserting the emitted-but-not-declared set is empty for the two-layout reproducer (FR-004, contract A-1)
- [X] T008 [P] [US1] Add a test asserting a block terminates at the following field in the hpack layout — entries at indent 4 following a first entry at indent 6 all belong to the block, and `default-language:` at the field's own indent does not (FR-001, contract A-2, research R1)
- [X] T009 [P] [US1] Add a test asserting a block terminates in the `cabal init` layout, where the first entry sits inline with the field name and continuations align under the value, and that a leading-comma list and a trailing-comma list yield the same dependency set (FR-001, DD-5, R1 Layout B)
- [X] T010 [P] [US1] Add a test asserting a `build-tool-depends` block does not absorb the `build-depends` block that follows it (FR-003)
- [X] T011 [P] [US1] Add a test asserting a comment **inside** a block contributes nothing (FR-002). Note this is a genuinely different case from the observed bug: R1 measured that a comment at field indentation is already eaten by the termination rule, so this test must place the comment deeper or it proves nothing about comment handling
- [X] T012 [P] [US1] Add a test asserting a section that repeats `build-depends:` three times under separate comment headings emits dependencies from **all three** lists (FR-001a, SC-002a). This is the fifth defect and the largest by volume — the corpus target's library section carries five such lists and the current reader returns one
- [X] T013 [P] [US1] Add a test asserting a `build-depends:` nested inside an `if` conditional is read and attributed to the enclosing section (FR-001b)
- [X] T014 [P] [US1] Add the remaining enumerated edge cases as one batched test: a list that is the final field with no trailing newline, an empty list, a file with Windows line endings, and a list in which every entry is unreadable (SC-008, FB-1, FB-2, FB-5)
- [X] T015 [US1] Teeth-check T007-T014 against `target/release/waybill-baseline` and record each observed failure in `specs/895-fix-cabal-parser/measurements/README.md`. A test that passes against the baseline is not a regression test — say which ones those are rather than counting them as coverage

### Implementation for User Story 1

- [X] T016 [US1] Replace the field-block extraction in `waybill-cli/src/scan_fs/package_db/haskell.rs` so a block ends at the first subsequent non-blank line indented at most as far as the field line itself (FB-1, FB-2, FB-3). The reference point is the **field** line, not the first entry — R1 measured entries less indented than the first that still belong to the block
- [X] T017 [US1] Replace the first-match-only extraction in `extract_build_depends_block` (which calls `re.captures`, singular) with one that reads **every** occurrence in the section, including those nested inside conditionals (FR-001a, FR-001b)
- [X] T018 [US1] Drop comment lines from a block's body before entries are split (FR-002, FB-4)
- [X] T019 [US1] Ensure a block whose indentation cannot be interpreted — mixed tabs and spaces — emits no fabricated component (FB-5). Assert the property, not a particular fallback: an implementation that parsed it correctly must not fail this
- [X] T020 [US1] Re-run the T003 measurement and confirm both counts fall to zero on the reproducer — emitted-but-not-declared AND declared-but-not-emitted (SC-001, SC-002, SC-002a)
- [X] T021 [US1] Confirm the existing Haskell integration targets still pass unchanged — `haskell_cabal_baseline`, `haskell_edge_cases`, `haskell_tier_fallbacks` — which is FR-011 and contract A-7 (SC-005)
- [ ] T022 [US1] Run `./scripts/pre-pr.sh` and enumerate the per-target `N passed; 0 failed` lines

**Checkpoint**: Both the accuracy violation and the completeness defect are fixed and independently demonstrable. This is the MVP.

---

## Phase 4: User Story 2 — A dependency is identified by what it is (Priority: P2)

**Goal**: Identifiers name packages; constraints live only in the constraint record.

**Independent Test**: Scan a project whose dependencies carry constraints; every identifier is versionless and every constraint is recoverable from all three formats.

### Tests for User Story 2

- [X] T023 [P] [US2] Add a test asserting a constrained dependency emits a versionless `pkg:hackage/<name>` identifier with no version segment and no placeholder token, and that the result parses as a valid package URL (FR-005, SC-003, EC-1, contract A-3)
- [X] T024 [P] [US2] Add a test asserting the declared constraint is recoverable verbatim from **all three** output formats, not CycloneDX alone (FR-006, FR-009, SC-004, contract A-4)
- [X] T025 [P] [US2] Add a test asserting the same package declared with different constraints in two stanzas yields ONE component carrying both constraints (FR-007, EC-3). Assert the merge, not just the count — EC-3 holds by construction once identifiers drop the version, and a count-only assertion would pass even if one constraint were dropped
- [X] T026 [P] [US2] Add a test asserting a dependency declared with no constraint and one declared with a constraint produce identically-shaped identifiers (FR-008)
- [ ] T027 [US2] Teeth-check T023-T026 against the baseline and record the observed failures (SC-008)

### Implementation for User Story 2

- [X] T028 [US2] Stop sanitising the constraint into the identifier's version segment in `waybill-cli/src/scan_fs/package_db/haskell.rs`; emit `pkg:hackage/<name>` with no version segment (FR-005)
- [X] T029 [US2] Confirm the constraint still reaches `waybill:requirement-ranges` unchanged. Per research R3 this annotation already flows to all three emitters through `extra_annotations` — verify on real output rather than assume, as the m868 equivalent turned out to be verification rather than implementation
- [X] T030 [US2] Correct the C20 label in `docs/reference/sbom-format-mapping.md` from `waybill:requirement-range` to `waybill:requirement-ranges`, matching emission and all three extractors (FR-009). No extractor change is needed — the gate keys on `row_id`, which is why this typo has never failed a build (R3)
- [X] T031 [US2] Confirm the `ghc` / stackage-resolver placeholder still emits `@unspecified` and `haskell_stack_discrimination` still passes — that path is out of scope and must not be swept up (R2)
- [ ] T032 [US2] Run `./scripts/pre-pr.sh`

**Checkpoint**: Identifiers are resolvable in shape; constraints are preserved.

---

## Phase 5: User Story 3 — A build tool is not a library (Priority: P3)

**Goal**: Build-tool declarations are distinguishable from library dependencies and name real packages.

**Independent Test**: Scan a project declaring both kinds; the tool's identifier names a package, it is marked build-time, and the library dependency is not.

### Tests for User Story 3

- [X] T033 [P] [US3] Add a test asserting a `package:executable` declaration emits `pkg:hackage/<package>` — the package half alone, which is a real Hackage coordinate (FR-010a, DD-4, contract A-6)
- [X] T034 [P] [US3] Add a test asserting the build tool is marked build-time and a library dependency in the same file is not (FR-010, EC-4)
- [X] T035 [P] [US3] Add a test asserting the declared executable name is recoverable from the emitted document (FR-010b, EC-4)
- [X] T036 [P] [US3] Add a test asserting a package declared as **both** a build tool and a library dependency is not marked exclusively build-time (EC-5, US3 scenario 4) — the asymmetry being that over-reporting a runtime dependency is recoverable and hiding one from a runtime filter is not
- [ ] T037 [US3] Teeth-check T033-T036 against the baseline and record the observed failures (SC-008)

### Implementation for User Story 3

- [X] T038 [US3] Model the declaration kind and the executable name as fields rather than string conventions in `waybill-cli/src/scan_fs/package_db/haskell.rs`, so a `package:executable` string cannot flow into a package-name slot again (Principle IV, DD-1)
- [X] T039 [US3] Split `package:executable` into package and executable, emit the package as the identifier, and carry the executable as an annotation (DD-4, FR-010a, FR-010b)
- [X] T040 [US3] Mark build-tool components build-time using the lifecycle vocabulary already in the codebase — do not introduce a new marker (FR-010, EC-4)
- [ ] T041 [US3] Run `./scripts/pre-pr.sh`

**Checkpoint**: Phase A's stories are complete.

---

## Phase 6: Partial-failure reporting (cross-cutting)

**Purpose**: FR-012 applies to every story's parsing path, so it lands once rather than three times.

- [ ] T042 [P] Add a test asserting one unreadable entry among N emits N-1 components and reports a skip count of 1, with no sibling lost (FR-012, SC-006, contract A-5)
- [ ] T043 [P] Add a test asserting the skip count is reported even when zero, so "fully readable" stays distinguishable from "count missing" (FR-012b). Feed a document with the field absent to confirm the check fails on absence rather than only on a wrong value
- [ ] T044 Teeth-check T042-T043 and record what the baseline does (SC-008) — note it currently detects nothing as unreadable, so these may be regression guards rather than defect-catchers; say which
- [ ] T045 Implement per-entry skip with a counter in `waybill-cli/src/scan_fs/package_db/haskell.rs`, counting entries rather than files or bytes (FR-012, DD-3, SEC-3)
- [ ] T046 Emit the count document-scope, present when the reader ran and absent otherwise (FR-012a, SEC-1, SEC-2)
- [ ] T047 Add the catalogue row for the skip count in `docs/reference/sbom-format-mapping.md` **and** its three extractor entries in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` in the same commit — a row without an extractor fails `every_catalog_row_has_an_extractor` and `holistic_parity`
- [ ] T048 Run `./scripts/pre-pr.sh` and confirm zero golden churn — no Haskell project is in either corpus, so no committed fixture should move (R5)

**Checkpoint**: Phase A complete. The parser fix is shippable on its own.

---

## Phase 7: Corpus target (Phase B — can land separately)

**Purpose**: SC-002, SC-002a and SC-007. Makes the accuracy and completeness guarantees nightly checks rather than one-off measurements.

**⚠️ Do not start before Phase A lands.** Adding the target first would commit goldens containing the fabricated components and the dropped dependency lists, then immediately refresh them (R6).

- [ ] T049 Confirm the chosen repository is still cabal-only at the revision to be pinned — a `cabal.project.freeze` or `stack.yaml.lock` would route the scan down the lockfile path and the target would not exercise this feature at all. R4 measured `haskell/aeson` and `haskell/text` as suitable and rejected a third candidate on exactly this ground; re-check rather than trust the recorded result
- [ ] T050 Confirm the pinned revision still exhibits the shapes this feature fixes — repeated `build-depends:` fields and conditional nesting. Measured on `haskell/aeson`: five lists in `library`, three in `test-suite`. A target that no longer exercises them proves nothing
- [ ] T051 Mirror the chosen repository to a `kusari-sandbox` fork and pin by SHA so the pin cannot move underneath the gate, matching the existing corpus precedent
- [ ] T052 Record the pinned revision's declared dependency count, so SC-002 and SC-002a have a denominator rather than being bare assertions
- [ ] T053 Add the target to `waybill-cli/tests/corpus_harness_195/manifest.rs` with its layer-1 invariants
- [ ] T054 Generate the target's goldens **through CI** (`regen_goldens=true`), never locally — goldens embed runner-absolute paths and a locally-generated one passes only on the generating machine
- [ ] T055 Review every diff before installing, and attribute each category. A target being new does not make its first golden self-evidently correct
- [ ] T056 Install the goldens and confirm the lane passes on the branch with `regen_goldens=false` — the same comparison the nightly runs (SC-007)

---

## Phase 8: Polish

- [ ] T057 [P] Update `specs/895-fix-cabal-parser/measurements/README.md` with post-fix figures beside the baselines, so the before/after pair stays reproducible
- [ ] T058 [P] Confirm scan time has not regressed. If a difference is observed, establish it by interleaved A/B on identical machine state — a separately-taken baseline attributed machine drift to the change once already in this project
- [ ] T059 [P] Verify the walker-audit gate separately; it is not in `scripts/pre-pr.sh`. Expected to be a no-op since no file under `scan_fs/walk*` is touched, but confirm rather than assume
- [ ] T060 [P] Comment on #891 with the outcome, naming which change closed each of the four original defects **and** the fifth found during analysis
- [ ] T061 Open the PR with `./scripts/pre-pr.sh` green and every per-target `N passed; 0 failed` line enumerated

---

## Dependencies

```
Setup (T001-T003)
  └─> Foundational (T004-T006)         <- shared accuracy + completeness measurement
        ├─> US1 (T007-T022)  P1  MVP
        ├─> US2 (T023-T032)  P2        } independent of each other
        └─> US3 (T033-T041)  P3        }
              └─> Partial-failure (T042-T048)   <- touches the same parsing path
                    └─> Corpus target (T049-T056)   <- Phase B, needs Phase A landed
                          └─> Polish (T057-T061)
```

**Story independence**: US1, US2 and US3 change different things — block
extraction and termination, identifier construction, and declaration-kind
modelling respectively — and can be implemented in any order once T006
exists. US3's *version* half benefits from US1 but does not require it: the
`package:executable` split is independent of where a block ends.

**Phase 6 is sequenced after the stories** because per-entry skip handling
touches the same entry-splitting code all three modify, and landing it first
would force three merges through it.

## Parallel execution

Within each story's test block, every `[P]` task is a separate assertion in
the same new file and can be written concurrently. Across stories, T007-T014,
T023-T026 and T033-T036 are mutually independent once T006 lands.

The implementation tasks within a story are **not** parallel — T016, T017,
T028 and T038 all edit `haskell.rs`.

## Implementation strategy

**MVP is US1 alone.** It closes both the accuracy violation and the
completeness defect, which together are the only failures that make the
document actively wrong rather than merely awkward. US2 and US3 improve
identifiers that are, after US1, at least naming things that exist and
naming all of them.

**Phase A (US1-US3 plus Phase 6) ships without Phase B.** The corpus target
is worth having and is not worth blocking an accuracy fix on repository
administration.
