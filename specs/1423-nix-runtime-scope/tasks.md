---
description: "Task list for 1423: Nix closure scope from the runtime closure"
---

# Tasks: Nix closure scope from the runtime closure

**Input**: Design documents from `/specs/1423-nix-runtime-scope/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Requested. FR-016 calls for tests in both the normal lanes and the corpus, and the project convention is tests first, run red, with one mutation check per story (revert with `cp`, never `git checkout`).

Paths are relative to the repository root. `closure/` means `waybill-cli/src/scan_fs/package_db/nix/closure/`.

## Phase 1: Setup

- [ ] T001 Re-run `specs/1423-nix-runtime-scope/measurements/nix-behaviour/q2-query.sh` against the current host's nix and confirm the plain `nix path-info -r --store <url> <installable>` output format (one path per line) still holds. Record the nix version in `measurements/nix-behaviour/README.md` if it differs.
- [ ] T002 [P] Write `specs/1423-nix-runtime-scope/measurements/nix-behaviour/q8-verify-cost.sh`. It times `nix store verify --no-contents -r --store https://cache.nixos.org <installable>` for shellcheck, curl and haskellPackages.hlint at nixpkgs `a799d3e3`, cold and warm, 5 runs each (min/median/max), and writes the results to `q8-out/` and the README (research R11). If hlint verification costs more than about 0.2 s per path, stop and report back before T014.

## Phase 2: Foundational (blocks all stories)

- [ ] T003 Add `ScopeEvidence { RuntimeClosure, StaticallyLinked, ReachabilityWalk, ClosureRole }` with an exhaustive `as_str()` (wire values per `contracts/c203-values.md`) and a `scope_evidence: Option<ScopeEvidence>` field on `ResolvedComponent` in `waybill-common/src/resolution.rs`. Default it to `None` at every construction site that `cargo build --workspace --all-targets` reports.
- [ ] T004 Extend `classification_source` in `waybill-cli/src/resolve/closure_role_scope.rs` to return the `ScopeEvidence` wire value when one is set. Exempt `ArtifactPresence` still wins. Unit-test each variant.
- [ ] T005 [P] Add a `"path-info"*` branch and a `"store verify"*` branch to the shared stub in `waybill-cli/tests/common/nix_stub.rs`. They are driven by new optional `NixStub` fields `runtime: Option<String>` (a listing, one store path per line), `substituters: Vec<String>` (also printed by `config show substituters`) and `verify_ok: bool`. When `runtime` is `None`, path-info exits 1 with `path '…' is not valid`. Every argv still goes to `nix.log`.
- [ ] T006 Add an explicit `"path-info"*) echo "path '…' is not valid" >&2; exit 1 ;;` branch, with a comment that the walk is intended, to each private stub: `waybill-cli/tests/{closure_binary_contents,closure_role_scope,closure_edges,closure_members,closure_dep_fields,closure_structured_attrs,closure_component_cpes,closure_ecosystem_identity,nix_closure_hackage_identity,nix_config_closure,nix_degradation_detail,nix_eval_tier,nix_ifd_reason}.rs`. Run each file and list every assertion that changes. Fix each one by reasoning about what the walk should produce, never by regenerating a golden blindly.
- [ ] T007 [P] Add the C216 `waybill:closure-scope-method` row (KEEP-NO-NATIVE, with the justification from `contracts/c216-closure-scope-method.md`), and the three new C203 enum values to row C203, in `docs/reference/sbom-format-mapping.md`.
- [ ] T008 Add C216 extractors following the C204 pattern: `cdx_anno!` in `waybill-cli/src/parity/extractors/cdx.rs`, `spdx23_anno!` in `spdx2.rs`, `spdx3_anno!` in `spdx3.rs`, plus the imports and a sorted table row in `waybill-cli/src/parity/extractors/mod.rs`. Confirm `every_catalog_row_has_an_extractor` and `extractors_table_is_sorted_by_row_id` pass.
- [ ] T009 Add a `scope_method: Option<ScopeMethodRecord>` field and a `closure_scope_method_wire()` (sorted-key JSON, `schema: 1`) to `closure/summary.rs::NixClosureSummary`. Emit it beside C204 in `waybill-cli/src/generate/cyclonedx/metadata.rs`, `generate/spdx/annotations.rs` and `generate/spdx/v3_annotations.rs`.

**Checkpoint:** the workspace builds, all existing tests pass with the stub changes from T006 reviewed, and C216 is emitted as absent.

## Phase 3: User Story 1 — scope from the runtime closure (P1) 🎯 MVP

**Goal:** with the output in a store, closure members are marked exactly by the runtime closure plus static linking (FR-001–FR-004, FR-006–FR-009).
**Independent test:** `cargo test -p waybill --test closure_runtime_scope us1_`. With the stub returning curl's recorded listing (`measurements/rt/curl.paths`), python3/libxml2/pcre2 are marked and curl/openssl/nghttp2/libssh2/zlib/brotli are not, in all three formats.

### Tests (write first, run red)

- [ ] T010 [P] [US1] Create `waybill-cli/tests/closure_runtime_scope.rs` with a closure fixture under `waybill-cli/tests/fixtures/nix_closure_runtime/` (synthetic `waybill-fixture-*` names) and these tests:
  - `us1_runtime_members_unmarked_and_others_marked` (all three formats);
  - `us1_build_tooling_member_inside_runtime_closure_is_unmarked` (the dns-root-data shape, both directions of FR-001);
  - `us1_local_store_answer_records_store_local`;
  - `us1_substituter_answer_is_verified_and_recorded` (C216 `store` is the URL and `verified` is true);
  - `us1_unverified_substituter_answer_is_not_used` (falls to the walk with `verification-failed`);
  - `us1_remote_queries_carry_connect_timeout_and_one_attempt` (asserts the `--option connect-timeout` / `download-attempts 1` argv in `nix.log`);
  - `us1_exempt_members_keep_exemptions`;
  - `us1_every_closure_member_has_c203`.
- [ ] T011 [P] [US1] Add these to `closure_runtime_scope.rs`, with a fixture of a static Haskell executable whose library deps are in the static build's `buildInputs` and propagate through `propagatedBuildInputs` (the shellcheck shape, `measurements/nix-behaviour/q6-*`):
  - `us1_static_haskell_libraries_are_runtime_and_statically_linked`;
  - `us1_test_only_haskell_dep_marked_with_declared_edges` (an ormolu-shaped fixture, run with m1275 package-set edges present);
  - `us1_test_only_haskell_dep_kept_runtime_without_declared_edges`, asserting C216 `static-link-source: derivation-fields`.
- [ ] T012 [P] [US1] Add `us1_embedded_go_module_inherits_runtime_host` to `closure_runtime_scope.rs`. Reuse the `NIX_STORE_DIR` synthetic-binary approach from `waybill-cli/tests/closure_binary_contents.rs`: a module in a runtime host is unmarked with `statically-linked`, and a module in a non-runtime host is marked.

### Implementation

- [ ] T013 [US1] Create `closure/runtime.rs` with:
  - `RuntimeStore`, `RuntimeClosure` and `RuntimeUnavailable` (data-model.md, wire strings exhaustive);
  - argv builders: local `path-info -r <installable>`; remote `path-info -r --store <url> --option connect-timeout 2 --option download-attempts 1 <installable>`; `store verify --no-contents -r --store <url> <installable>`;
  - `fn query(cfg: &ClosureConfig, installable: &str, substituters: &[String]) -> Result<RuntimeClosure, RuntimeUnavailable>`: local first, then each substituter; a miss moves to the next; a hit is accepted only after verification succeeds.

  Every invocation goes through `eval::invoke::run_bounded` and `guard()`. Parse the plain one-path-per-line output into store basenames. Unit-test the parser and argv builders.
- [ ] T014 [US1] Read `substituters` from the `nix config show` output the tier already captures (`closure/mod.rs`). Add the `config show substituters` invocation only if that output doesn't include it, and say which in a comment citing `measurements/nix-behaviour/q1-*`. Record the installable (`<flakeref>#<attr>`) on `ClassifiedClosure` so `runtime::query` can use it.
- [ ] T015 [US1] Create `closure/static_link.rs` with `StaticLinkSource` and `fn static_link_set(raw: &RawClosure, runtime_drvs: &BTreeSet<DrvKey>, declared: Option<&DeclaredEdges>) -> StaticLinkSet`:
  - with declared edges (the m1275 `nix-closure-declared` relationships, mapped back to drv keys), follow them from each runtime Haskell-builder member;
  - otherwise take that member's `buildInputs + propagatedBuildInputs`, then `propagatedBuildInputs` transitively through Haskell derivations.

  Identify Haskell-builder derivations the way the closure tier already does (grep `classify.rs` / `emit.rs` for the generic-builder test). Unit-test both sources on fixtures.
- [ ] T016 [US1] Join runtime paths to members in `runtime.rs`: `store_basename` → `RawClosure::output_index()` → drv key, collecting the runtime drv-key set and the unmatched-path count. Use the evaluated drv's own output paths, never `nix path-info`'s `deriver` field, which differs (`measurements/nix-behaviour/q2-deriver.sh`).
- [ ] T017 [US1] Build `RuntimeEvidence` in `waybill-cli/src/cli/scan_cmd.rs` before `assign_role_scope` (around line 5111). Inputs: `ClassifiedClosure`, the runtime query, the static-link set, and the `relationships` filtered to `embedded-build-info` (host → module) and `nix-closure-declared`. Pass it to the pass, and hand the resulting `ScopeMethodRecord` to `NixClosureSummary`.
- [ ] T018 [US1] Implement the decision function from data-model.md in `waybill-cli/src/resolve/closure_role_scope.rs::assign_role_scope`: rules 1–3 and 5, with FR-001 in both directions (clear a prior `NotInArtifact`). Map components to drv keys with the same `MemberKey` join `contents.rs` uses (`contents.rs:298-324`); extract a shared helper rather than duplicating it.
- [ ] T019 [US1] Stop `closure/contents.rs` from deciding module presence (around line 557: `artifact_presence: (!ships).then_some(...)`). New modules start with `None`, and the pass decides. Keep the `ships` computation only if something else reads it; otherwise remove it.
- [ ] T020 [US1] Extend invariant I10 in `waybill-cli/tests/corpus_harness_195/invariants/checks/closure_role_scope.rs` per `contracts/c203-values.md` (population, passing rule, converse check), with unit tests that include a pre-1423 document shape.
- [ ] T021 [US1] Run T010–T012 green. Mutation-check `us1_build_tooling_member_inside_runtime_closure_is_unmarked`: save `closure_role_scope.rs` with `cp`, remove the clearing of a prior `NotInArtifact`, confirm the test fails, then restore with `cp` and confirm with `cmp`.
- [ ] T022 [US1] Run T002's verification-cost measurement and record it. Check SC-005 against the measured per-path cost and the unreachable-substituter bound (T010's timeout test covers the argv; record the real 2.1 s figure from `q5-*`).

**Checkpoint:** US1 tests green; C216 reports `runtime-closure`; every closure member carries C203.

## Phase 4: User Story 2 — reachability-walk fallback (P2)

**Goal:** when no store answers, scope comes from a walk from the evaluated package over runtime-linking edges (FR-005, FR-007).
**Independent test:** `cargo test -p waybill --test closure_runtime_scope us2_`. With the stub returning no listing, members reached from the root are unmarked with C203 `reachability-walk`, and C216 records `not-in-any-store`.

- [ ] T023 [P] [US2] Add these to `closure_runtime_scope.rs` (red first):
  - `us2_no_store_answer_uses_walk` (C216 `method: reachability-walk`, `unavailable-reason: not-in-any-store`);
  - `us2_walk_marks_unreached_members`;
  - `us2_walk_follows_only_runtime_linking_fields` (a member reached only through `nativeBuildInputs` is marked);
  - `us2_no_evaluated_package_falls_back_to_closure_role` (two-root fixture; C216 `method: closure-role`).
- [ ] T024 [US2] Implement the walk in `closure/static_link.rs` or a new `closure/walk.rs`. Start from the evaluated drv and follow `ARTIFACT_FIELDS` (`closure/classify.rs:38-44`) through `inputs.drvs`, matching the committed `measurements/reach.py` rule. Add the static-link set computed from the reached Haskell members. Return the reached drv-key set.
- [ ] T025 [US2] Wire `RuntimeEvidence::Walk` into `scan_cmd.rs` (T017) and rule 4 into `assign_role_scope` (T018). C216 carries the `RuntimeUnavailable` reason.
- [ ] T026 [US2] Score the walk on the five targets with the store query disabled. Run waybill against the wrapper flakes with `PATH` pointing at a nix wrapper that fails `path-info`, then compare the unmarked set with `measurements/rt/*.paths` using `measurements/reach.py`'s scoring. Assert finding-level precision ≥ 95.7% and recall ≥ 94.6% (SC-003), and record the result under `measurements/us2-walk-score/`.

**Checkpoint:** US2 tests green; moat (output not in any store) produces walk-derived marks.

## Phase 5: User Story 3 — one answer in all three formats (P2)

**Goal:** `sbom edit` selects not-in-artifact members identically in CDX, SPDX 2.3 and SPDX 3 (FR-009, FR-010).
**Independent test:** `cargo test -p waybill --bins edit::` plus `cargo test -p waybill --test sbom_edit_filter us3_`. On one scan, `--keep 'scope=excluded'` yields the same PURL set in all three formats.

- [ ] T027 [P] [US3] Add these to `waybill-cli/tests/sbom_edit_filter.rs` (red first):
  - `us3_scope_excluded_selects_same_set_in_three_formats` (a closure fixture scanned once, edited three times);
  - `us3_scope_development_no_longer_matches_bare_excluded`;
  - `us3_real_lifecycle_scopes_unchanged` (an npm fixture with dev dependencies; the `scope=development` result is identical to before).
- [ ] T028 [US3] Add `Scope::NotInArtifact`, parsed from `excluded`, to `waybill-cli/src/edit/select.rs`, and update the selector help text and the unknown-scope error.
- [ ] T029 [P] [US3] In `waybill-cli/src/edit/cdx.rs` (around 131-138), match `scope: "excluded"` as `NotInArtifact` and remove the fallback that inserted `Scope::Development`.
- [ ] T030 [P] [US3] In `waybill-cli/src/edit/spdx23.rs`, read `BUILD_TOOL_OF` relationships: their `spdxElementId` gets `NotInArtifact`.
- [ ] T031 [P] [US3] In `waybill-cli/src/edit/spdx3.rs`, read `LifecycleScopedRelationship` elements with `relationshipType: usesTool` and `scope: build`: each `to` gets `NotInArtifact`.
- [ ] T032 [US3] Run T027 green. Mutation-check `us3_scope_excluded_selects_same_set_in_three_formats` by removing the SPDX 2.3 `BUILD_TOOL_OF` read (T030), reverting with `cp`.

## Phase 6: User Story 4 — runtime-only document (P3)

**Goal:** a documented recipe gives scanners a runtime-only document (FR-011, FR-012).
**Independent test:** on the five wrapper flakes, grype on `--drop 'scope=excluded'` output reports 350 findings, a superset of sbomnix-runtime's 243.

- [ ] T033 [P] [US4] Add `us4_drop_scope_excluded_yields_valid_runtime_document` to `closure_runtime_scope.rs`. For each format, check the result against its schema validator (`tests/common/cdx_schema.rs`, the SPDX 2.3 schema, `spdx3-validate` via `tests/common/spdx3_validate.rs` once #1424 lands), check that the subject is the evaluated package and that every relationship resolves. Also check (FR-012) that a VEX statement about a kept component survives the drop, and that one about a dropped component goes.
- [ ] T034 [US4] Document the recipe in `docs/user-guide/sbom-edit.md` (a "Runtime-only view of a Nix closure" section) and add a short pointer from the Nix closure docs (grep `docs/` for `--nix-closure`).
- [ ] T035 [US4] Run `quickstart.md`'s live check on the five wrapper flakes (`$CLAUDE_JOB_DIR/tmp/cmp/targets`, or regenerate them with `specs/1377-nix-subject-identity/measurements/gen-sboms.sh`). Score with `measurements/runtime.py` and record the result under `measurements/us4-scanner-check/`. Assert SC-001 (350 findings, none lost) and SC-002 (all 243 sbomnix-runtime findings present). If a number differs, explain each difference before claiming the criterion.

## Phase 7: Polish & cross-cutting

- [ ] T036 Add the `nix-closure-shellcheck` corpus target (research R10): a `CorpusTarget` in `waybill-cli/tests/corpus_harness_195/manifest.rs`, an expectations function in `expectations.rs` carrying SC-007 (50 `statically-linked` Hackage components, none marked, C216 `runtime-closure` with `verified: true`), and an `#[ignore]` test in `public_corpus.rs`. **Blocked until the maintainer approves creating the public wrapper-flake repository under `github.com/kusari-sandbox/`.** Ask; do not create it.
- [ ] T037 Run the six golden-owning test suites with `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`. Verify the churn with a normalized, sorted diff and confirm it is only C203 values on closure members and C216 on closure documents.
- [ ] T038 Dispatch `public-corpus.yml` with `regen_goldens=true` for the branch. Commit the Linux moat golden: moat takes the walk on CI, and after T036 shellcheck takes the runtime closure. Run I10 on the result.
- [ ] T039 [P] Add CHANGELOG [Unreleased] entries in `CHANGELOG.md`:
  - Changed: closure scope comes from the runtime closure;
  - Added: C216, the C203 values and `scope=excluded`;
  - Changed: bare CDX `scope: excluded` no longer matches `scope=development`, which also affects Go `NotNeeded` components.
- [ ] T040 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Read `>>> all pre-PR checks passed.` and count the `test result: ok` lines. Then verify every task's claim at close-out, not its tick.

## Dependencies

- Setup (T001–T002) → Foundational (T003–T009) → US1.
- US2 depends on US1 (it shares `RuntimeEvidence`, the decision function and C216).
- US3 depends only on Foundational. It can run in parallel with US1 and US2, since it touches `edit/` only.
- US4 depends on US1 and US3, plus #1428 merged (VEX-bearing documents).
- Polish depends on everything else. T036 is also blocked on the maintainer.

## Parallel opportunities

- T002 alongside T001; T005 and T007 alongside T003/T004.
- In US1: T010, T011 and T012 (all in one test file, so write them in sequence but design them together). T013 and T015 are separate new files.
- US3 (T027–T032) in a separate worktree, alongside US1.
- T029, T030 and T031 (three different files).

## Implementation strategy

- **MVP:** Phases 1–3. That is the runtime-closure path with static linking and C203/C216; enough to make the marks true for every scan whose output is in a store.
- **Next:** US3 (format agreement), then US2 (the walk) for moat-like cases.
- **Then:** US4's recipe and the scanner check, then the polish.
- Each phase ends with its checkpoint, and nothing is pushed without the T040 gate.
