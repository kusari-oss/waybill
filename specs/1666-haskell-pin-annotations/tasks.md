# Tasks: Haskell pin annotations say what is true

**Input**: specs/1666-haskell-pin-annotations/ (spec, plan, research R1–R8, data-model, contracts/annotations.md)
**Tests**: required (spec FR-009, SC-005).

C172 needs a nixpkgs package set, so its tests are unit tests over a hand-built set in `enrich_tests` (`nix/haskell_packages/mod.rs:2074`) and in `twin_merge` tests, as the existing C172 tests are. #1741 is a hermetic three-format integration test.

## Phase 1: Setup

- [X] T001 Write `specs/1666-haskell-pin-annotations/measurements/measure.sh <golden-root> <label>`. For each Haskell corpus target (`hpack-*`, `haskell-*`, `nix-closure-*`), read `<golden-root>/<target>/cdx.json` and write `<label>/<target>.tsv` with these columns:
  - PURL;
  - version;
  - `waybill:source-type`;
  - C172 `emitted`, `nixpkgs` and the note's first 60 characters;
  - C221 present or absent.

  Then append a second section listing every unreachable component and its `waybill:orphan-reason`, plus C174's `disagreements`. Run it on the committed goldens as `main` (research R1).
- [X] T002 Re-run the #1741 three-file fixture with a `main` build, and record `pkg:hackage/waybill-fixture-frozen@4.0.0` as unreachable with no reason in `specs/1666-haskell-pin-annotations/measurements/fixture-1741-main.txt`.

## Phase 2: Foundational

- [X] T003 Confirm `main.tsv` reproduces the research tables: 10 main-module lockfile notes, 8 `"emitted": null` (two of them versioned), and haskell-language-server `disagreements: 11`. If it doesn't, stop and amend research before any code changes.

## Phase 3: US1 + US2 — the note names its source, and main modules are skipped (P1)

**Independent test**: unit tests in `enrich_tests` over a package set where nixpkgs differs.

- [X] T004 [US1] Write unit tests in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs` `enrich_tests`:
  - a `hackage-stack-lock` component with source path `stack.yaml.lock` gets a note naming the Stack lock file and `stack.yaml.lock`;
  - a `hackage-freeze` component with `cabal.project.freeze` gets a note naming the freeze file;
  - a component listing two lock files (spec 1665) names both, sorted;
  - a `hackage-main-module` component whose version differs gets no C172 and `summary.disagreements == 0`.
- [X] T005 [US1] In the disagreement loop (`mod.rs:1079`), add `if is_main_module(c) { continue; }` and build the note with a new `fn disagreement_note(c: &ResolvedComponent) -> String`. It maps `hackage-stack-lock` to "the Stack lock file" and `hackage-freeze` to "the freeze file", and any other type to its `source_type` value. The file list is `source_path` plus `extra_source_paths`, sorted and comma-joined, in the form `the emitted version came from <kind> <files> and takes precedence`. Run T004 green.

## Phase 4: US3 — recompute after the twin merge (P2)

**Independent test**: `twin_merge` unit tests.

- [X] T006 [US3] In `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/twin_merge.rs` tests, write two tests:
  - a design twin whose C172 has `nixpkgs` equal to the built version leaves the survivor with no C172 and with C221;
  - a design twin whose C172 has `nixpkgs` 1.0.0.0 and is built at 2.1 leaves the survivor with C172 `emitted` 2.1, no `requirement-ranges`, and a note containing "closure".

  Also update the existing assertion `"C172 stays true"` (`twin_merge.rs:389`) to check the rewritten value. Give its fixture a real C172 object (it is currently the string `"{\"nixpkgs\":\"1.0.0.0\"}"`).
- [X] T007 [US3] In `fold`, after the version is final, recompute C172 as research R4 describes:
  - remove it when the survivor's version equals `nixpkgs`, and count that in a new `TwinMergeSummary.c172_removed`;
  - otherwise rewrite `emitted`, drop `requirement-ranges`, and set the note to the data-model text.

  `enrich` inserts C172 as a JSON object (`serde_json::json!`, `mod.rs:1131`), and that is the form `fold` sees. Test that form. The string-form fixture at `twin_merge.rs:254` is test debt; make it an object in T006. Run T006 green.
- [X] T008 [US3] In `waybill-cli/src/cli/scan_cmd.rs`, at the merge call (around line 5173), subtract `twins.c172_removed` from the C174 summary's `disagreements` (research R5). Make `nixpkgs_haskell_summary` mutable and subtract at the merge site, so C174 stays owned by its pass. Add a unit or integration assertion that C174 equals the C172 count.

## Phase 5: US4 — the freeze orphan arm (P2)

**Independent test**: `waybill-cli/tests/haskell_pin_annotations.rs`.

- [X] T009 [US4] Create `waybill-cli/tests/haskell_pin_annotations.rs`. Build the #1741 fixture in a tempdir:
  - `cabal.project` listing `app`;
  - `app/waybill-fixture-app.cabal`, depending on `base`;
  - `cabal.project.freeze` pinning `waybill-fixture-frozen ==4.0.0`.

  Then add a second fixture whose app depends on `waybill-fixture-frozen`. Scan with `--offline` and an empty `HOME`. Assert:
  - in all three formats (CycloneDX, SPDX 2.3, SPDX 3), the frozen pin is unreachable with `edgeless-lock-pin`;
  - in the second fixture it is reachable and has no reason;
  - the I7 check (unreachable implies a reason) passes on the first fixture;
  - `common::parity::assert_parity` passes.

  Use fixed output names: `waybill.cdx.json`, `waybill.spdx.json`, `waybill.spdx3.json`.
- [X] T010 [US4] In `waybill-cli/src/generate/orphan_reason.rs:648`, match `Some("hackage-stack-lock" | "hackage-freeze")`. Update the comment to cite spec 1666 and #1741. Add a unit test beside the one at `:1270`. Run T009 green.

## Phase 6: Polish

- [X] T011 [P] Update `docs/reference/sbom-format-mapping.md`:
  - C172: the note names the source, main modules are excluded, and the post-merge recompute (removed when the built version equals nixpkgs, otherwise rewritten);
  - C45: `edgeless-lock-pin` covers `hackage-freeze`;
  - C174: `disagreements` counts the C172 annotations that remain.
- [X] T012 Mutation-test with `specs/1666-haskell-pin-annotations/measurements/mutate.py`, which reverts with `cp` and checks with `cmp`. The mutations are:
  - M1: the note is a fixed string again (it must kill a T004 test, since the corpus has no lock-sourced case; SC-001);
  - M2: remove the main-module skip;
  - M3: never remove C172 on equality;
  - M4: never rewrite `emitted`;
  - M5: drop the `hackage-freeze` alternative;
  - M6: no C174 subtraction.

  Each must fail at least one test. Record the results in `measurements/mutations.md`.
- [X] T013 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh`. Check `GATE_RESULT=pass` and the Summary line.
- [ ] T014 Open the PR, then dispatch `gh workflow run public-corpus.yml -f branch=1666-haskell-pin-annotations -f regen_goldens=true` and download `corpus-goldens-regen`.
  - Run `measure.sh` on it as `branch`.
  - Diff it against `main`, using a normalized, sorted diff, and explain every line in `measurements/branch.md`. The expected result:
    - 10 main-module C172 removed;
    - hspec-discover's C172 removed;
    - slacklinker slack-web's C172 rewritten;
    - 6 haskell-language-server entries unchanged;
    - `disagreements` lowered by the same counts.
  - Commit only the goldens the measurement explains, and exclude the #1738 stray element if it recurs.
- [X] T015 Add the `CLAUDE.md` Recent Changes line by hand, and tick every task only after checking its claim.

## Dependencies

- T001 → T002 → T003 must finish before any code changes.
- US1+US2 (T004–T005), US3 (T006–T008) and US4 (T009–T010) are independent, so any order works. T008 needs T007.
- The Polish phase needs every story. T014 needs the PR open.

## Parallel

- T004, T006 and T009 are in different files and can be written in parallel. T011 can run alongside the stories.

## MVP

US1+US2 (T004–T005) removes most of the wrong annotations: 10 of the 12.
