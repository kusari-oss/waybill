# Tasks: CMake fetches made through user-defined wrappers

**Input**: `specs/1539-cmake-fetch-wrappers/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/wire.md`, `quickstart.md`, `measurements/`)

**Tests are required.** The spec requires a contract test, and a mutation that fails it, for every shape, binding rule, recursion case and accounting rule (SC-007). Plan.md names unit, contract, parity and mutation tests. Each story's contract tests are written first, and must fail at `main` before the code that passes them.

**Conventions:**
- Synthetic `waybill-fixture-*` names only.
- No new Cargo dependency (FR-013).
- TheRock in CI only. Single public files may be read through the GitHub API.
- Revert mutations with `cp`, never `git checkout`.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Run it alone, with no other cargo process.
- Commits carry `Assisted-by: Claude:claude-opus-5-5`.

## Phase 1: Setup

- [X] T001 Census TheRock's fetch wrappers in CI, never locally: `census/wrappers.py` and a step in `.github/workflows/census-fetches.yml` in `kusari-sandbox/waybill-fixture-therock-sdk` (`15e06bd`, run 44096336322). Recorded in `specs/1539-cmake-fetch-wrappers/measurements/census.md`, with `census-wrappers.json`, `census-wrappers.py` and `census-refs.txt` (SC-001).
- [X] T002 Probe CMake 4.4.5's binding of wrapper calls: `specs/1539-cmake-fetch-wrappers/measurements/cmake-semantics/probe.sh`, with `results.txt` and `cmake-version.txt` (probes W1 to W8; research R3 to R5).

## Phase 2: Foundational (blocks every story)

- [ ] T003 [P] Create `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs`, declared from `cmake.rs` beside `blocks`, `fallback` and `fetches`. It holds the definitions side of data-model.md:
  - **Types:**
    - `WrapperDef`, with `name`, `kind`, `file`, `params`, `parses: Vec<ParseArgs>`, `defaults`, `locals`, `call: BodyCall`, `conditions` and `shapes`;
    - `BodyTarget { Builtin(FetchKind), Wrapper(String) }`;
    - `ForwardingShape { Argn, Argv, Unparsed, ParsedVariable, NameParameter }`;
    - `Definitions = BTreeMap<String, Vec<WrapperDef>>`, keyed by the lowercased name.
  - **`collect_definitions(files: &[(PathBuf rel, Vec<(String, Vec<Arg>)>)]) -> Definitions`.** Walk each file's commands with a local block stack. For each `function()`/`macro()`, capture:
    - its parameters (args after the name);
    - its body commands;
    - every `cmake_parse_arguments` (both forms, research R3);
    - defaults in the two forms of R3 (`if(NOT DEFINED <P>_<KW>)` or `if(NOT <P>_<KW>)` around a literal `set()` of that variable);
    - other literal `set()`s as `locals` (a variable set twice to different values maps to `None`);
    - the `if()` conditions enclosing the body call **inside** the body.
  - **The wrapper rule (R1, R2), to a fixed point:**
    - a body is a candidate when it has exactly one call to a fetch built-in or to a name already known to be a wrapper;
    - it is a wrapper when at least one `ForwardingShape` is present on that call, **and**, for a call to a built-in, the call read on its own through the existing `fetch_entry` has no identity (no PURL, or a PURL containing `${`). A call to another wrapper needs only the shape (R2);
    - each `WrapperDef` records its `(file, ordinal)`: its index among the file's `function()`/`macro()` definitions;
    - iterate until no name is added.
  - **Unit tests, `#[cfg(test)]` with `allow(clippy::unwrap_used)`:**
    - each of the five shapes detected (W-1, W-2, W-3 and W-21 bodies), including `ParsedVariable` after a different keyword (`URL ${W_SOURCE_URL}`);
    - a body calling another wrapper is a wrapper without the own-identity check (LLVM's `builtin_default_target` shape);
    - W-10c: a same-named wrapper and non-wrapper get distinct `(file, ordinal)`;
    - not a wrapper: a literal-only body; spec 1538's C-13b body (literal GitHub repository); a body with two fetch calls;
    - a wrapper of a wrapper found only on the second iteration (W-8);
    - case-insensitive names (W-22);
    - two definitions of one name both kept, in walk order.
- [ ] T004 Implement the argument binder in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs`: `bind_args(call: &[Arg], def: &WrapperDef) -> Option<BTreeMap<String, Value>>`, where a value is a scalar or a list. It covers data-model "Binding" steps 1 to 4:
  - **Positional binding:** named parameters take `ARGV0…`; `ARGN` is the rest; `ARGV` is everything; `ARGV<n>` (W7). Too few arguments gives `None`.
  - **`cmake_parse_arguments`**, in body order:
    - `PARSE_ARGV <n>` reads `ARGV[n..]`, and the other form reads its expanded trailing arguments;
    - options;
    - a one-value keyword: **the last occurrence wins** (W4);
    - a multi-value keyword: **consumes up to the next keyword of the same call**, including an unparsed `URL` (W1);
    - the remainder, in order, is `<P>_UNPARSED_ARGUMENTS`.
  - **Defaults** apply only when the variable is unset (W8).
  - **`substitute(args, bindings, locals) -> Vec<Arg>`:**
    - a list value expands to one `Arg` per element, quoted or not (W6);
    - an unknown `${…}` stays as written.
  - **Unit tests, one per probe:** W1 (multi-value swallows `URL`, so `unparsed` is empty), W1b, W4, W6, W7, W8, plus `ARGV1`. Each test's comment names its probe line in `results.txt`.
- [ ] T005 [P] Add the FR-005 helper beside `parse_version_from_url` in `waybill-cli/src/scan_fs/package_db/cmake.rs`: `archive_name(url: &str, version: &str) -> Option<String>` (research R6).
  - **The rule:** take the last path segment, without query; strip one archive extension from `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`, `.tar.xz`, `.txz`, `.tar.zst`, `.zip`, `.tar` and `.7z`; match `^(.+?)[-_]v?<version>$`, where `<version>` is the given version, escaped.
  - **Unit tests over every filename shape in `measurements/census.md`:**
    - `zstd-1.5.7.tar.gz` → `zstd`;
    - `grpc-v1.78.1.tar.gz` → `grpc`;
    - `libdrm-libdrm-2.4.134.tar.bz2` → `libdrm-libdrm`;
    - `fmt-11.1.3.zip` → `fmt`;
    - `Catch2-3.8.1.tar.gz` → `Catch2`, case kept;
    - `OpenBLAS-18638c7.tar.gz` with version `unknown` → `None`;
    - `libbacktrace-<40 hex>.zip` → `None`;
    - `sqlite-amalgamation-3510300.zip` → `None`;
    - plus `v2.1.tar.gz` → `None` and a URL with a query string.
- [ ] T006 Extend `FetchCoverage` in `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs` with `wrapper_definitions`, `wrapper_calls` and `wrapper_ambiguous_calls` (research R9).
  - **Wire format:** `wire()` adds `"wrappers":{"ambiguous-calls":N,"calls":N,"definitions":N}` **only when `wrapper_definitions > 0`**, so trees without wrappers keep `main`'s C231 byte for byte (I-7).
  - **Unit test:** both wire shapes, with sorted keys.
- [ ] T007 Register C234 `waybill:cmake-fetch-wrapper` in `docs/reference/sbom-format-mapping.md`:
  - **The row:** the value shape (sorted array of chains, outermost first), presence (iff a wrapper call is among the component's declarations), and R10's Principle V audit.
  - **Same file:** extend C231's row with the `wrappers` keys, their meanings, and "outside V1".
  - **Extractors, in the same commit:** the catalog gate is in the lib target. Add `cdx_anno!(c234_cdx, …, component)`, `spdx23_anno!(c234_spdx23, …)` and `spdx3_anno!(c234_spdx3, …)` to `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`, registered in `parity/extractors/mod.rs` as `SymmetricEqual`, `order_sensitive: false`.

**Checkpoint:** the gate's lib target passes. Nothing binds calls or emits C234 yet.

## Phase 3: User Story 1 — a dependency fetched through a forwarding wrapper is a component (P1) 🎯 MVP, lands with US2

**Goal:** every call to a recognised wrapper that binds to a `URL` or `GIT_REPOSITORY` is emitted, exactly as a direct call with the bound arguments would be, apart from FR-005's name.
**Independent test:** contract cases W-1 to W-5, W-8, W-12 to W-18b, W-21 and W-22, plus I-1, I-2 and I-7.

- [ ] T008 [US1] Create `waybill-cli/tests/cmake_fetch_wrappers.rs`.
  - **Helpers:** `mod common;` with `common::git_fixture::GitFixture`, as `tests/cmake_nested_fetches.rs` does. Its `scan`, per-format component extractors and `coverage(docs)` (which asserts V1) are local to that file, so move them into a new `waybill-cli/tests/common/cmake_fetch.rs` and use them from both files, rather than copying.
  - **Fixtures:** write `cmake/wrap.cmake` per case.
  - **Cases:** W-1, W-2, W-3, W-4, W-5, W-8, W-12, W-13, W-14, W-15, W-16, W-17, W-18, W-18b, W-21 and W-22, as written in `contracts/wire.md`, checking PURL, hash, mechanism, scope, C122, C232, C18 (the call site) and C231 in all three formats.
  - **Not yet:** C234 assertions belong to US3 (T016). Leave a `// C234: T016` marker at each.
  - **I-7:** a tree with a direct fetch and no wrapper definitions yields the same C231 string as before this feature, with no `wrappers` key.
  - **Prove they bite:** run against `main`'s reader. Record in this task's text which cases fail (expected: all but W-5's "no component") and which pass as controls.
- [ ] T009 [US1] Implement `bind(call, defs, name, depth, chain) -> Binding` in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs` (data-model steps 5 and 6).
  - **Recursion:** for `BodyTarget::Wrapper(w)`, recurse with the substituted arguments. A name already on `chain` gives `Unresolved(Recursion)`, and depth > 8 gives `DepthExceeded`.
  - **Built-in target:** `BodyTarget::Builtin(kind)` gives `Bound { kind, args, conditions, chain }`, with conditions the call site's then each body's, outermost first (R8).
  - **Several definitions (R5):** bind against every definition of the name. If every result is `Bound` with the same PURL and hash (computed through `fetch_entry`), use the first in walk order; otherwise `Unresolved(AmbiguousDefinitions)`. A definition of that name that is not a wrapper counts as disagreeing (W-10c).
  - **Unit tests:** a two-level chain; recursion `a → b → a`; a depth of 9; agreement and disagreement across two definitions.
- [ ] T010 [US1] Wire binding into `read_fetches` in `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs` (research R7).
  - **Pre-pass:** build `Definitions` with `collect_definitions` from the already-parsed files, and set `wrapper_definitions` to the number of `WrapperDef`s.
  - **Template bodies:** in the main pass, count `function()`/`macro()` openings per file. When the block stack is inside the body whose `(file, ordinal)` is a `WrapperDef`'s (a same-named definition that is not a wrapper is read as spec 1538 reads it), its built-in fetch call and any wrapper call are part of the template. Skip them: no `read`, no `unresolved.identity`.
  - **Call sites:** a command whose lowercased name is in `Definitions` is a call site, outside a wrapper body.
    - Increment `read` and `wrapper_calls`.
    - Expand the call's arguments with the file's literal `set()`s, as direct calls are.
    - Bind it.
    - A `Bound` result goes through the existing path: `fetch_entry(bound_name, bound_args.join(" "), call_site_path, rule, vars)`, the parameterised-name rule, `fallback::classify` with the bound arguments, the merged conditions and the **call site's** `blocks.function()`, test scope from the call site's directory, `mark()`, and a `Declaration` keyed on the bound name.
    - `Unresolved(_)` increments `unresolved_identity`, and also `wrapper_ambiguous_calls` for `AmbiguousDefinitions`.
  - **Chain for C234:** carry the chain on the `Declaration` (a new `chain: Vec<String>` field; empty for direct calls), unused until T017.
- [ ] T011 [US1] Apply FR-005 in `fetches.rs`. For a bound declaration whose entry came from a `URL` (mechanism `cmake-externalproject` or `cmake-fetchcontent-url`, with no `GIT_REPOSITORY`) and whose version is not `unknown`: if `archive_name(url, version)` is `Some(n)`, rebuild the PURL as `pkg:generic/<n>@<version>` with `encode_purl_segment`, and set `entry.name = n`. Direct declarations are never renamed (FR-011).
- [ ] T012 [US1] Update spec 1538's C-13 expectation in `waybill-cli/tests/cmake_nested_fetches.rs`. The parameterised declaration inside `function(wf_fetch)`, never called, is now a template: assert `wrappers.definitions` = 1, and `unresolved.identity` reduced by 1 (research R9; `contracts/wire.md` note). Leave C-13's literal declaration and C-13b's assertions unchanged.
- [ ] T013 [US1] **The FR-011 control.** The existing CMake regression goldens (`cdx_regression_cmake`, plus the SPDX 2.3 and SPDX 3 `cmake` cases in `tests/{cdx,spdx,spdx3}_regression.rs`) and all of `tests/cmake_nested_fetches.rs` except C-13 pass unchanged, with no golden regenerated. If a golden changes, stop and trace it to a wrapper definition in that fixture before going on.

**Checkpoint:** the US1 cases, C-1 to C-15 and the goldens all pass. Without US2's cases written, in-tree wrappers are already unresolved by construction, but unverified, which is why US1 and US2 land together.

## Phase 4: User Story 2 — a wrapper around an in-tree build adds nothing false (P1)

**Goal:** wrapper calls with no identity, ambiguous definitions, recursion, multi-value swallowing, excluded definitions and spec 1538's literal-identity bodies emit nothing false, and are counted.
**Independent test:** contract cases W-6, W-7, W-9, W-10, W-10b, W-10c, W-11, W-19, W-20, W-23 and W-24, plus I-2 and I-6.

- [ ] T014 [US2] Add cases W-6, W-7, W-9, W-10, W-10b, W-10c, W-11, W-19, W-20, W-23 (R9 merge through a wrapper) and W-24 (ownership by a submodule's call site) to `waybill-cli/tests/cmake_fetch_wrappers.rs`, with an `assert_i6(docs)` helper (`ambiguous-calls ≤ unresolved.identity`, `calls ≤ read`) called from `coverage()`.
  - **The two cases that would catch a wrong reader:**
    - W-11: a component here means the binder read `URL` by keyword search instead of CMake's multi-value rule;
    - W-19: a missing component means R2 was skipped.
  - **Run against the US1 implementation** and record any that fail. Each failure is fixed in T015, not by changing the case.
- [ ] T015 [US2] Fix whatever T014 found, in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs` or `fetches.rs`. Record each fix and its cause in this task's text; "none" is a valid result, stated as such.

**Checkpoint:** all W cases except C234's assertions pass, with I-1, I-2, I-6 and I-7. This is the MVP: US1 + US2.

## Phase 5: User Story 3 — the SBOM says how a wrapped fetch was found (P2)

**Goal:** C234 on every component a wrapper call stands for, and on no other.
**Independent test:** I-5 over every W case. W-8's chain is `[["wf_outer","wf_argn"]]`.

- [ ] T016 [US3] Replace the `// C234: T016` markers in `waybill-cli/tests/cmake_fetch_wrappers.rs` with assertions, in all three formats, and add `assert_i5(docs)` to `coverage()`:
  - W-1's direct component has no C234;
  - W-8's chain is outermost first;
  - W-16's survivor has none;
  - fftw's shape: two calls of one URL through one wrapper give one component with one chain, `[["wf_argn"]]`, not two.

  These must fail before T017.
- [ ] T017 [US3] Emit C234 in `fetches.rs`.
  - **On the entry:** a bound declaration sets `entry.extra_annotations["waybill:cmake-fetch-wrapper"] = [chain]`, with names as defined.
  - **`one_per_purl`:** unions the chains of the declarations it combines, sorted and deduplicated.
  - **`first_declaration_wins` and `merge_into_existing`:** do **not** carry the loser's chain (research R10).
  - **Constant:** add `ANN_CMAKE_FETCH_WRAPPER` beside the C231 to C233 constants.

**Checkpoint:** every contract case passes, with I-1 to I-7.

## Phase 6: User Story 4 — TheRock's third-party sources are measured in CI (P2)

**Goal:** SC-002 to SC-005 on the tree that motivated the feature.
**Independent test:** `census-fetches` and `measure-source` at the branch.

- [ ] T018 [US4] Push the branch, and dispatch `census-fetches` in the fixture repository with `--ref main -f waybill_ref=1539-cmake-fetch-wrappers -f base_ref=main`. Record in `specs/1539-cmake-fetch-wrappers/measurements/therock.md`:
  - **C231 at both refs,** against `census.md`'s prediction: `read` 241, `unresolved.identity` 54, `wrappers` `{"ambiguous-calls":0,"calls":53,"definitions":7}`. Explain every difference; the prediction is inference, and a mismatch is a finding, not a failure to hide.
  - **SC-002:** every `therock_subproject_fetch` call with a URL is emitted, with its version and SHA-256. List the 39 components: PURL, version, hash prefix, call site, scope. 36 should be named from the archive, and 3 at their bound name and `unknown`.
  - **SC-003:** LLVM's 13 calls emit nothing, and no component on the tree is named after a wrapper parameter or a target variable. Grep the PURLs for `${`, `therock-` and `-sources`; the last two should match only the 3 FR-005 fallbacks.
  - **SC-004:** V1 and V2 hold, and `wrappers.calls` = 53 matches the census.
  - **SC-008 and I-1:** C234 on all 39 and on no other component in all three formats, and the root's direct targets agree in the three formats.
  - **Scan times** of the sparse tree, interleaved, from `times.txt`.
- [ ] T019 [US4] Dispatch `measure-source` with the branch against `main` (interleaved: a warm-up, then three alternating rounds). Record the medians and their ratio in `measurements/therock.md` (SC-005, ≤ 1.5×). One run is one measurement of 3 rounds per side. If the ratio is over 1.1, run it twice more before drawing a conclusion.
- [ ] T020 [US4] **Ask the user** whether to file the FR-010 follow-up: 8 `download_project` calls with an identity no wrapper rule can bind (research R11; `measurements/census.md`), proposing `download_project` as an `ExternalProject_Add` alias with `PROJ` as the name. If yes, file it in `kusari-oss/waybill`, linked to #1518 and #1539, with the attribution line, and record its number here.

## Phase 7: Polish and cross-cutting

- [ ] T021 Mutations: `specs/1539-cmake-fetch-wrappers/measurements/mutate.py` plus `mutate-run.sh`, in spec 1538's pattern (`specs/1538-cmake-nested-fetches/measurements/`), restoring with `cp` and checking `git diff --quiet` after a commit. Record the results in `measurements/mutations.md`. At least these mutations, each of which must be caught by a named test:
  - each `ForwardingShape` no longer detected (×5);
  - R2's own-identity check removed;
  - multi-value consumption stops at any keyword (W1 lost);
  - one-value keyword first-wins instead of last-wins (W4 lost);
  - defaults applied unconditionally;
  - the binder takes the caller's `URL` before the body's (W2 order lost);
  - the ambiguity rule replaced by first definition wins;
  - the recursion guard removed (bounded by a test timeout);
  - the depth cap raised to unbounded;
  - FR-005 ignores the version tie, renaming `libbacktrace-<sha>`;
  - FR-005 applied to direct calls;
  - template bodies counted in `read`;
  - calls inside a wrapper body treated as call sites;
  - C234's union in `one_per_purl` dropped;
  - command-name lookup made case-sensitive;
  - the call site's `function()` replaced by the wrapper's own for `cmake-function-body`.
- [ ] T022 Corpora in CI.
  - **Public corpus:** dispatch `public-corpus.yml` in compare mode at the branch. Expected: identical, or every changed target traced to a CMake wrapper definition or call in its tree (SC-006). A C231-only change from a template moving out of `read` is expected where a target defines a parameterised wrapper (research R13). Regenerate goldens only after tracing, with `-f regen_goldens=true`.
  - **Quality corpus:** compare against `main`, PURL by PURL. pytorch's C-13b components (dlfcn-win32, xz) must be unchanged (R2).
  - **Record** in `measurements/corpus.md`, labelling any attribution not traced as inference.
- [ ] T023 [P] Docs:
  - **`docs/ecosystems.md`' CMake section:** what a wrapper is (the five shapes, R2), how a call binds (the measured rules, with W1 and W2 called out), multiply-defined wrappers, FR-005 naming and its fallback, C234, C231's `wrappers`, and the `download_project` boundary.
  - **`CHANGELOG.md` `[Unreleased]`:** Added C234 and C231 `wrappers`; Fixed #1518, with TheRock's numbers from T018.
- [ ] T024 Run the gate **alone** and record it in `specs/1539-cmake-fetch-wrappers/measurements/gate-log.md`:
  - the ok-line count, 0 failed, and the expected line found;
  - `cmake_fetch_wrappers` N of N;
  - `cmake_nested_fetches` N of N;
  - `git diff --quiet main -- Cargo.toml Cargo.lock '*/Cargo.toml'` (FR-013).
- [ ] T025 Open the PR against `main`.
  - **Title:** `feat(cmake): fetches made through user-defined wrappers (#1518)`.
  - **Body:** the census and probe findings, TheRock's before and after from T018/T019, corpus results, the mutation count, the spec changes (FR-008 note, C-13), and `Refs #1539` (not "fixes"). End it with the attribution line. No session links.
  - **After opening:** tick #1518 on tracker #1542 only after the user lands it.

## Dependencies

- **Setup** (T001, T002) is done.
- **Foundational:** T003 and T005 are independent. T004 follows T003, in the same file. T006 and T007 can run beside them. All of them block the stories.
- **US1:** T008 (tests first) → T009 → T010 → T011 → T012 → T013.
- **US2:** T014 needs T010. T015 follows T014. US1 and US2 land together, in one PR.
- **US3:** T016 needs T010 and T007; T017 follows T016.
- **US4:** T018 and T019 need US1 to US3 pushed. T020 needs only the census (done), and can be asked at any time.
- **Polish:** T021 needs every story. T022 needs the branch pushed. T023 can run any time after T018. T024 comes after everything else, and T025 after T024.

## Parallel examples

- **Foundational:** T003 (definitions) and T005 (archive name) run in parallel. T006 and T007 can run beside them. T004 (binder) follows T003 in `wrappers.rs`.
- **CI:** T018, T019 and T022 are independent dispatches, and run concurrently in CI. Run nothing that compiles locally at the same time as the gate.
- **US1 vs US2:** T014's cases can be written while T009 to T011 are in progress, because they only need the helpers from T008.

## Implementation strategy

1. **MVP = US1 + US2,** in one PR: TheRock's 39 tarballs appear, LLVM's in-tree wrappers stay counted and unresolved, and the 1538 contract and goldens hold.
2. **US3** adds the evidence row in the same PR, because C234 is the auditable trail for an inferred binding (spec US3's rationale), and shipping components without it would make them indistinguishable from direct declarations.
3. **US4 and Polish** close the measurement loop before the PR is opened: the prediction in `census.md` is checked, the cost measured, and the corpora traced.
