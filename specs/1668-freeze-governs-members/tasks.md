# Tasks: A cabal.project.freeze governs the packages its cabal.project lists

**Input**: specs/1668-freeze-governs-members/: spec, plan, research R1–R4, data-model, contracts/annotations.md
**Tests**: required (spec SC-001–SC-004).

## Phase 1: Setup

- [X] T001 Write `specs/1668-freeze-governs-members/measurements/fixture.sh <waybill> <out>`. It builds these `waybill-fixture-*` trees in tempdirs:
  - (a) the #1744 fixture: `packages: app`; a root freeze file pinning `waybill-fixture-frozen ==4.0.0` and the unused `waybill-fixture-unused ==1.0.0`;
  - (b) `packages: libs/*`, with `libs/x/waybill-fixture-x.cabal` and `libs/y/waybill-fixture-y.cabal`; `y` depends on `waybill-fixture-frozen`;
  - (c) `packages: ./app/`;
  - (d) `packages: app/waybill-fixture-app.cabal`;
  - (e) the (a) member as hpack `app/package.yaml`, with no `.cabal`;
  - (f) the control `packages: .` with the `.cabal` at the root;
  - (g) a package directory `other/` that `cabal.project` doesn't list, plus a `../outside` entry in `packages:`.

  In (a)–(e), the member also depends on `waybill-fixture-loose`, which the freeze file doesn't pin. Record that component and the member's main-module annotations.

  It scans each tree in all three formats and records three things per tree in `<out>/summary.tsv`:
  - every `waybill-fixture-frozen` and `waybill-fixture-unused` PURL;
  - whether the member has an edge to each;
  - each one's C45 value.

  Run it with a `main` build into `measurements/main/`. Expected: (a)–(e) give 2 frozen components and `edgeless-lock-pin` on `@4.0.0`, and (f) gives 1. Record what (g) does.
- [X] T002 [P] Write `measurements/corpus.sh <golden-root> <label>`. For each `public_corpus/<target>` it lists:
  - every component with C45 `edgeless-lock-pin`;
  - every versionless `pkg:hackage` component whose name a freeze file in that target pins.

  It writes `<label>/corpus.tsv`. Run it as `main` over the committed goldens.

## Phase 2: Foundational

- [X] T003 Confirm that T001 reproduces #1744 and that research R1 holds. Settle the hpack lookup point with fixture (e). Record the expected corpus population from T002. If anything differs, amend the spec and research before changing code.

## Phase 3: US1, a member's frozen dependency is one pinned component (P1)

**Independent test**: `waybill-cli/tests/haskell_freeze_members.rs`, plus unit tests in `haskell.rs`.

- [X] T004 [US1] In `waybill-cli/src/scan_fs/package_db/haskell.rs`, take the per-entry expansion out of `resolve_root_declared_members` into `fn cabal_project_member_dirs(text: &str, candidates: &BTreeSet<String>) -> BTreeSet<String>`, as research R3 describes. `resolve_root_declared_members` calls it. Every existing `resolve_root_declared_members` and spec 1662 membership unit test must pass unchanged.
- [X] T005 [US1] Add unit tests in `haskell.rs` for `fn lock_governed_dirs(lock_dir: &Path, members: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf>`, which returns the lock's directory plus its members, canonical and unique. The tests cover:
  - with no members, it returns only the lock directory;
  - members are canonicalised and deduplicated;
  - a member equal to the lock directory appears once.

  Then implement it.
- [X] T006 [US1] In Phase C of `haskell.rs` (around line 1113), replace the inline `governed` vector with `lock_governed_dirs(dir, <stack.yaml packages joined to dir>)`. Behaviour must not change: the spec 1663 tests and `hpack-hs-opentelemetry` measurements stay the same.
- [X] T007 [US1] In Phase B of `haskell.rs` (around line 1069), when `dir/cabal.project` exists:
  - hoist `package_dirs` out of the `root_declared_member_dirs` block (~line 875) so Phase B can use it;
  - build candidates from the discovered package directories (`.cabal` and `package.yaml` parents) under `dir`, keyed relative to `dir`;
  - call `cabal_project_member_dirs`;
  - map the keys back to `dir.join(key)`;
  - register exact pins under every directory `lock_governed_dirs` returns.

  With no sibling `cabal.project`, only `dir` is governed. Leave `successful_freeze_dirs` as it is unless T003 shows that Phase G needs it per member.
- [X] T008 [US1] Create `waybill-cli/tests/haskell_freeze_members.rs` with these tests:
  - `freeze_governs_subdirectory_member`, on fixture (a). It checks:
    - exactly one `waybill-fixture-frozen` component, `@4.0.0`;
    - an edge to it from the member;
    - no C45 on it;
    - its `waybill:source-files` includes `cabal.project.freeze`;
    - `common::parity::assert_parity`.
  - `freeze_governs_every_packages_form`, on fixtures (b)–(e), with the same assertions.
  - `unpinned_member_dep_is_unchanged` (FR-011): `waybill-fixture-loose` is one versionless component reached from the member, and the main module's annotations equal T001's `main` record except `waybill:sbom-tier`, which is `source` as in control (f).
  - `unused_freeze_pin_stays_edgeless`: `waybill-fixture-unused@1.0.0` has C45 `edgeless-lock-pin` in all three formats.
  - `unlisted_directory_is_not_governed`: fixture (g) matches what T001 recorded on `main`.

## Phase 4: US2, the corpus changes only where explained (P2)

- [ ] T009 [US2] After the PR is open:
  - dispatch `gh workflow run public-corpus.yml -f branch=1668-freeze-governs-members -f regen_goldens=true`, then `gh run download <id> -n corpus-goldens-regen`;
  - run `corpus.sh` as `branch`, and `fixture.sh` into `measurements/branch/`;
  - compare with `main` using a normalized, sorted diff, and explain every difference in `measurements/branch.md`;
  - commit only explained goldens. Exclude the #1738 stray `Organization "Microsoft"` element if it appears.

## Phase 5: Polish

- [X] T010 Mutation-test with `measurements/mutate.py`. It reverts each mutation with `copyfile` plus `utime`, checks the revert with `cmp`, and rebuilds; an unmutated control run is included. The mutations:
  - M1: Phase B ignores members, so only its own directory is governed;
  - M2: `cabal_project_member_dirs` drops glob matching;
  - M3: it drops `.cabal` entries naming their directory;
  - M4: `lock_governed_dirs` drops the lock directory.

  Each mutation must fail at least one test. Record the results in `measurements/mutations.md`.
- [X] T011 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh`. Confirm `GATE_RESULT=pass` and the Summary line.
- [ ] T012 Commit with the trailer `Assisted-by: Claude:claude-opus-5-5`, and open the PR with "Refs #1744, #1741".
- [X] T013 Add the `CLAUDE.md` Recent Changes line by hand. Tick each task only after checking its claim.

## Dependencies

- T001–T003 come before any code change.
- T004 and T005 come before T006 and T007; T008 needs T007.
- T009 needs the PR (T012). T010 needs T008.

## Parallel

- T001 and T002 can run together.
- T004 and T005 touch the same file, so they run in sequence.

## MVP

US1 (T004–T008) closes #1744.
