# Tasks: Nix-built Haskell packages count as project members

**Input**: `specs/1664-nix-built-members/` (spec.md, plan.md, research.md, data-model.md, contracts/membership.md)
**Tests**: required by spec FR-008 (positive control plus mutation testing).

Format: `- [ ] Txxx [P?] [Story?] description (path)`.

## Phase 1: Setup

- [X] T001 Record the `main` baseline: copy `specs/1664-nix-built-members/measurements/rejected.tsv` to `measurements/rejected.main.tsv`. Write the C248 count on `main` (0, because the row does not exist yet) into `measurements/membership.main.txt`.

## Phase 2: Foundational (blocks every story)

- [X] T002 Create `waybill-cli/src/scan_fs/package_db/haskell/nix_refs.rs` and declare it in `haskell.rs` next to `mod hpack;`. It provides:
  - `strip_comments(&str) -> String`, which removes `#` to end of line and `/* … */`, and does not treat a `#` inside `"…"` or `''…''` as a comment;
  - `callcabal2nix_paths(&str) -> Vec<(String, usize)>`, which returns the literal path and the 1-based line of the `callCabal2nix` token. It skips `callCabal2nixWithOptions`, needs a quoted name next, and then needs an unquoted token starting `./` or `../` with no `${`. Tokens may be separated by whitespace that includes newlines (research R3);
  - `resolve(nix_file: &Path, literal: &str, rootfs: &Path) -> Option<PathBuf>`, a lexical normalisation that returns `None` outside `rootfs`.
- [X] T003 Add on-demand `.nix` discovery in `waybill-cli/src/scan_fs/package_db/haskell.rs`: `discover_nix_files`, called by `nix_admit` through a closure only when something is rejected. *As implemented:* the shared-walker pattern was dropped because it registered a false `flake.nix` claim in the m924 repo report (see the plan's implementation note).

## Phase 3: User Story 1 — a package Nix builds is a member (P1) 🎯 MVP

**Independent test**: in the fixture, package (i) is emitted with C18 naming the `.nix` file and with C248 = `nix-callcabal2nix`.

- [X] T004 [P] [US1] Unit tests in `nix_refs.rs` for the recognised forms R1 found:
  - `haskellPackages.callCabal2nix "a" ../../x { }`;
  - `hfinal.callCabal2nix` across three lines (the haskell-security-advisories shape);
  - `./.`;
  - `../` resolution;
  - resolution that escapes the root gives `None`.
- [X] T005 [US1] Add the lazy build of `nix_admitted: BTreeMap<PathBuf, BTreeSet<PathBuf>>` to the #1032 block in `haskell.rs`. Read the `.nix` files once, only when some project's `declared` would drop a candidate, and keep only directories that hold a candidate `.cabal` or stand-in. A candidate under `project_dir` whose directory is in `nix_admitted` is retained. When `declared` did not contain it, record it in `nix_members: BTreeMap<PathBuf /*manifest*/, BTreeSet<PathBuf>>` (research R5). Extend the INFO log with `admitted_by_nix`.
- [X] T006 [US1] Apply the same union in `resolve_root_declared_members` in `haskell.rs`, so that `--project-discovery=root-only` agrees (research R5).
- [X] T007 [US1] On the emit path in `haskell.rs`, for the main-module component of a manifest in `nix_members` (`.cabal` and `package.yaml` alike). Its declared dependencies share the manifest but were not admitted, so they carry neither (implementation note): append the rootfs-relative `.nix` files to `extra_source_paths`, sorted and deduplicated, and set `extra_annotations["waybill:membership-source"] = "nix-callcabal2nix"`.
- [X] T008 [US1] Add row C248 `waybill:membership-source` to `docs/reference/sbom-format-mapping.md` with the R4 Principle V audit, and add a one-sentence note to C18 that a Nix-admitted member lists its `.nix` file.
- [X] T009 [US1] Add C248 parity extractors in `waybill-cli/src/parity/extractors/cdx.rs`, `spdx2.rs` and `spdx3.rs`, registered in `mod.rs`, as a per-component `SymmetricEqual` following an existing per-component annotation row.
- [X] T010 [US1] Integration test `waybill-cli/tests/nix_built_membership.rs`. The fixture `waybill-fixture-root` holds a `cabal.project` with `packages: */*.cabal` and the following:
  - (i) `tools/waybill-fixture-built/` is built by `nix/pkgs/built.nix` with `haskellPackages.callCabal2nix "waybill-fixture-built" ../../tools/waybill-fixture-built { }`. Assert it is present, with C18 containing `nix/pkgs/built.nix` and with C248.
  - (i) is named a second time, from `nix/overlay.nix`. Assert that C18 lists both `.nix` files, sorted (spec edge case: more than one Nix file).
  - (i)'s `.cabal` has `build-depends: waybill-fixture-dep`. Assert `pkg:hackage/waybill-fixture-dep` is emitted and that (i) has an edge to it (spec US1: dependencies are emitted).
  - (iv) `lib/` matches the glob and is also named by a `callCabal2nix`. Assert it is present and carries **no** C248.
  - (v) a `callCabal2nix` names `tools/empty/`, which has no manifest. Assert that no component's source files lie under `tools/empty/` (spec edge case: a path with no manifest admits nothing).

## Phase 4: User Story 2 — nothing else becomes a member (P1)

**Independent test**: in the same fixture, (ii) and (iii) are absent.

- [X] T011 [P] [US2] Unit tests in `nix_refs.rs` for the exclusions. Each must return no path:
  - a `#` comment and a `/* */` comment containing `callCabal2nix "x" ./x`;
  - `"${src}/x"`;
  - a variable `path`;
  - `(builtins.fetchTarball { … })`;
  - `callCabal2nixWithOptions "x" ./x "" { }`;
  - `/abs/x`;
  - `src = ./x;` without a call.
- [X] T012 [US2] Extend `tests/nix_built_membership.rs` with (ii) `tools/waybill-fixture-attr/`, named only as `src = ../../tools/waybill-fixture-attr;` in a non-build attribute, and (iii) `tools/waybill-fixture-interp/`, built only through `"${root}/tools/waybill-fixture-interp"`. Assert both are absent, and assert that exactly one of the three two-level candidates is a member (SC-003).
- [X] T013 [US2] Add laziness tests (research R6).
  - In `tests/nix_built_membership.rs`: a repository whose `cabal.project` rejects nothing produces byte-identical output with and without its `.nix` files.
  - A unit test in `haskell.rs` checks that the `.nix` read is skipped when nothing is rejected. Gate the read behind a function that takes the rejection set and returns early when it is empty, and test that function so M9 is detectable.

## Phase 5: User Story 3 — the choice is evidenced (P2)

- [X] T014 [US3] Rerun `measurements/rejected.sh` with the branch release binary into `measurements/rejected.branch.tsv`. Count C248 per target into `measurements/membership.branch.txt`. Expected: temporal rejected 1 → 0 and C248 only on `tix-to-markdown`. Explain every other difference in `research.md` under a new "R7 — Branch measurement".
- [X] T015 [US3] Run the mutations and record them in `specs/1664-nix-built-members/measurements/mutations.md`:
  - M1: accept `WithOptions`;
  - M2: accept quoted paths;
  - M3: skip comment stripping;
  - M4: allow `${`;
  - M5: drop the root-containment check;
  - M6: omit C248;
  - M7: omit the `.nix` file from source-files;
  - M8: mark C248 on directories already in `declared`;
  - M9: drop the lazy guard, so `.nix` files are always read. T013's unit test catches this.
  - M10: stop accepting newlines between tokens. The multi-line test catches this.
  - M11: require a bare `callCabal2nix` with no attribute prefix. The prefix test catches this.
  - Also confirm with grep that `nix_refs.rs` and the read path contain no `Command`, which is SC-005's evidence.
  
  Each mutation uses a `cp` backup, a `cmp` revert and a rebuild, and records the test that failed.

## Phase 6: Polish

- [X] T016 Add a CLAUDE.md "Recent Changes" line for 1664, by hand (the context script deletes lines).
- [X] T017 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh`, and check `GATE_RESULT=pass` and the Summary line.
- [X] T018 Open the PR ("Refs #1731" until the goldens land). Dispatch `public-corpus.yml` with `regen_goldens=true`, and commit only the goldens T014 explains (expected: `hpack-hs-temporal-sdk` gains `tix-to-markdown`, `hpc` and C248). Confirm no `invariant-declarations.toml` row moves unless R7 explains it.

## Dependencies

- T001 → everything else. T002 and T003 block every story.
- US1 (T004–T010) comes before US2's integration tasks (T012–T013), because they share the fixture file. T011 can run in parallel with T004.
- US3 (T014–T015) needs US1 and US2 complete. Polish comes last.

## Parallel

- T004 ∥ T011 (separate test blocks in the new module; write them in sequence if the file conflicts).
- T008 ∥ T009 (docs and extractors).

## Strategy

MVP is US1 plus US2 together, because a member rule with no exclusions isn't shippable (spec US2 rationale). One gate, one PR.
