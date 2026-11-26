# Tasks: Stack snapshots pinned by URL

**Input**: `specs/1663-stack-snapshot-url/` (spec.md, plan.md, research.md, data-model.md, quickstart.md)
**Tests**: required (FR-007: a positive control and a mutation-tested test for each naming form).

## Phase 1: Setup (the baseline, before any source edit)

- [X] T001 Build `main`'s release binary (`cargo build --release -p waybill`) and copy it to the scratchpad as `waybill-main`. Record its SHA in `specs/1663-stack-snapshot-url/measurements/snapshots.md`.
- [X] T002 Write `specs/1663-stack-snapshot-url/measurements/measure.sh`, adapted from spec 1662's. For every Haskell corpus target at its `manifest.rs` pin, plus the no-generated-cabal variants, it records:
  - the snapshot components (purl, `source_path`, `extra_source_paths`, reachable from the root, `waybill:orphan-reason`);
  - the sorted `pkg:hackage` set;
  - the count of `failed to parse stack.yaml.lock` warnings in the log.

  Scan with the harness's flags and an empty `WAYBILL_NIXPKGS_CACHE` and `HOME`, so local caches can't affect the result (the spec 1662 lesson). Rewrite paths under `<work>`.
- [X] T003 Run T002 with `waybill-main`, and write `measurements/main/` and the summary in `snapshots.md`. Positive control: hs-opentelemetry shows `pkg:generic/unknown@93a10755…` with no orphan reason, and 1 parse warning.

## Phase 2: Foundational

- [X] T004 In `waybill-cli/src/scan_fs/package_db/haskell.rs`, make `StackSnapshotYaml.original` accept a string or a map with `resolver` / `url` (an untagged enum), and keep `completed.url`. Write the failing test first: `parse_stack_lock` on a lock with `original: lts-24.35` returns its snapshot and packages (FR-000).
- [X] T005 Add `SnapshotId` (`Resolver(String)` / `Url(String)`) and `fn snapshot_identity(original, completed_url) -> Option<SnapshotId>` in `haskell.rs`, per research R5, R3 and R4:
  - the stackage path match is the last segments, digits only;
  - nightly parts are zero-padded.

  Extend `StackSnapshot` with `id` and `source: PathBuf`, set by `parse_stack_lock` and by `extract_resolver_from_stack_yaml`.

## Phase 3: User Story 1, a URL-pinned snapshot is named (P1) 🎯 MVP

**Independent Test**: the four naming forms give the identities in data-model.md, and the same snapshot pinned by name and by URL has one PURL.

- [X] T006 [US1] Unit tests in `haskell.rs` for `snapshot_identity`:
  - an `lts` URL → `lts-18.8`;
  - a `nightly` URL `…/nightly/2026/4/4.yaml` → `nightly-2026-04-04`;
  - a mirror host with the same path → named;
  - a non-numeric part → URL form;
  - a non-stackage URL → URL form;
  - a string resolver, and a map resolver;
  - `completed.url` used when `original` has neither;
  - `original.url` wins when it differs from `completed.url`.
- [X] T007 [US1] In `build_snapshot_placeholder` (`haskell.rs`):
  - a `Resolver` keeps today's PURL shapes;
  - a `Url` → `pkg:generic/stack-snapshot@<sha|unspecified>?download_url=<url>`;
  - `waybill:stackage-resolver` only for a `Resolver`.

  Update the existing placeholder unit tests where the struct changed, and add a URL-form test.

## Phase 4: User Story 2, a snapshot says where it came from and how it fits (P1)

**Independent Test**: every snapshot component names its lock file, and is reachable or `build-toolchain`. hs-opentelemetry passes I7.

- [ ] T008 [US2] `build_snapshot_placeholder` sets `source_path` from `StackSnapshot.source`. In Phase F's loop (`haskell.rs`, `for snapshot in &stack_snapshots`), merge a duplicate PURL instead of dropping it: add its source to `extra_source_paths`, sorted and unique.
- [ ] T009 [US2] In `waybill-cli/src/generate/orphan_reason.rs` mechanism E, an unreachable component whose `waybill:source-type` is `hackage-snapshot` is `BuildToolchain`, whatever its tier and source files. Keep the existing three-condition test as is for every other component. Add a unit test: a `source`-tier snapshot with a source file is `build-toolchain`, and a non-snapshot `source`-tier component with a source file is not.
- [ ] T010 [US2] Positive control, `waybill-cli/tests/stack_snapshot_identity.rs`. One temporary `waybill-fixture-*` tree with four `stack.yaml.lock` files in separate package directories:
  - (a) `original: {url: …/lts/18/8.yaml}`;
  - (b) `original: {url: …/nightly/2024/5/7.yaml}` (the URL path and its padding, end to end);
  - (c) `original: {url: https://example.invalid/waybill-fixture-snap.yaml}`;
  - (d) `original: lts-18.8`, with the same sha256 as (a).

  Each has a matching `stack.yaml` and a `package.yaml`. Assert:
  - `stackage-lts-18.8@<sha>` appears once, and its emitted `waybill:source-files` lists both (a) and (d) lock files;
  - `stackage-nightly-2024-05-07@…` is present;
  - `stack-snapshot@…?download_url=…` is present;
  - no `unknown` component;
  - every snapshot is reachable or `build-toolchain`;
  - (d)'s lock packages are emitted with their pinned versions.
- [X] T011 [US2] Remove the two I7 rows for `hpack-hs-opentelemetry` and `hpack-hs-opentelemetry-no-generated-cabal` from `waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`.
- [X] T012 [US2] Mutation-test T004–T010 and record the results in `measurements/mutations.md`. Mutations:
  - the string form rejected;
  - `lts` naming off;
  - `nightly` padding off;
  - URL fallback replaced by `unknown`;
  - the digits-only check off;
  - the merge reverted to drop;
  - the orphan rule reverted;
  - `source_path` left empty.

  Every new test must be failed by at least one mutation. Back up the clean files first; after each mutation, restore with `cp`, check with `cmp`, and rebuild.

## Phase 5: Polish

- [X] T013 Build the branch's release binary, rerun T002 into `measurements/branch/`, and diff against `main/`. In `snapshots.md`, list every difference with its reason. Expected: hs-opentelemetry and lootbox gain lock-pinned versions and snapshot hashes, hs-opentelemetry's `unknown` becomes `stackage-lts-18.8@93a10755…` with `build-toolchain`, parse warnings fall to 0, and haskell-language-server's two snapshots (`stackage-lts-23.28`, `ghc-9.6.5`) gain their `stack.yaml` as source file. Nothing else changes. Unexplained differences must be 0.
- [X] T014 Run the gate:

  ```sh
  ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' --expect '^ +Summary \[ *[0-9.]+s\] [1-9][0-9]* tests? run: [1-9][0-9]* passed( \([0-9]+ slow\))?(, [0-9]+ skipped)?$' prepr -- ./scripts/pre-pr.sh
  ```

  Check for `GATE_RESULT=pass` and the summary line.
- [X] T015 Open the PR (Refs #1678) and enumerate every CI job's result. Dispatch `public-corpus.yml` with `regen_goldens=true`, and read the golden diff against T013's table: `hpack-*` goldens move only in `pkg:hackage`. Commit the goldens.
- [X] T016 File the side finding as an issue: lock-file package components record an empty `source_path` (`haskell.rs:2407`). Add a "Recent Changes" line to CLAUDE.md.

## Added at implementation (after the first corpus run)

- [X] T017 `stack.yaml.lock` pins govern the packages its sibling `stack.yaml` lists (FR-000a, R7), in `waybill-cli/src/scan_fs/package_db/haskell.rs`. Fixture test: `a_root_lock_governs_listed_subdirectory_packages`.
- [X] T018 Orphan reason `edgeless-lock-pin` (FR-000b, R8) in `waybill-cli/src/generate/orphan_reason.rs`, with a unit test, documented in `docs/reference/sbom-format-mapping.md` (C45) and `docs/reference/sbom-invariants.md`.
- [X] T019 `measure.sh` also counts unreachable components with no reason (I7's population), so the measurement checks what the corpus checks. Mutations M9 and M10.

## Dependencies

- T001–T003 come before any source edit.
- T004 → T005 → T006/T007 → T008 → T009 → T010 → T011 → T012.
- T009 touches a different file and can follow T005.
- T013 needs T011. T014 and T015 come last.

## Implementation Strategy

FR-000 (T004) is the largest effect, and comes first. US1 names the snapshot; US2 makes it explained. All ship in one PR.
