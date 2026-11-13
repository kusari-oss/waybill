# Tasks: One identity for a Nix-built Haskell dependency the package set does not carry

**Input**: Design documents from `/specs/1046-haskell-overlay-identity/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R10), data-model.md, contracts/merge-and-identity.md, quickstart.md

**Tests**: Required. The constitution (VII, IX) and the plan's testing section call for a failing test before each behaviour, a control in every test, and no network in any test.

**Organization**: by user story.
- **US1 (P1):** merge.
- **US2 (P1):** Hackage-published identity.
- **US3 (P2):** VCS source.
- **US4 (P3):** out-of-range build.

**Emitters need no change for C221.** Per-component `waybill:*` annotations already flow from `extra_annotations` to all three formats; C221 needs only the catalog row (T002) and the parity extractors (T025).

**Conventions for every task:**
- Fixture names are synthetic (`waybill-fixture-*`).
- Test modules use `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- No `.unwrap()` in production code.
- Never `cargo fmt`, and revert a mutation with `cp`.

## Phase 1: Setup

- [X] T001 Create the fixture directory `waybill-cli/tests/fixtures/closure_haskell_overlay/`. Copy the stub-nix project layout from `waybill-cli/tests/fixtures/closure_role_scope/project`, and add a Haskell manifest (`package.yaml`) declaring `waybill-fixture-overlaid >= 2.0.0.0`, `waybill-fixture-relaxed >=1.0 && <1.5`, `waybill-fixture-namesake` and `waybill-fixture-variants >= 3.0`.
- [X] T002 [P] Add the catalog row **C221 `waybill:requirement-unsatisfied`** to `docs/reference/sbom-format-mapping.md`. Use the shape and the KEEP-NO-NATIVE audit text from research R7, and the format carriers of C172.
- [X] T003 [P] Extend the **C201** row in `docs/reference/sbom-format-mapping.md` with the five values from research R8 (`hackage-published`, `hackage-unpublished`, `hackage-unchecked-offline`, `hackage-unchecked-opted-out`, `hackage-unreachable`). State that they apply only to a GHC-built member whose version the pinned set does not carry, and that `version-disagrees` keeps its meaning for every other member.

## Phase 2: Foundational (blocks US2 and US3)

- [X] T004 Write unit tests in `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs` (tests module) for a new `vcs_source(drv) -> Option<VcsSource { url, rev }>`. Cases:
  - `url`/`rev` under `structuredAttrs` (the `fetchgit` shape measured in research R5);
  - both in `env`;
  - `rev` missing, which gives `None`;
  - `url` missing, which gives `None`;
  - an empty string, which gives `None`.

  Also test `source_url`:
  - it reads `url` from `structuredAttrs`;
  - it reads a structured `urls` list (first entry);
  - its existing `env` behaviour is unchanged.

  Confirm the new tests fail before T005.
- [X] T005 Implement `vcs_source` and change `source_url` to read through `RawDerivation::attr_str` (both carriers), taking the first string of a structured `urls` list, in `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs`. `VcsSource::reference()` returns `ExternalReference { ref_type: "vcs", url: "git+<url>@<rev>" }`. Make T004 pass.
- [X] T006 Add a helper `src_derivation(drv, raw) -> Option<&str>` to `waybill-cli/src/scan_fs/package_db/nix/closure/derivation.rs`. It resolves a derivation's `src` attribute (via `attr_str`) to the key of the closure derivation whose output path it is. Build an output-path → drv-key index once per closure. Unit-test it: a `src` that is an output in the closure, one that is not, and a derivation with no `src`.

**Checkpoint**: VCS facts and `src` resolution are available. US2 and US3 can start.

## Phase 3: User Story 1 — One component per dependency (P1) 🎯 MVP

**Goal**: A versionless design-tier `pkg:hackage/<n>` folds into the build-tier closure member of the same Haskell package.

**Independent test**: scan the T001 fixture with a stub `nix`. `waybill-fixture-overlaid` appears once, with the build's version and the declared range.

- [X] T007 [P] [US1] Write unit tests in a new `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/twin_merge.rs` (tests module) for selection, per data-model "Twin pair":
  - one Haskell candidate merges;
  - a candidate that is neither `pkg:hackage` nor GHC-built never merges (the namesake);
  - two variants with exactly one in range merge onto that one;
  - two variants with none or both in range do not merge, and say why;
  - a design-tier component that already has a version is not a twin.
- [X] T008 [P] [US1] Write unit tests in the same module for the merge's effect (research R6):
  - the survivor keeps its PURL, version, tier and scope;
  - it gains C20 `waybill:requirement-ranges` and the source-file evidence;
  - `waybill:haskell-version-unresolved-reason` and `waybill:unresolved-reason` are dropped;
  - C172 is kept;
  - an edge naming the design PURL is renamed to the survivor's, and a duplicate edge is removed;
  - the design component is gone.
- [X] T009 [US1] Implement `twin_merge::merge(components, relationships, raw) -> TwinMergeSummary` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/twin_merge.rs`. Use `closure_identity::built_by_ghc` for the Haskell evidence, `version_range::admits` for variant selection, and `super::apply_renames` for edges. The summary carries `merged` and `declined_by_reason` counts. Register the module in `haskell_packages/mod.rs`. Make T007 and T008 pass.
- [X] T010 [US1] Call `twin_merge::merge` in `waybill-cli/src/cli/scan_cmd.rs`:
  - **Where:** immediately after `closure_identity::reclassify` (which, once US2 lands, includes the Hackage step), and before `emit::qualify_backport_identities`.
  - **Logging:** one INFO line with the summary.
  - **Ordering comment:** record why this position (research R1). A merge before `reclassify` would merge `pkg:generic` and miss US2. A merge after `graph::relationships` would leave the closure's edges built against a removed PURL.
- [X] T011 [US1] Add a new integration test `waybill-cli/tests/closure_haskell_overlay.rs`. Use `common::nix_stub::NixStub` with a closure holding these derivations, each `nativeBuildInputs` naming a `ghc-9.6.6` store path except the namesake's:
  - `waybill-fixture-overlaid-2.1.0.0`;
  - `waybill-fixture-namesake-1.0` (not GHC-built);
  - two `waybill-fixture-variants` versions.

  Assert:
  - **Merge:** one `waybill-fixture-overlaid` component in all three formats, carrying the declared range; the namesake design and build components both remain.
  - **Graph:** no dangling endpoint (`common::endpoints::dangling`).
  - **Control:** a scan without `--nix-closure` still holds the versionless design-tier component.

**Checkpoint**: US1 is shippable alone. The twins on the corpus collapse. slack-web stays `pkg:generic` until US2.

## Phase 4: User Story 2 — A VCS-built released version is reachable by advisories (P1)

**Goal**: A GHC-built `version-disagrees` member that Hackage publishes gets `pkg:hackage/<n>@<v>`, plus `vcs_url` when its `src` is a VCS fetch. Every candidate records a C201 outcome.

**Independent test**: with an injected Hackage source answering 200 for the overlaid version, the merged component is `pkg:hackage/waybill-fixture-overlaid@2.1.0.0?vcs_url=…`. Answering 404, it stays `pkg:generic` with `hackage-unpublished`.

- [X] T012 [P] [US2] Add `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/hackage.rs`, with a trait `HackageSource { fn published(&self, name, version) -> Published }` where `Published` is `Yes`, `No` or `Unreachable(String)`.
  - **HTTP implementation:** `GET https://hackage.haskell.org/package/<name>-<version>/<name>.cabal` on a dedicated `std::thread` with a `reqwest::blocking` client, using the same per-request timeout the nixpkgs fetch uses (`HttpSource::new`'s caller), as in `fetch.rs::HttpSource`. 200 means `Yes`, 404 means `No`, and anything else means `Unreachable`.
  - **Name as given:** names are passed case-preserved (research R3: Hackage is case-sensitive).
  - **Memo:** a per-scan memo wrapper.
  - **No credentials:** no credential resolution.
- [X] T013 [P] [US2] Write unit tests in `hackage.rs` against a `wiremock` server:
  - 200 → `Yes`;
  - 404 → `No`;
  - 503 → `Unreachable`;
  - timeout → `Unreachable`;
  - the requested path is `/package/<name>-<version>/<name>.cabal`, with the name's case preserved;
  - the memo issues one request for two identical lookups.
- [X] T014 [US2] Extend `HackageIdentity` in `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/closure_identity.rs`:
  - **New variants:** `HackagePublished { vcs: Option<VcsSource> }`, `HackageUnpublished`, `HackageUncheckedOffline`, `HackageUncheckedOptedOut` and `HackageUnreachable`, with their `wire()` values from research R8. Keep the `match` exhaustive.
  - **Tests (`wire()`):** every variant maps to a distinct value, and `version-disagrees` is unchanged.
- [X] T015 [US2] Write unit tests in `closure_identity.rs` for the decision with an injected `HackageSource` and an `--offline` / opt-out setting:
  - a GHC-built `version-disagrees` member at 200 → `HackagePublished`;
  - at 404 → `HackageUnpublished`;
  - unreachable → `HackageUnreachable`;
  - offline → `HackageUncheckedOffline` with **no request made** (the stub counts requests);
  - opted out → `HackageUncheckedOptedOut` with no request made;
  - a non-GHC-built `version-disagrees` member (the `bash` collision shape) → `VersionDisagrees` with no request made;
  - a `confirmed` member makes no request.
  - a GHC-built `not-in-package-set` member makes no request and keeps `pkg:generic` (FR-006a: a name the pinned set does not carry may be private).
- [X] T016 [US2] Implement the decision in `reclassify`, in `closure_identity.rs`.
  - **When:** only for a member that `decide` returns `VersionDisagrees` for and that `built_by_ghc` holds for.
  - **Lookup:** consult the source unless offline or opted out.
  - **On `HackagePublished`:** mint `pkg:hackage/<n>@<v>` through `Purl::new`. Add the `vcs_url` qualifier when the member's derivation's `src` (T006) is a derivation with a `vcs_source` (T005). Push the rename onto `summary.renames`, so edges follow (#980).

  Make T015 pass.
- [X] T017 [US2] Write a unit test in `closure_identity.rs` for the `vcs_url` encoding (research R4). Build the PURL for `url = https://example.invalid/waybill-fixture.git`, `rev = 40 hex`, and assert:
  - it round-trips through `Purl::new`;
  - its qualifier value is encoded per the purl-spec rules (the `+` and `@` handling checked against the purl-spec test-suite vectors);
  - a tarball-built published member has no qualifier.
- [X] T018 [US2] Add the `--no-hackage-lookup` flag to `waybill-cli/src/cli/scan_cmd.rs`, with help text modelled on `--no-nixpkgs-haskell`: "disables this lookup only; `--offline` suppresses it too". Construct the HTTP `HackageSource`, or none under `--offline` or the flag, and pass it to `reclassify`. Add the flag to the test helpers' `ScanArgs` defaults (`no_hackage_lookup: false`).
- [X] T019 [US2] Extend `waybill-cli/tests/closure_haskell_overlay.rs` with an injected Hackage source. The stub harness needs a seam: an env-selected base URL served by `wiremock`, set **on the spawned `Command` only**, never with `std::env::set_var` in the test process. A process-wide variable shared by concurrent tests is the race #1493 fixed. Cases:
  - **200:** the merged component is `pkg:hackage/waybill-fixture-overlaid@2.1.0.0?vcs_url=…`, and C201 is `hackage-published` in all three formats.
  - **404:** `pkg:generic`, with `hackage-unpublished`.
  - **`--offline`:** `hackage-unchecked-offline`, and wiremock received 0 requests.
  - **`--no-hackage-lookup`:** `hackage-unchecked-opted-out`, and 0 requests.
  - **The namesake:** never requested.
- [X] T020 [US2] Add the new C201 values to the parity tests if they enumerate values (`waybill-cli/src/parity/extractors/mod.rs` row C201 is `SymmetricEqual` and needs no change; verify), and to any test that asserts C201's closed set.

**Checkpoint**: US1 and US2 together satisfy SC-001 and SC-002a on the fixture.

## Phase 5: User Story 3 — The VCS source is in the document (P2)

**Goal**: A git-fetched source member, and each package built from it, carry an A10 `vcs` reference.

**Independent test**: a closure with a `fetchgit` source under `structuredAttrs`. The member, and the package whose `src` it is, each have `externalReferences[type=vcs]` `git+<url>@<rev>`. The SPDX 2.3 and SPDX 3 carriers per A10 match.

- [X] T021 [US3] In `waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs::components`, give a source member with a `vcs_source` a `vcs` external reference **in addition to** any `distribution` reference. Give a package component the same `vcs` reference when its derivation's `src` resolves (T006) to such a member. Keep the order `distribution`, then `vcs`, for byte-stable output.
- [X] T022 [US3] Add integration cases to `waybill-cli/tests/closure_haskell_overlay.rs`:
  - a `fetchgit`-shaped source (`url`/`rev` under `structuredAttrs`) consumed as `src` by `waybill-fixture-overlaid` → `vcs` on both, in CDX `externalReferences`, SPDX 2.3 `externalRefs` (`OTHER`/`vcs`) and SPDX 3 `software_sourceInfo`;
  - a `fetchurl`-shaped source (`urls` in `env`) → `distribution` only, unchanged;
  - **control:** a source with `url` but no `rev` → no `vcs`.

## Phase 6: User Story 4 — A build outside the declared range (P3)

**Goal**: The merge also applies when the build's version is outside every declared range, and the survivor records C221.

**Independent test**: `waybill-fixture-relaxed` declared `>=1.0 && <1.5`, built `1.6`. One component, with C221 `{"requirement-ranges":[">=1.0 && <1.5"],"version":"1.6"}`.

- [X] T023 [P] [US4] Write unit tests in `twin_merge.rs`:
  - an out-of-range single candidate merges and gets C221 with sorted keys;
  - an in-range one gets no C221;
  - a range that does not parse (`Admits::Unknown`) gets no C221;
  - several ranges, one admitting, get no C221 (`version_range::contradicts` semantics).
- [X] T024 [US4] Implement C221 (`ANN_REQUIREMENT_UNSATISFIED = "waybill:requirement-unsatisfied"`) in `twin_merge.rs`, set on the survivor when `version_range::contradicts(&ranges, &version)`. Make T023 pass.
- [X] T025 [P] [US4] Add C221 parity extractors: `cdx_anno!`, `spdx23_anno!` and `spdx3_anno!` (`component` scope) in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`, and a `ParityExtractor` row `C221` (`SymmetricEqual`, `order_sensitive: false`) in `waybill-cli/src/parity/extractors/mod.rs`. The catalog row from T002 and the extractor must land together (the lib-target gate).
- [X] T026 [US4] Extend `waybill-cli/tests/closure_haskell_overlay.rs` with `waybill-fixture-relaxed`. Assert one component, C221 in all three formats, and that the value is equal across them.

## Phase 7: Polish & Cross-Cutting

- [X] T027 [P] Document the merge, the Hackage check (its outcomes, `--no-hackage-lookup`, `--offline`) and the `vcs` references for overlays in `docs/reference/nix-evaluation.md`.
- [X] T028 [P] Add CHANGELOG entries under `[Unreleased]` in `CHANGELOG.md`. Do not name any other tool.
- [X] T029 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`). Read the `passed= failed=` totals and every `---- … stdout ----` block.
- [X] T030 Dispatch `public-corpus.yml` with `regen_goldens=true`, download `corpus-goldens-regen`, and review the diff semantically against research R10. Expect:
  - **slacklinker:** `slack-web` is a single `pkg:hackage/slack-web@2.3.0.0?vcs_url=…` component (SC-001, SC-002a), and `tmp-postgres@1.35.0.0` has `hackage-unpublished`;
  - **slack-web:** `hspec-discover` is merged, with C221 (SC-002);
  - **source references:** the count of `source` members with a reference rises above 111 on slacklinker (SC-003; record the number);
  - **edges and non-closure targets:** named-component edges change only where they named a merged twin (SC-004), and non-closure targets are byte-identical (SC-005).
  - **Observed (run 43434793971, after T033).** All four held. Source members with a reference on slacklinker: 111 → 898 of 902, 4 of them `vcs`. Every removed named edge (5 on slack-web, 31 on slacklinker) reappears on the merged survivor. slacklinker gains 9 edges, which are `slack-web`'s declared dependencies carried from its design-tier twin. Only the 5 `nix-closure-*` targets changed. The first regeneration (run 43389225175) found the T033 defect.
- [X] T031 Commit the goldens, run a non-regenerating corpus check, and confirm the merged tree is byte-identical to a fresh regeneration. Observed: run 43438497867 at `ca2eab7a`, 158 passed, 0 failed, all five closure targets included.
- [X] T033 Compute fixed-output paths for the `src` join (research R5 correction): `waybill-cli/src/scan_fs/package_db/nix/closure/fixed_output.rs`, `src_derivation` in `derivation.rs`; fixture fetches carry no `path`; re-run T030.
- [X] T032 Re-run `specs/1046-haskell-overlay-identity/measurements/hackage-probe.sh` and `qualifier-match-probe.sh` and append the output, dated, to the measurements directory, confirming that research R3 and R11 still hold at implementation time.

## Dependencies & Execution Order

- **Setup (T001–T003)**: no dependencies. T002 and T003 are parallel.
- **Foundational (T004–T006)**: after setup. They block US2's `vcs_url` (T016, T017) and US3 (T021).
- **US1 (T007–T011)**: after setup. Independent of Foundational.
- **US2 (T012–T020)**: after Foundational. Its Hackage step runs inside `reclassify`, so T010's call site needs no change when US2 lands.
- **US3 (T021–T022)**: after Foundational. Independent of US1 and US2.
- **US4 (T023–T026)**: after US1 (it extends `twin_merge.rs`).
- **Polish (T027–T032)**: after every story. T030 needs T029 green.

## Parallel Opportunities

- T002 and T003 (docs).
- T007 and T008 (US1 tests, one file but independent cases), with T012 and T013 (the Hackage client).
- US3 (T021–T022) alongside US2, once Foundational is done.
- T025 (parity extractors) alongside T023.

## Implementation Strategy

1. **MVP = US1.** One component per dependency on the corpus. It is shippable on its own, and the twin counts fall.
2. **Then US2 and US3 together.** They share Foundational, and give slack-web its advisory-reachable identity and its source.
3. **Then US4,** the out-of-range record.
4. **One regeneration at the end** (T030), so the corpus churns once.
