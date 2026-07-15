---
description: "Task list for m197 — bundle of 7 follow-ups to m190 (epoch PURLs) + m191 (reconciler + versionless PURLs)"
---

# Tasks: m190 + m191 Follow-Up Bundle

**Input**: Design documents from `/specs/197-purl-reconciler-followups/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/annotation-shapes.md, quickstart.md

**Tests**: Included — new unit tests per reader edit; new fuzz test binary; new integration fixtures per US.

**Organization**: 7 user stories (5 P1: epoch × 3 ecosystems + versionless-PURL × 6 ecosystems; 3 P2: fuzz test + npm-alias + always-array reconciler). US5 (alias) depends on US6 (always-array) because both touch the reconciler and US5 uses the array-emission pattern US6 establishes. All other user stories are file-disjoint and parallel-capable.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (distinct files, no dependency on incomplete tasks)
- **[Story]**: US1 / US2 / US2b / US3 / US4 / US5 / US6
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **Ecosystem readers**: `mikebom-cli/src/scan_fs/package_db/{dpkg,apk,rpm,composer,dart,cocoapods,scala,haskell,erlang}.rs`
- **Reconciler**: `mikebom-cli/src/resolve/reconciler.rs`
- **npm alias helper**: `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs`
- **Fuzz test**: `mikebom-common/tests/versionless_purl_fuzz.rs` (new file)
- **Test fixtures**: `mikebom-cli/tests/fixtures/{dpkg,apk,rpm,composer,dart,cocoapods,scala,haskell,erlang,npm}/`
- **Feature spec dir**: `specs/197-purl-reconciler-followups/`

---

## Phase 1: Setup

**Purpose**: Verify m197 branch clean + m196 changes intact.

- [ ] T001 Confirm branch `197-purl-reconciler-followups` is checked out. `git status` shows only `specs/197-purl-reconciler-followups/` untracked. Baseline compile: `cargo test --workspace --no-run` — MUST succeed.

**Checkpoint**: Clean starting state.

---

## Phase 2: User Story 1 — dpkg epoch qualifier emission (Priority: P1)

**Goal**: Debian/Ubuntu `.deb` scans emit `pkg:deb/debian/<name>@<version-without-epoch>?epoch=<N>` (not inline). Closes #562.

**Independent Test**: Synthetic `.deb` with `Version: 1:2.0-r0` scans to `pkg:deb/debian/test-pkg@2.0-r0?epoch=1`.

### Implementation for User Story 1

- [ ] T002 [US1] In `mikebom-cli/src/scan_fs/package_db/dpkg.rs`: add `parse_deb_version_with_epoch(raw: &str) -> (Option<i64>, String)` mirroring `ipk_file.rs::parse_opkg_version_with_epoch` (split on first `:`, parse prefix as `i64`, return `(Some(epoch), naked_version)` on success, else `(None, raw.to_string())`).
- [ ] T003 [US1] In `dpkg.rs`: locate the PURL-construction site (grep for `pkg:deb/`), split the raw `Version:` field via T002's helper, pass the `Option<i64>` epoch through to the PURL builder, adding `?epoch=<N>` qualifier via `mikebom_common::types::purl::Purl` construction. Preserve non-epoch code path byte-identically (versions without `:` go through unchanged).
- [ ] T004 [P] [US1] Add unit tests to `dpkg.rs::tests`: (a) `parse_deb_version_with_epoch` for `"1:2.0-r0"` → `(Some(1), "2.0-r0")`; (b) same for `"0:1.0"` → `(Some(0), "1.0")` (edge: explicit epoch 0 preserved); (c) `"1.0-r0"` → `(None, "1.0-r0")`; (d) `"not-a-number:1.0"` → `(None, "not-a-number:1.0")` (graceful fallback).
- [ ] T005 [P] [US1] Add integration fixture `mikebom-cli/tests/fixtures/dpkg/epoch/`: minimal synthetic `.deb` with `Version: 1:2.0-r0`. Fixture generation may be a checked-in `.deb` or a `build.rs` step depending on maintainer convention — check existing dpkg fixtures for the pattern.
- [ ] T006 [US1] Add integration test `mikebom-cli/tests/scan_dpkg.rs::scan_dpkg_epoch_version_uses_qualifier_form`: scan the T005 fixture, assert emitted CDX component's PURL == `pkg:deb/debian/test-pkg@2.0-r0?epoch=1` (NOT `pkg:deb/debian/test-pkg@1:2.0-r0`).

**Checkpoint**: US1 acceptance scenarios pass; #562 closable.

---

## Phase 3: User Story 2 — apk epoch qualifier emission (Priority: P1)

**Goal**: Alpine `.apk` scans emit `pkg:apk/alpine/<name>@<version-without-epoch>?epoch=<N>`. Closes #563.

**Independent Test**: Synthetic `.apk` with epoch version scans to canonical `?epoch=` form.

### Implementation for User Story 2

- [ ] T007 [P] [US2] In `mikebom-cli/src/scan_fs/package_db/apk.rs`: add `parse_apk_version_with_epoch` mirroring T002's helper (same split-on-`:` logic; apk uses Debian-style epoch semantics).
- [ ] T008 [US2] In `apk.rs`: locate PURL-construction site, apply T007's helper, pass epoch through to Purl builder for `?epoch=<N>` qualifier. Preserve non-epoch code path byte-identically.
- [ ] T009 [P] [US2] Add unit tests to `apk.rs::tests` mirroring T004's shape (same 4 cases against `parse_apk_version_with_epoch`).
- [ ] T010 [P] [US2] Add integration fixture `mikebom-cli/tests/fixtures/apk/epoch/` — synthetic `.apk` with epoch-prefixed version.
- [ ] T011 [US2] Add integration test `mikebom-cli/tests/scan_apk.rs::scan_apk_epoch_version_uses_qualifier_form` asserting `pkg:apk/alpine/test-pkg@2.0-r0?epoch=1`.

**Checkpoint**: US2 acceptance scenarios pass; #563 closable.

---

## Phase 4: User Story 2b — rpm epoch qualifier non-regression audit (Priority: P1)

**Goal**: Verify rpm reader's pre-existing epoch handling still produces `?epoch=<N>` qualifier form. m197-native (no pre-existing GH issue).

**Independent Test**: Synthetic `.rpm` with `Epoch: 1` scans to `pkg:rpm/<vendor>/test-pkg@2.0-r0?epoch=1&...`.

### Implementation for User Story 2b

- [ ] T012 [P] [US2b] Add integration fixture `mikebom-cli/tests/fixtures/rpm/epoch/` — synthetic `.rpm` with `Epoch: 1, Version: 2.0, Release: r0`. Check existing rpm fixtures for pattern (m003/m004/m144 already ship rpm fixtures; extend the set).
- [ ] T013 [US2b] Add integration test `mikebom-cli/tests/scan_rpm.rs::scan_rpm_epoch_version_uses_qualifier_form` — scan T012's fixture, assert emitted CDX component's PURL matches `pkg:rpm/<vendor>/test-pkg@2.0-r0?epoch=1&...` (may include `&arch=...&distro=...` qualifiers per rpm.rs PURL construction convention; assert only the `?epoch=1` qualifier presence).
- [ ] T014 [US2b] If T013 fails (rpm reader NOT already correct despite m003/m004/m144 code path), file a follow-up issue AND apply the same fix pattern as T003 (US1) inside `rpm.rs`. If T013 passes (rpm already correct — the expected outcome), record findings in `specs/197-purl-reconciler-followups/scratch/rpm-audit-findings.txt`.

**Checkpoint**: rpm epoch handling confirmed; either non-regression documented or a fix mirrors T003.

---

## Phase 5: User Story 3 — Extend versionless-PURL fix to 6 additional ecosystems (Priority: P1)

**Goal**: Composer / dart / cocoapods / scala / haskell / erlang readers emit purl-spec-canonical `pkg:<type>/<name>` (no trailing `@`) for versionless deps. Closes #567.

**Independent Test**: For each of the 6 ecosystems, a scan of a versionless-dep-declaring fixture produces the canonical form.

### Implementation for User Story 3

- [ ] T015 [P] [US3] In `mikebom-cli/src/scan_fs/package_db/composer.rs`: locate `build_composer_purl` (or equivalent PURL-construction path per research §R2). Add `if version.is_empty()` short-circuit emitting `pkg:composer/<vendor>/<name>` (no `@`). Preserve versioned code path byte-identically.
- [ ] T016 [P] [US3] Same shape for `mikebom-cli/src/scan_fs/package_db/dart.rs` — versionless emits `pkg:pub/<name>`.
- [ ] T017 [P] [US3] Same shape for `mikebom-cli/src/scan_fs/package_db/cocoapods.rs` — versionless emits `pkg:cocoapods/<name>`.
- [ ] T018 [P] [US3] Same shape for `mikebom-cli/src/scan_fs/package_db/scala.rs` — versionless emits `pkg:maven/<groupId>/<artifactId>` (scala publishes via Maven Central per research §R2 quirk note; audit whether scala.rs constructs PURLs directly OR delegates to maven.rs — if the latter, no code change needed and this task is a non-regression assertion only).
- [ ] T019 [P] [US3] Same shape for `mikebom-cli/src/scan_fs/package_db/haskell.rs` — versionless emits `pkg:hackage/<name>`.
- [ ] T020 [P] [US3] Same shape for `mikebom-cli/src/scan_fs/package_db/erlang.rs` — versionless emits `pkg:hex/<name>`.
- [ ] T021 [US3] Add per-ecosystem unit tests to each of the 6 reader files' `::tests` module: for each, verify (a) versionless input → canonical form (no trailing `@`), (b) versioned input → byte-identical to pre-m197 output.
- [ ] T022 [P] [US3] Add integration fixtures under `mikebom-cli/tests/fixtures/{composer,dart,cocoapods,scala,haskell,erlang}/versionless/` — each with a minimal manifest declaring a versionless dep in that ecosystem.
- [ ] T023 [US3] Add integration tests (one per ecosystem in the existing `scan_<ecosystem>.rs` files where present, or create a new consolidated `mikebom-cli/tests/scan_versionless_purls.rs`) asserting the versionless-canonical PURL form for each of the 6 ecosystems' fixture from T022.

**Checkpoint**: All 6 additional ecosystems emit purl-spec-canonical versionless PURLs; #567 closable.

---

## Phase 6: User Story 4 — Fuzz-test versionless PURL round-trip (Priority: P2)

**Goal**: A fuzz suite covers all 11 ecosystems with ≥ 100 synthetic inputs each. Closes #566.

**Independent Test**: `cargo test -p mikebom-common versionless_purl_fuzz -- --nocapture` reports zero failures across ≥ 1100 total invocations.

### Implementation for User Story 4

- [ ] T024 [US4] Create new test file `mikebom-common/tests/versionless_purl_fuzz.rs`. Declare `const ECOSYSTEMS: &[(&str, &[&str])]` — 11 entries, each a `(ecosystem_type, name_shape_catalog)`. Per research §R3, name shapes include: empty, single-char, max-length per ecosystem, unicode where permitted, scoped (`@scope/name` for npm; `group:artifact` for maven; `vendor/pkg` for composer), URL-encoded segments, digit-prefixed, hyphen/underscore/dot mix. Aim for ≥ 10 unique shape variants per ecosystem — repeated to reach ≥ 100 invocations per ecosystem.
- [ ] T025 [US4] Implement `#[test] fn versionless_purl_fuzz_all_ecosystems()`: for each `(ecosystem, name)` pair, construct the ecosystem-appropriate versionless PURL via the reader's `build_*_purl` helper (or equivalent construction path — for pure `Purl::new(...)` inputs, format the PURL string manually to the ecosystem's canonical shape). Assert (a) `Purl::new(&s).is_ok()`, (b) `Purl::new(&s).unwrap().as_str() == s` (round-trip byte-identity), (c) `.ecosystem()` and `.name()` return expected values. On failure emit a diagnostic naming ecosystem + name + observed vs expected.
- [ ] T026 [US4] Verify test-count invariant: run `cargo test -p mikebom-common versionless_purl_fuzz -- --nocapture`, inspect diagnostic output showing per-ecosystem invocation counts. Assert each ecosystem shows ≥ 100 invocations. If a shortfall, extend the T024 catalog until the floor is met.

**Checkpoint**: Fuzz suite meaningful — 1100+ inputs across 11 ecosystems all round-trip byte-identically; #566 closable.

---

## Phase 7: User Story 6 — Always-array reconciler survivor shape (Priority: P2)

**Goal**: Reconciler emits `mikebom:requirement-ranges` + `mikebom:source-manifests` as JSON arrays (always, per Q1 clarification), replacing the m191 singular scalars. Preserves consumer-shape uniformity across single-vs-multi-declaration cases. Closes #565.

**Reason ordered before US5**: US5 also touches `reconciler.rs` and uses the array-emission pattern this story establishes; ordering avoids merge churn.

**Independent Test**: Monorepo fixture with 2 sibling manifests declaring same dep → survivor carries both ranges + both manifests as arrays.

### Implementation for User Story 6

- [ ] T027 [US6] In `mikebom-cli/src/resolve/reconciler.rs` (lines 85-105 per research §R4): rewrite the transfer logic. When a design-tier match is reconciled, ensure the survivor's `mikebom:requirement-ranges` and `mikebom:source-manifests` annotations are JSON arrays. On first match: initialize as 1-element arrays. On subsequent matches: append. Remove any code path emitting the singular `mikebom:requirement-range` / `mikebom:source-manifest` scalar keys.
- [ ] T028 [US6] In `reconciler.rs`: after all matches are transferred, apply the deterministic ordering per data-model.md E3: sort `mikebom:source-manifests` lex; reorder `mikebom:requirement-ranges` to match (Nth range corresponds to Nth manifest). Ensures golden byte-identity across reruns.
- [ ] T029 [P] [US6] Add unit tests to `reconciler.rs::tests`: (a) single-declaration case — survivor has 1-element arrays; (b) multi-declaration case — survivor has N-element arrays sorted per T028; (c) no-declaration case — survivor has neither annotation (arrays are only emitted when non-empty per E2/E3 validation rules).
- [ ] T030 [P] [US6] Add integration fixture `mikebom-cli/tests/fixtures/npm/multi-declaration/` per quickstart Reproducer 7 — 2 sibling `packages/{foo,bar}/package.json` declaring different `commander` ranges; root lockfile resolving both to `commander@11.1.0`.
- [ ] T031 [US6] Add integration test `mikebom-cli/tests/scan_npm.rs::scan_npm_multi_declaration_preserves_all_ranges` asserting the survivor `pkg:npm/commander@11.1.0` component carries `mikebom:requirement-ranges` and `mikebom:source-manifests` as 2-element arrays. **Assert across all 3 formats** (CDX, SPDX 2.3, SPDX 3) per contracts/annotation-shapes.md wire-shape contract — scan the T030 fixture with `--format cyclonedx-json,spdx-2.3-json,spdx-3-json`, then per-format extract the annotation (CDX from `.components[].properties[]`; SPDX 2.3 from `.packages[].annotations[].comment` JSON-in-string; SPDX 3 from `.[]` Annotation elements' `.statement` JSON-in-string) and verify the array shape is identical across all three. This closes the m197 analyze-phase C1 finding (SPDX cross-format verification gap).

**Checkpoint**: Reconciler survivor shape is always-array; #565 closable.

---

## Phase 8: User Story 5 — npm-alias reconciler resolved-identity matching (Priority: P2)

**Goal**: npm-alias declarations (`"my-alias": "npm:actual-pkg@1.0.0"`) reconcile against source-tier by resolved identity, with the original alias preserved as a `mikebom:declared-as` array annotation. Closes #564.

**Independent Test**: Fixture with npm-alias declaration → exactly one component per resolved identity + survivor carries `mikebom:declared-as: [<alias>]`.

**Depends on**: US6 (T027-T028) — uses the array-emission pattern.

### Implementation for User Story 5

- [ ] T032 [US5] In `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs`: extend the existing `AliasResolution` struct (or add a parallel one) to expose the raw alias name distinct from the resolved name. Add `parse_package_json_alias(dep_name: &str, dep_ver_raw: &str) -> Option<AliasResolution>` for the `"my-alias": "npm:actual@ver"` package.json form. Mirror the m159 `detect_pnpm_alias` shape.
- [ ] T033 [US5] In the npm reader's design-tier emission (grep the npm/ subdir for the manifest-parse path that creates PackageDbEntry from package.json direct deps): when `parse_package_json_alias` returns Some, stamp the emitted design-tier component with `mikebom:declared-as` (single-element array containing the alias name).
- [ ] T034 [US5] In `mikebom-cli/src/resolve/reconciler.rs`: extend the match-key logic per research §R4. When a design-tier component's `extra_annotations` contains `mikebom:declared-as`, match against source-tier by the RESOLVED name (extracted from the design-tier component's own PURL, which is keyed to `actual-pkg` post-alias-resolution), NOT the alias name. Accumulate `mikebom:declared-as` values across all reconciled matches onto the survivor as an array (sorted lex, dedup).
- [ ] T035 [P] [US5] Add unit tests to `alias_mapping.rs::tests` covering `parse_package_json_alias`: (a) `"my-alias": "npm:actual@1.0.0"` → `AliasResolution { alias: "my-alias", resolved: "actual", version: "1.0.0" }`; (b) `"regular": "^1.0.0"` → `None` (no `npm:` prefix); (c) `"my-alias": "npm:@scope/actual@1.0.0"` → alias resolves scoped-name variant.
- [ ] T036 [P] [US5] Add unit tests to `reconciler.rs::tests` covering the alias-aware match path: (a) design-tier with `declared-as` → matches source-tier by resolved name; (b) multi-manifest alias case → survivor has `declared-as` as 2-element sorted array.
- [ ] T037 [P] [US5] Add integration fixture `mikebom-cli/tests/fixtures/npm/alias/` per quickstart Reproducer 6 — `package.json` with `"my-alias": "npm:actual-pkg@1.0.0"` + resolving `package-lock.json`.
- [ ] T038 [US5] Add integration test `mikebom-cli/tests/scan_npm.rs::scan_npm_alias_reconciles_by_resolved_identity` asserting (a) exactly one `pkg:npm/actual-pkg@1.0.0` component emitted, (b) survivor carries `mikebom:declared-as: ["my-alias"]`, (c) no `pkg:npm/my-alias` phantom component. **Assert across all 3 formats** (CDX, SPDX 2.3, SPDX 3) per contracts/annotation-shapes.md wire-shape contract — same cross-format extraction pattern as T031. This closes the C1 finding for the m197 alias-annotation shape.

**Checkpoint**: npm-alias declarations correctly reconcile; #564 closable.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Golden regen for the m191 reconciler-path fixtures (bounded scope per Q1 exception), pre-PR gate, PR.

- [ ] T039 Grep existing goldens for m191 singular scalars per quickstart Reproducer 9:
  ```bash
  grep -rln '"mikebom:requirement-range"\|"mikebom:source-manifest"' mikebom-cli/tests/fixtures/golden/ > specs/197-purl-reconciler-followups/scratch/reconciler-golden-drift-set.txt
  ```
  Record the count + file list.
- [ ] T040 Regen goldens for the T039 drift set:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
    MIKEBOM_UPDATE_PKG_ALIAS_GOLDENS=1 MIKEBOM_UPDATE_OCI_PULL_GOLDENS=1 MIKEBOM_UPDATE_OPTIONAL_DEP_GOLDENS=1 \
    cargo test --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification
  ```
  Diff-review — every diff MUST be exclusively singular-→-array shape rotation per Q1. Reject any other class of diff.
- [ ] T041 Update CLAUDE.md agent-context: `.specify/scripts/bash/update-agent-context.sh claude` (idempotent if already run during /speckit-plan).
- [ ] T042 Run `./scripts/pre-pr.sh` — MUST pass green per SC-007 (wall-clock delta ≤ 5s vs pre-m197 baseline). If delta exceeds threshold, investigate — m197 adds no runtime cost, delta expected ≈0s.
- [ ] T043 Open the PR (via `gh pr create`) with title `impl(197): m190 + m191 follow-up bundle (7 items across dpkg / apk / rpm / 6 versionless ecosystems / fuzz test / reconciler)`. Body summarizes: (a) `Closes #562 #563 #564 #565 #566 #567`, (b) US2b rpm audit finding (non-regression or fix, per T014), (c) reconciler always-array migration note with consumer migration example (data-model.md E4), (d) golden regen scope (from T039 count).

**Checkpoint**: All 7 user stories delivered; pre-PR green; PR opened.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001. Independent.
- **US1 / US2 / US2b (Phases 2, 3, 4)**: T002-T014 all edit distinct reader files (dpkg.rs vs apk.rs vs rpm.rs); ALL 3 stories can proceed in parallel after Setup.
- **US3 (Phase 5)**: T015-T023 edit 6 distinct reader files; T015-T020 fully parallel; T021 and T023 need per-ecosystem unit + integration tests sequenced after the corresponding reader edit.
- **US4 (Phase 6)**: T024-T026. Depends on nothing outside Setup. Parallel with US1/US2/US2b/US3.
- **US6 (Phase 7)**: T027-T031 all edit `reconciler.rs` (T027, T028) then add tests. Sequential internally.
- **US5 (Phase 8)**: T032-T038. **DEPENDS ON US6 (T027-T028 in reconciler.rs)** because US5 adds to the same array-emission code path.
- **Polish (Phase 9)**: T039-T043. Depends on all user stories done.

### User Story Dependencies

- **US1 ⊥ US2 ⊥ US2b**: independent; parallel-capable across the 3 epoch-ecosystems.
- **US3**: 6 sub-tasks fully independent per ecosystem (different reader files).
- **US4**: independent; new file.
- **US6 → US5**: sequential (reconciler.rs collision).
- **US1/US2/US2b/US3/US4 can all proceed in parallel with US6, then US5 lands last before Polish.**

### Parallel Opportunities

- Phase 2 T004 + T005 [P] (unit vs integration fixture, different files)
- Phase 3 T007 [P] + T009 [P] + T010 [P] (US2 internal parallelism)
- Phase 4 T012 [P] (US2b integration fixture standalone)
- Phase 5 T015-T020 [P] (6 reader edits, all parallel); T022 [P] with T021
- Phase 6 T024-T026 (US4 sequential internally but parallel with US1/US2/US2b/US3)
- Phase 7 T029 [P] + T030 [P] (US6 unit test + integration fixture)
- Phase 8 T035 [P] + T036 [P] + T037 [P] (US5 tests + fixture)

---

## Parallel Example: US1 + US2 + US2b + US3 + US4 all in parallel

```bash
# After Setup (T001), 5 developer lanes proceed independently:
Developer A: T002 → T003 → T004[P]+T005[P] → T006      (US1 dpkg)
Developer B: T007[P] → T008 → T009[P]+T010[P] → T011   (US2 apk)
Developer C: T012[P] → T013 → T014                     (US2b rpm audit)
Developer D: T015-T020 [P] all parallel → T021 → T022[P] → T023  (US3 6 ecosystems)
Developer E: T024 → T025 → T026                         (US4 fuzz)

# Then US6 sequential (blocks on nothing but reconciler.rs collision with US5):
T027 → T028 → T029[P]+T030[P] → T031

# Then US5 (depends on US6's reconciler.rs shape):
T032 → T033 → T034 → T035[P]+T036[P]+T037[P] → T038

# Then Polish:
T039 → T040 → T041 || T042 → T043
```

---

## Implementation Strategy

### MVP (this milestone IS the MVP)

Every user story delivers a discrete acceptance-testable improvement. No smaller MVP is meaningful — cutting any of the 7 leaves a listed follow-up issue open (US1-US3, US5, US6) or leaves an m197-native audit unresolved (US2b, US4).

### Single-PR delivery (matches m190 / m191 / m192 / m194 / m195 / m196 shape)

Land Phases 1-9 in a single PR titled `impl(197): m190 + m191 follow-up bundle`. Commit granularity per user story (5-6 commits total: US1, US2+US2b combined, US3, US4, US6+US5 combined, Polish). Per-story commits preserve reviewer digestibility.

### Split-PR alternative (if bundle rejected in review)

If review-time signals the bundle is too big, split as:
- PR-A: US1 + US2 + US2b (epoch audits, ~15 tasks, ~200 LOC)
- PR-B: US3 (versionless-PURL extension, ~9 tasks, ~150 LOC)
- PR-C: US4 (fuzz test, ~3 tasks, ~100 LOC standalone new file)
- PR-D: US6 + US5 (reconciler work, ~12 tasks, ~250 LOC, must land in order)

Each sub-PR closes 1-3 GH issues; the split is naturally shaped by domain.

---

## Notes

- Total tasks: 43 across 9 phases.
- US1: 5 tasks (T002-T006). US2: 5 (T007-T011). US2b: 3 (T012-T014). US3: 9 (T015-T023). US4: 3 (T024-T026). US6: 5 (T027-T031). US5: 7 (T032-T038). Setup: 1. Polish: 5.
- **Zero new Cargo dependencies** (research §R3 audit — fuzz suite is hand-rolled catalog-driven).
- **1 new `mikebom:*` annotation** (`mikebom:declared-as`) — audited against native CDX/SPDX constructs per Principle V; no native alternative exists.
- **2 rotated annotations** (`mikebom:requirement-range/s`, `mikebom:source-manifest/s`) — singular scalars → always-array per Q1 clarification; FR-007 additive-only exception documented.
- **Golden regen scope** bounded by T039 grep-driven identification (m191 reconciler-path exercises only); every other golden holds byte-identically per FR-007.
- **CI cadence**: pre-PR gate (T042) is the only synchronous gate. Nightly public-corpus workflow (m195 / m196) will independently exercise the m197-changed reader paths on real public repos; if any drift, PR body should acknowledge and either regen the corpus goldens in a follow-up or update layer1 assertions.
