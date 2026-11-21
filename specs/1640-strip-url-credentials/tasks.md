# Tasks: Strip credentials from declared URLs (#1647)

**Input**: `specs/1640-strip-url-credentials/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-001, SC-002, SC-005) and the plan's Testing section. Every D-case is written, and must fail on its credential assertion, before the reader change that fixes it (research R5). G-cases and C-ctl are guards that pass throughout.

**Paths**:
- `SZ` = `waybill-cli/src/identifiers/sanitize.rs`
- `PDB` = `waybill-cli/src/scan_fs/package_db/`
- `IT` = `waybill-cli/tests/declared_url_credentials.rs` (new)
- `FX` = `waybill-cli/tests/fixtures/declared_url_credentials/` (new)
- `MS` = `specs/1640-strip-url-credentials/measurements/`
- `CRED` = the two credential strings `wbfxuser` and `wbfxtoken7c41`
- Rows D1–D19, S1–S3, N1–N7 are research R1's. Cases D-*, I-*, G-*, S-1, C-ctl are contracts/wire.md's.

## Phase 1: Setup

- [X] T001 Re-derive research R1's inventory at the branch HEAD. Run its four searches over `waybill-cli/src` and `waybill-common/src`, counting with `wc -l` or writing to files, never `head`. Diff the result against R1's table and write `MS/inventory.md`, recording:
  - the search commands and their hit counts;
  - the final table: rows added, rows removed, and every disposition with its reason;
  - the exact function and line for D3, where CMake turns a `GIT_REPOSITORY` URL into `pkg:github`;
  - every `tracing::` call in a D-row reader that formats a URL variable before the point where R2 sanitises it;
  - that `git grep 'wbfxuser\|wbfxtoken7c41'` outside this spec directory counts 0.

  If T001 adds a row, add a D-case to `contracts/wire.md` and a task in Phase 3 before going on.
- [ ] T002 Create `IT` with these helpers:
  - `scan3(dir, extra_args) -> (cdx, spdx23, spdx3, log)`: runs the binary three times, once per format, with `RUST_LOG=trace` (so FR-005's "any level" is checked, analysis C1), a fake `HOME` (`common::normalize::apply_fake_home_env`) and `WAYBILL_FIXED_TIMESTAMP`. Returns the three documents as strings and the combined stderr.
  - `assert_no_cred(outputs)`: `CRED` occurs 0 times in each of the four strings. On failure, name which one.
  - `assert_field(doc, …)`: reads a field from the CycloneDX document (a component property, a PURL qualifier or an external reference) and returns it.
  - `parity(dir)`, `assert_repeatable(dir)`, unique `bom-ref`s and CycloneDX validation. Copy these from `waybill-cli/tests/also_detected_via.rs`, using `common::cdx_schema::cdx_validation_errors`.
  - `assert_warned(log, manifest)`: at least one line containing `stripped credentials from URL`, and every such line contains `<userinfo redacted>` and the manifest's file name. It is not "exactly one", because a reader may read one declaration twice (analysis U2).
- [ ] T003 C-ctl (FR-006):
  - **Tree.** `FX/ctl/` holds one clean-URL fixture for each D-row that a directory scan reaches: CMake, Bazel, uv.lock, flake.lock, Pants, CocoaPods, Dart, Composer, Elixir, Erlang, requirements, Yocto, Helm.
  - **Golden.** Generate `FX/ctl-golden/{cdx,spdx-2.3,spdx-3}.json` with **`main`'s** binary, before any source change. Use the masking `waybill-cli/tests/cmake_unresolved_github_fetch.rs` uses. The regeneration env var is `WAYBILL_UPDATE_DECLARED_URL_GOLDENS`.
  - **Assertion.** The test asserts byte identity, and that the log contains no `stripped credentials` line.

**Checkpoint**: C-ctl passes at `main`.

## Phase 2: Foundational (the helper)

This phase changes no D-row's output: no reader calls the new function yet. Only S1–S3, for URLs that carry userinfo, change in how the rest of the URL is normalised (research R3).

- [ ] T004 Unit tests in `SZ`'s test module, with `#[cfg_attr(test, allow(clippy::unwrap_used))]` where needed. Each must fail before T005. Every result is compared with the input minus its userinfo span, byte for byte:
  - mixed-case `https` host;
  - default port `:443` kept;
  - dot segments kept;
  - a space kept;
  - `git+https://`;
  - `git://…;protocol=https;branch=main`;
  - `ssh://git@host:22/…` gives `ssh://host:22/…`;
  - query and fragment kept;
  - a token as the username (`https://ghp_x@host/…`);
  - a `%40` inside the password;
  - an `@` in the path after the authority (not userinfo), unchanged.

  The existing cases keep passing.
- [ ] T005 In `SZ`:
  - change `sanitize_userinfo` to research R3's splice: detect with the `url` crate, delete the authority's userinfo span from the original string, re-parse to confirm no username or password remains, and fall back to today's re-serialised form otherwise. Keep its `Cow` contract.
  - Add `pub fn sanitize_declared_url<'a>(raw: &'a str, manifest: &str) -> Cow<'a, str>`. It calls `sanitize_userinfo` and, on `Cow::Owned`, unless the only userinfo was the bare `git` user (no password) of an `ssh` URL (planning amendment A1), emits `tracing::warn!(target: "waybill::identifiers::sanitize", manifest_file = manifest, url_redacted = %redact_userinfo_for_log(raw), "stripped credentials from URL")`.
  - Unit tests: it emits one event containing neither the username nor the password; none for a clean URL; none for `ssh://git@host/…`, though the user is still stripped; one for `ssh://alice@host/…`.
  - Move `PDB/pip/uv_lock.rs`'s artifact-URL warning onto `sanitize_declared_url` (S3), so one event shape covers every site.

  Run `cargo test -p waybill --lib identifiers::sanitize` and the uv.lock unit tests.

**Checkpoint**: the helper is ready, and C-ctl still passes.

## Phase 3: User Story 1: a shared SBOM carries no credential from a declared URL (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-003. **Independent test**: cases D-1 to D-19, each with its control.

- [ ] T006 [US1] Write D-1 to D-19, G-1 to G-5 and S-1 in `IT`, with fixtures under `FX/`, as `contracts/wire.md` specifies. Each D-case:
  - calls `scan3` and `assert_field` (the field is present with the clean URL), then `assert_no_cred`;
  - calls `parity` and `assert_repeatable`.

  D-10 and D-11 use `common::nix_stub`, following an existing closure test (`waybill-cli/tests/closure_dep_fields.rs`). D-18 is a unit test in `waybill-cli/src/scan_fs/binary/entry.rs` if no cargo-auditable binary fixture exists; say so in the commit message.

  **Run at this commit, before any reader change.** Every D-case must fail on `assert_no_cred` and pass `assert_field`. Every G-case must pass. Record each case's result in `MS/control.md`. A D-case that passes here has a fixture that does not reach its field: fix the fixture before going on.
- [ ] T007 [P] [US1] CMake, D1–D3, in `PDB/cmake.rs` and `PDB/cmake/fetches.rs`:
  - sanitise each `GIT_REPOSITORY` and `URL` value with `sanitize_declared_url` where the reader first takes it from the call's arguments, before the PURL, the GitHub recognition located by T001 and C53;
  - sanitise the dataset URL before `dataset_entry` builds its qualifier.

  The manifest argument is the CMake file's path.
- [ ] T008 [P] [US1] Bazel, D4, in `PDB/bazel.rs`: sanitise the `http_archive` URL before C53.
- [ ] T009 [P] [US1] uv.lock, D5 and D6, in `PDB/pip/uv_lock.rs`: sanitise the index URL before `waybill:pypi-source-url`, and each source URL before its `waybill:source-url` write.
- [ ] T010 [P] [US1] Nix, D7, D10 and D11, in `PDB/nix/identity.rs` (or wherever T001 finds `source_url` set), `PDB/nix/closure/emit.rs` (`vcs_source`, `source_url`) and `PDB/nix/haskell_packages/closure_identity.rs` (`location`).
- [ ] T011 [P] [US1] Pants, D8 and D9, in `PDB/pants/lockfile.rs` and `PDB/pants_jvm/lockfile.rs`: sanitise each artifact URL before `waybill:source-url`.
- [ ] T012 [P] [US1] Git-source PURLs, D12–D16, in `PDB/cocoapods.rs`, `PDB/dart.rs` (both `vcs_url` and `repository_url`), `PDB/composer.rs` (both), `PDB/elixir.rs` (both sites) and `PDB/erlang.rs` (both sites). Sanitise before `minimal_qualifier_encode`.
- [ ] T013 [P] [US1] pip, D17, in `PDB/pip/requirements_txt.rs`: sanitise a direct URL before `DirectUrlRef.url` is stored.
- [ ] T014 [P] [US1] cargo-auditable, D18, in `waybill-cli/src/scan_fs/binary/entry.rs`: sanitise the parsed git source before `waybill:cargo-vcs-source-url`. The manifest argument is the binary's path.
- [ ] T015 [P] [US1] Yocto, D19, in `PDB/yocto/recipe.rs`: sanitise each `SRC_URI` entry before `waybill:src-uri`.
- [ ] T016 [P] [US1] Log sites (research N7 and T001's list). G-5 already guards the Helm SBOM. Change `PDB/helm.rs` only if T001 found that a credentialled repository URL can reach `build_helm_purl`'s no-host `warn!`; then sanitise the URL where it is read. Fix any other log site T001 listed the same way. If T001 listed none, record that in the commit message and change nothing.
- [ ] T017 [US1] Run every D-case, G-case and C-ctl until they pass, plus the existing suites of every reader touched (`cargo test -p waybill --test <suite>`, one filter per invocation). An existing assertion that changes is recorded with its reason in the commit message. None may be loosened.

## Phase 4: User Story 2: removing a credential does not change what a component is (P1)

**Goal**: FR-004. **Independent test**: I-1 to I-3.

- [ ] T018 [US2] Write I-1, I-2 and I-3 in `IT`:
  - I-1 uses `common::git_fixture::GitFixture` for the submodule and the conditional-fetch shape of spec 1538's R9 tests;
  - each compares the credentialled tree's component (PURL, C53, `bom-ref`) with the all-clean tree's.

  I-3 fails if T005's splice regresses to re-serialising.
- [ ] T019 [US2] Make I-1 to I-3 pass. If T007 left `cmake/fetches.rs::repository_identity` or `normalise_repository` comparing an unsanitised value, sanitise it there too. Re-run the CMake suites (`cmake_*`, `git_submodule_reader`).

## Phase 5: User Story 3: the operator learns that a credential was found (P2)

**Goal**: FR-005. **Independent test**: the warning assertions on every D-case.

- [ ] T020 [US3] Add `assert_warned(log, <manifest>)` to every D-case. Each case must name its own fixture's manifest (CMakeLists.txt, uv.lock, Podfile.lock and so on), which checks every call site's `manifest` argument. Add a C-ctl assertion that the log has no `stripped credentials` line, and the same for S-1 (planning amendment A1). Fix any site whose manifest argument is wrong.

## Phase 6: Polish and cross-cutting

- [ ] T021 [P] FR-009 docs:
  - the C53 row of `docs/reference/sbom-format-mapping.md`, worded as in `contracts/wire.md`;
  - a paragraph in `docs/reference/reading-a-mikebom-sbom.md` naming the fields (research R1's D-rows by field) and what is not removed (the query string, #1652; scp-form remotes);
  - `CHANGELOG.md` `## [Unreleased]`, Fixed: declared URLs no longer carry userinfo (#1647), naming the readers. Add a second line: an already-sanitised URL (submodules, source identifiers, uv.lock artifacts) now keeps its host case, port and path when stripped.
- [ ] T022 Mutations (SC-005). Write `MS/mutate.py`, modelled on spec 1639's `specs/1639-also-detected-via/measurements/mutate.py`, with one mutation per `sanitize_declared_url` call site. Each mutation replaces the call with its `raw` argument. Also mutate T005's splice back to re-serialising (I-3 must fail).
  - Restore with `cp` and rebuild after the last one.
  - Use a `^error\[E…\]` guard so a compile failure is not counted as a kill.
  - Record every mutation and its killing test in `MS/mutations.md`.
- [ ] T023 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Read `target/gate/prepr.log` for `GATE_RESULT=` and every `test result:` line, and record the counts in `MS/gate-log.md`.
- [ ] T024 Public corpus (SC-004): `public-corpus.yml`, `regen_goldens=true`, on the pushed branch. Enumerate every job's conclusion and confirm the scanned SHA in the log. Download `corpus-goldens-regen` and `cmp` every file against the committed goldens: all 67 identical is expected. Trace any difference to FR-003 or Clarification Q1. Record in `MS/corpus.md`.
- [ ] T025 TheRock and quality corpus (SC-004):
  - TheRock: `measure-source` in `kusari-sandbox/waybill-fixture-therock-sdk`, with `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`. Read the SHAs from `refs.txt`.
  - Compare all six documents, masking per-run fields as spec 1639's `therock.md` did. Count `ssh://git@` URLs on each side, and trace every changed field to a removed userinfo. Component counts must be equal.
  - Quality corpus: branch and `main` runs. Metrics must be identical, or every difference traced.
  - Record in `MS/therock.md` and `MS/corpus.md`.
- [ ] T026 Verify each task's claim against the code and measurements before ticking it. Confirm `git diff --quiet origin/main -- Cargo.lock '*/Cargo.toml' Cargo.toml` (FR-010). Then open the PR with "Closes #1647" and "Refs #1652", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- **Setup:** T001 → T002 → T003. T003 needs `main`'s binary, before any source change.
- **The helper:** T004 → T005. Phase 2 needs Phase 1.
- **US1:** T006 needs T002 and T005. T007–T016 need T006 and touch different files, so they can run in parallel. T017 needs T007–T016.
- **US2:** T018 → T019, after T007.
- **US3:** T020, after T017.
- **Polish:**
  - T021 can run any time after T005;
  - T022 after T020;
  - T023 after T022;
  - T024 and T025 after T023, on the pushed branch;
  - T026 last.

## Parallel examples

After T006 is committed with its control recorded:

```text
T007 CMake        T008 Bazel        T009 uv.lock      T010 Nix
T011 Pants        T012 git-source PURLs              T013 pip
T014 cargo-auditable                T015 Yocto        T016 log sites
```

T024 and T025 dispatch together.

## Implementation strategy

- **MVP:** Phases 1–3. Every declared URL in the inventory is sanitised and proven by a fixture that fails on `main`.
- **Then:**
  - Phase 4 for identity, which mostly falls out of Phase 3 and T005;
  - Phase 5 for the operator signal;
  - Phase 6 for documentation, mutations and the corpus evidence.
