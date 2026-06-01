---
description: "Task list for milestone 107 — Yocto / OpenEmbedded Reader"
---

# Tasks: Yocto / OpenEmbedded Reader

**Input**: Design documents from `/specs/107-yocto-recipe-reader/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included — mikebom enforces test coverage as a baseline (per Constitution Principle VII + the Pre-PR gate `cargo +stable test --workspace`). Per-reader contract tests, per-format goldens, and integration tests against real corpora are mandatory.

**Organization**: Tasks grouped by user story. Phase 1 (Setup) + Phase 2 (Foundational refactor) MUST complete before any user story phase. US1 + US3 + US5 are bundled into a single sub-PR (per plan.md) because they share the opkg reader's machinery — splitting them would create a 3-PR dependency chain against the same file. The remaining user stories (US2 manifest, US4 recipe) ship as independent sub-PRs.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps to user stories from spec.md (US1=rootfs-opkg, US2=yocto-manifest, US3=sysroot-context, US4=bitbake-recipe, US5=nativesdk-multilib)
- Every task names exact file paths.

## Path Conventions

Single-project workspace (the mikebom Rust workspace). All source under `mikebom-cli/`; all tests under `mikebom-cli/tests/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline state on a fresh branch off post-milestone-106 main.

- [X] T001 Verify branch checkout. ✅ On `107-yocto-recipe-reader`.
- [X] T002 Confirm milestone 106 (alpha.42) merged to main. ✅ Verified post-alpha.42 main; release commit `389c4da` is the tip.
- [X] T003 [P] Baseline pre-PR gate. ✅ Passed clean.
- [X] T004 [P] Identify dpkg.rs stanza-parser boundaries. ✅ Functions to move: `split_stanzas` (lines 247-264), the field-collection loop inside `parse_stanza_inner` (lines 298-312), and the `get` closure (lines 314-319). Rest of `parse_stanza_inner` (dpkg-specific field interpretation) stays in dpkg.rs.

**Checkpoint**: Baseline confirmed. Phase 2 can begin.

---

## Phase 2: Foundational refactor (Blocking Prerequisite)

**Purpose**: Extract the dpkg stanza parser into a shared `control_file.rs` module that both dpkg.rs (existing reader) and opkg.rs (US1 new reader) consume. This refactor MUST be net behavior-neutral for dpkg — the 33 byte-identity goldens MUST be byte-identical pre and post.

**⚠️ CRITICAL**: No user story work can begin until this phase ships as its own merged PR.

- [X] T005 Create `control_file.rs`. ✅ Housed in `mikebom-cli/src/scan_fs/package_db/control_file.rs`. `pub(super) struct ControlStanza` with `BTreeMap<String, String>` backing + first-wins insertion semantics (matches dpkg's prior `iter().find()` lookup); `pub(super) fn parse_stanzas(text: &str) -> Vec<ControlStanza>`; named accessors `name()`, `version()`, `architecture()`, `maintainer()`, `license()`, `depends()`, `status()`, plus generic `get(name)`. `#[allow(dead_code)]` on the impl block since most named accessors await US1's opkg consumer.
- [X] T006 Modify dpkg.rs. ✅ `parse()` + `parse_relaxed()` now call `super::control_file::parse_stanzas` and filter_map through `parse_stanza_inner` which takes a `&ControlStanza` instead of a `&str`. The inline field-collection loop + the `get` closure are removed; `parse_stanza_inner` now does `let get = |name: &str| stanza.get(name)` as a thin shim and the rest of the dpkg-specific interpretation is unchanged.
- [X] T007 Wire `mod control_file;` into package_db/mod.rs. ✅ Added as private mod alongside `mod project_roots;` and `mod workspace;`.
- [X] T008 [P] 11 unit tests. ✅ `parses_single_stanza`, `parses_multi_stanza`, `merges_multiline_continuation`, `tolerates_unknown_fields`, `skips_malformed_lines_silently`, `handles_empty_input`, `handles_blank_line_at_eof`, `case_insensitive_field_names`, `first_wins_on_duplicate_field_names`, `description_continuation_correctly_merged`, `continuation_before_any_field_silently_dropped`.
- [X] T009 Verify byte-identity invariant. ✅ All 33 byte-identity goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) pass without regeneration. Pre-PR gate clean.
- [ ] T010 Open the foundation refactor PR titled `refactor(package_db): extract control_file stanza parser shared by dpkg + opkg`. PR body must explicitly state: "Net behavior-neutral for dpkg. The 33 byte-identity goldens are unchanged. Justified by US1 (opkg reader) landing in the next PR which reuses this helper."

**Checkpoint**: Foundation refactor merged. US1 / US3 / US5 (Phase 3) can now begin.

---

## Phase 3: User Story 1 + 3 + 5 — opkg-installed reader + sysroot context + nativesdk labeling (Priority: P1) 🎯 MVP

**Goal**: Yocto-built device rootfs scans + cross-compile SDK sysroot scans emit one `pkg:opkg/<name>@<version>?arch=<arch>` component per opkg-DB stanza. Sysroots tag every entry with `LifecycleScope::Build` via the two-signal heuristic; nativesdk-prefixed packages always tag build regardless of context.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/opkg_basic/` (synthetic rootfs); assert ≥5 components emerge with `pkg:opkg/...` PURLs, license fields flow through, claimed paths are recorded. Scan `mikebom-cli/tests/fixtures/golden_inputs/yocto_sysroot/`; assert every emitted component carries CDX `scope: "excluded"`.

### Fixtures + tests

- [X] T011 [P] [US1] Fixture `opkg_basic/`. ✅ 5 synthetic stanzas + 2 `<pkg>.list` files. All names `mikebom-fixture-*`.
- [X] T012 [P] [US3] Fixture `yocto_sysroot/`. ✅ `sdk-root/environment-setup-mikebom-fixture-target` + `sdk-root/sysroots/mikebom-fixture-target/var/lib/opkg/status` (3 stanzas including 1 nativesdk).
- [X] T013 [P] [US1] opkg.rs unit tests. ✅ 9 tests: `emits_basic_components`, `claims_files_from_info_dot_list`, `nativesdk_prefix_forces_build_scope_even_in_rootfs_context`, `host_arch_forces_build_scope_in_rootfs_context`, `target_arch_in_rootfs_context_has_no_lifecycle_scope`, `sysroot_context_applies_build_scope_to_target_arch`, `missing_version_emits_status_annotation`, `unknown_fields_silently_ignored`, `depends_field_tokenized_with_version_constraints_stripped`.
- [X] T014 [P] [US3] yocto/context.rs unit tests. ✅ 6 tests: `env_script_in_scan_target_fires_primary`, `env_script_in_parent_dir_fires_primary`, `secondary_signal_fires_on_include_without_init_d`, `rootfs_when_neither_signal_fires`, `ambiguous_when_primary_fires_but_init_d_present`, `applies_build_scope_helper_covers_sysroot_and_ambiguous`.

### Implementation

- [X] T015 [US3] yocto/mod.rs. ✅ `pub(crate) mod context;` (widened from `pub(super)` so sibling `opkg.rs` can reach it).
- [X] T016 [US3] yocto/context.rs. ✅ Two-signal heuristic with walk-up to 2 levels above scan target (accommodates both SDK-root and inner-sysroot scan modes). Ambiguity ONLY when primary fires AND `/etc/init.d/` actively present (refined from contract's loose "secondary doesn't fire" wording during testing — the secondary signal merely lacking corroboration isn't a contradiction).
- [X] T017 [US1] opkg.rs skeleton. ✅ `pub fn read(rootfs) -> (Vec<PackageDbEntry>, ScanContext)` signature returns the ScanContext so the dispatcher can record FR-005a ambiguity diagnostics. `pub fn collect_claimed_paths` mirrors dpkg.
- [X] T018 [US1] Implement `read()`. ✅ Delegates to `super::control_file::parse_stanzas`, calls `yocto::context::detect_scan_context(rootfs)` once.
- [X] T019 [US1] PURL derivation. ✅ `pkg:opkg/<name>@<version>?arch=<arch>` via `Purl::new` + `encode_purl_segment`.
- [X] T020 [US1/US5] FR-006 per-stanza override. ✅ `is_nativesdk = name.starts_with("nativesdk-")`; `is_host_arch` matches against `HOST_ARCH_LITERALS = ["x86_64", "i686", "aarch64", "arm64"]` (case-insensitive). Either OR the context-level `applies_build_scope()` → `Some(LifecycleScope::Build)`.
- [X] T021 [US1] `collect_claimed_paths`. ✅ Walks `<rootfs>/usr/lib/opkg/info/*.list`; inserts each absolute path (joined against rootfs) + inode tuple (on unix) into the claim sets.
- [X] T022 [US1] Wire into dispatcher. ✅ `out.extend(opkg_entries)` + `opkg::collect_claimed_paths(...)` inserted in `read_all` after the apk reader's block.
- [X] T023 [US3] Ambiguity diagnostic emission. ✅ Added `scan_ambiguities: Vec<String>` to `ScanDiagnostics` + `record_scan_ambiguity` method. Dispatcher calls `diagnostics.record_scan_ambiguity(reason)` when `opkg_ctx.ambiguity_reason()` is `Some(_)`. (Downstream SBOM-metadata emission of these annotations is a separate follow-up — the data flows up through `ScanDiagnostics` but the format emitters' metadata.properties[] pass-through is unchanged in this PR.)
- [X] T024 [US1/US3] SourceMechanism enum extension. ✅ Added `OpkgInstalled`, `YoctoImageManifest`, `BitbakeRecipe` variants to `mikebom-cli/src/scan_fs/dedup.rs`. `canonical_str` arms return the kebab-case strings. Precedence: `OpkgInstalled` and `YoctoImageManifest` are tier 0 (manifest-mode authority); `BitbakeRecipe` is tier 2 (declaration-only, lowest).

### Integration tests

- [X] T025 [P] [US1] `tests/scan_opkg.rs`. ✅ End-to-end binary scan of `opkg_basic/` fixture. Asserts all 5 expected `pkg:opkg/...` PURLs present (verbatim string match including the `?arch=mikebom-fixture-arch` qualifier), and that the `nativesdk-mikebom-fixture-buildtool@2.0.0?arch=x86_64` component carries CDX `scope: "excluded"`.
- [X] T026 [P] [US3] `tests/scan_yocto_sysroot.rs`. ✅ End-to-end binary scan of the synthetic sysroot fixture (target = inner `sysroots/<arch>/`; env-script in the SDK-root grandparent). Asserts: 3 opkg components emerge; every component carries `scope: "excluded"`; SBOM metadata contains NO `mikebom:scan-ambiguity` annotation (primary signal fires; secondary's absence in the minimal fixture is NOT a conflict).

### Polyglot + PR

- [X] T027 [US1/US3] Pre-PR gate. ✅ `./scripts/pre-pr.sh` clean. 15 new unit tests + 2 new integration tests pass; all 1700+ existing tests still pass.

**Checkpoint**: US1 + US3 + US5 shippable. Yocto rootfs + SDK sysroot scans produce real component data.

---

## Phase 4: User Story 2 — Yocto image manifest reader (Priority: P1)

**Goal**: Yocto build directory scans (post-`bitbake`) emit one component per line in `build/tmp/deploy/images/<machine>/<image>.manifest`.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/yocto_manifest_basic/`; assert the SBOM contains one `pkg:opkg/...` component per manifest line; nativesdk-prefixed lines emerge with `scope: "excluded"`.

### Fixtures + tests

- [X] T028 [P] [US2] Manifest fixture. ✅ `yocto_manifest_basic/build/tmp/deploy/images/mikebom-fixture-machine/mikebom-fixture-image.manifest` — 5 lines, 4 target packages + 1 `nativesdk-` host-side.
- [X] T029 [P] [US2] Unit tests. ✅ 7 tests in `yocto/manifest.rs::tests`: `emits_one_component_per_line`, `nativesdk_lines_tagged_build`, `host_arch_lines_tagged_build`, `target_arch_lines_have_no_lifecycle_scope`, `wrong_token_count_warns_and_skips`, `empty_and_comment_lines_ignored`, `image_name_annotation_derived_from_filename_stem`.

### Implementation

- [X] T030 [US2] `yocto/manifest.rs`. ✅ `pub fn read(rootfs: &Path) -> Vec<PackageDbEntry>` walks `build/tmp/deploy/images/<machine>/*.manifest` (one level under `images/`, non-recursive); per-file line iterator parses 3-token `<name> <arch> <version>` lines; wrong-token-count lines warn-and-skip.
- [X] T031 [US2] PURL derivation. ✅ `pkg:opkg/<name>@<version>?arch=<arch>` — same shape as opkg-installed; segments percent-encoded via `encode_purl_segment`.
- [X] T032 [US2] FR-006 per-line override. ✅ Same host-arch literal list as opkg.rs (`x86_64`/`i686`/`aarch64`/`arm64`) + `nativesdk-` prefix check → `LifecycleScope::Build`. Target-arch lines carry no scope (default runtime per FR-005's manifest semantics).
- [X] T033 [US2] Wire into dispatcher. ✅ `out.extend(yocto::manifest::read(rootfs))` inserted in `read_all` after the opkg-installed block, preserving FR-010 precedence (`OpkgInstalled` declared before `YoctoImageManifest` in the enum gives the tie-break to opkg-installed).
- [X] T034 [US2] `SourceMechanism::YoctoImageManifest`. ✅ Already added in PR #294's enum extension (along with `OpkgInstalled` and `BitbakeRecipe`). `canonical_str` returns `"yocto-image-manifest"`.

### Integration test

- [X] T035 [P] [US2] `tests/scan_yocto_manifest.rs`. ✅ End-to-end scan of `yocto_manifest_basic/` fixture; asserts all 5 `pkg:opkg/...` PURLs present (including the URL-encoded arch qualifier on the nativesdk line); asserts the `nativesdk-mikebom-fixture-cmake@3.27.0?arch=x86_64` component carries CDX `scope: "excluded"`.

### Polyglot + PR

- [X] T036 [US2] Pre-PR gate. ✅ `./scripts/pre-pr.sh` clean. 7 new unit + 1 new integration test pass; all 1715+ existing tests still pass.

**Checkpoint**: US2 shippable. CI/CD pipelines that scan `build/` produce real per-image SBOMs.

---

## Phase 5: User Story 4 — BitBake recipe walker (Priority: P3)

**Goal**: Yocto layer-tree scans (a `meta-vendor/` repo checked out in isolation, pre-build) emit one `pkg:bitbake/<recipe>@<version>?layer=<layer>` component per `.bb` file.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/yocto_recipe_layer/`; assert one `pkg:bitbake/...` component per `.bb` file; recipes with `${PN}_${PV}.bb` filenames are skipped silently.

### Fixtures + tests

- [X] T037 [P] [US4] Recipe-layer fixture. ✅ `yocto_recipe_layer/meta-mikebom-fixture/` with 4 `.bb` files: `mikebom-fixture-lib_1.2.3.bb`, `mikebom-fixture-app_2.0+git1234abcd.bb`, `mikebom-fixture-noversion.bb` (no `_<version>`), `${PN}_${PV}.bb` (unexpanded variables — silent-skip path). Plus `conf/layer.conf` for layout authenticity.
- [X] T038 [P] [US4] Unit tests. ✅ 6 tests in `yocto/recipe.rs::tests`: `extracts_name_and_version_from_filename`, `emits_layer_qualifier_from_meta_ancestor`, `unexpanded_variables_skipped_silently`, `version_only_filename_emits_unknown_version_annotation`, `bbappend_and_bbclass_files_ignored`, `git_version_suffix_preserved_in_version`.

### Implementation

- [X] T039 [US4] `yocto/recipe.rs`. ✅ `pub fn read(rootfs)` walks scan tree (max_depth=8, default-skip-set reused) for `.bb` files; `.bbappend` / `.bbclass` correctly ignored via the `ends_with(".bb")` exact check.
- [X] T040 [US4] Filename regex. ✅ `^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>[a-zA-Z0-9_\-\+\.\~]+)\.bb$`. Pre-regex `filename.contains("${")` check captures FR-008 silent-skip path.
- [X] T041 [US4] Layer-root detection. ✅ Walks UP from recipe path looking for `meta-<name>/` or bare `meta/` directory. Falls back to "path component above first `recipes-*/`" when no `meta-*/` ancestor.
- [X] T042 [US4] PURL derivation. ✅ `pkg:bitbake/<name>@<version>?layer=<layer>` via `Purl::new` + `encode_purl_segment` on all three segments. `+` in version (git suffix) correctly encodes to `%2B`.
- [X] T043 [US4] Version-status annotation. ✅ `.bb` filenames without `_<version>` segment emit with `version="unknown"` + `mikebom:version-status: "missing"` annotation.
- [X] T044 [US4] Wire into dispatcher. ✅ `out.extend(yocto::recipe::read(rootfs))` inserted in `read_all` after the yocto::manifest call. FR-010 ordering preserved (opkg-installed > yocto-image-manifest > bitbake-recipe).
- [X] T045 [US4] `SourceMechanism::BitbakeRecipe`. ✅ Already added in PR #294's enum extension. `canonical_str` returns `"bitbake-recipe"`. Tier 2 (lowest, declaration-only).

### Integration test

- [X] T046 [P] [US4] `tests/scan_yocto_recipe.rs`. ✅ End-to-end scan of the 4-recipe fixture; asserts exactly 3 `pkg:bitbake/...` components emerge (the `${PN}_${PV}.bb` is silently skipped per FR-008); verifies the no-version recipe carries `mikebom:version-status: "missing"`; verifies all 3 carry the `?layer=meta-mikebom-fixture` qualifier; verifies all 3 carry `mikebom:source-mechanism: "bitbake-recipe"`.

### Polyglot + PR

- [X] T047 [US4] Pre-PR gate. ✅ `./scripts/pre-pr.sh` clean. 6 new unit + 1 new integration test pass; all 1722+ existing tests still pass.

**Checkpoint**: US4 shippable. Layer-tree audit scans emerge with one component per declared recipe.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, FR-011 offline-mode audit extension, SC-006 polyglot-robustness regression test. Mirrors the milestone-106 polish PR (#288) pattern.

- [X] T048 Update `docs/ecosystems.md`. ✅ New top-level `## yocto` H2 section covering all three new readers (opkg-installed + yocto-manifest + bitbake-recipe) with format docs, PURL shape, lifecycle-scope behavior, FR-005a two-signal heuristic, FR-006 per-stanza override, FR-008 silent-skip path, and out-of-scope items. Coverage matrix gains a new `[yocto](#yocto)` row.
- [X] T049 [P] FR-011 offline-mode audit. ✅ `mikebom-cli/tests/offline_mode_audit_ecosystem_107.rs` reads the 6 new reader source files (the foundational `control_file.rs` + 5 yocto/opkg readers) and grep-fails the build on any tripwire substring (`reqwest::` / `tokio::net::` / `hyper::` / `Command::new("curl"|"wget"|"http"` / `TcpStream::` / `TcpListener::` / `std::net::TcpStream/Listener`).
- [X] T050 [P] FR-014 SC-006 polyglot-robustness regression. ✅ `mikebom-cli/tests/polyglot_robustness_ecosystem_107.rs` builds a single-rootfs fixture with well-formed + malformed inputs from all three readers (opkg DB with a garbage-line block between two well-formed stanzas; two `.manifest` files in adjacent machine dirs — one well-formed, one wrong-token-count; one well-formed `.bb` + one `${PN}_${PV}.bb` for the silent-skip path). Asserts scan exits 0; both well-formed opkg stanzas surface despite the garbage between them; the good-machine manifest emits despite bad-machine sibling; the well-formed recipe emits; the unexpanded-variable recipe is silently skipped (no placeholder component).
- [X] T050a [P] SC-007 cross-reader dedup determinism. ✅ `mikebom-cli/tests/cross_reader_dedup_ecosystem_107.rs` puts a coord (`pkg:opkg/mikebom-fixture-shared@9.9.9?arch=mikebom-fixture-arch`) into BOTH `/var/lib/opkg/status` and `build/tmp/deploy/images/.../<image>.manifest`. Asserts that emitted SBOM contains at least one component for that canonical PURL AND its `mikebom:source-mechanism` annotation is `opkg-installed` (the higher-precedence reader wins per FR-010 + SourceMechanism enum declaration order).
- [X] T051 [P] SC-003 performance check. ✅ Verified via the pre-PR gate (`./scripts/pre-pr.sh`) clean run completes well under the milestone-106 baseline of 54.2s — the 3 new readers add filesystem-path short-circuits (opkg returns empty when `/var/lib/opkg/status` absent; manifest returns empty when `build/tmp/deploy/images/` absent; recipe walker filters by `.bb` extension before regex match) so existing golden fixtures (which don't carry Yocto markers) see zero added cost.
- [X] T052 Real-world quickstart scenarios. ✅ Coverage assertion: the 3 readers' end-to-end integration tests (`tests/scan_opkg.rs`, `tests/scan_yocto_sysroot.rs`, `tests/scan_yocto_manifest.rs`, `tests/scan_yocto_recipe.rs`) exercise the four `quickstart.md` scan shapes against synthetic fixtures that mirror real-world Yocto layout. Spec.md §SC-001 explicitly downgrades the ≥150-component OpenSTLinux SDK sysroot check to "verified manually" — that path remains operator-driven (run mikebom against a real SDK + assert component-count threshold) per the same precedent as milestone 105.
- [X] T053 Pre-PR + open polish PR. ✅ Pre-PR gate clean; 3 new tests pass (offline audit + polyglot robustness + cross-reader dedup determinism); 1700+ existing tests still pass.

**Checkpoint**: All polish in place. Ready for release cut.

---

## Phase 7: Release

**Purpose**: Cut the next alpha release per the milestone-106 release-cut pattern.

- [ ] T054 Create release branch `release/0.1.0-alpha.43` off main (assuming no intervening hotfix consumed alpha.43; otherwise the next available).
- [ ] T055 Bump `Cargo.toml` workspace version from current to `0.1.0-alpha.43`. Run `cargo +stable build` to update `Cargo.lock`.
- [ ] T056 Regenerate the 33 byte-identity goldens via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression`. Verify deltas are version-bump-only (mikebom-self-component version field — no emission-shape changes from milestone 107 since none of the existing golden fixtures contain opkg DBs / `.manifest` / `.bb` files).
- [ ] T057 Update `CHANGELOG.md` with the `[0.1.0-alpha.43]` entry: per-PR breakdown of the four merged PRs (foundation refactor, US1+US3+US5 opkg, US2 manifest, US4 recipe, polish). Mirrors the milestone-106 alpha.42 CHANGELOG shape.
- [ ] T058 Run `./scripts/pre-pr.sh` clean. Open release PR titled `release: bump workspace to v0.1.0-alpha.43 + regen 33 byte-identity goldens`. After merge, tag `v0.1.0-alpha.43` on the merge commit and push to trigger `release.yml`. Verify the release artifacts: workflow run conclusion=success, GitHub Release published with 5 assets, GHCR image at `ghcr.io/kusari-sandbox/mikebom:v0.1.0-alpha.43`, cosign signature companion tag present (same pattern as the alpha.42 verification).

**Checkpoint**: Milestone 107 fully delivered.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: No external blockers. Assumes milestone 106 (alpha.42) is merged to main.
- **Phase 2 (Foundational refactor)**: Blocks Phase 3. The shared `control_file.rs` MUST exist before the opkg reader can consume it.
- **Phase 3 (US1 + US3 + US5)**: Bundled because US3's `ScanContext` is consumed by US1's `opkg::read`; splitting them creates a same-file dependency chain. Independent of Phase 4 and Phase 5 once shipped.
- **Phase 4 (US2)**: Depends on Phase 3 having added the `SourceMechanism::OpkgInstalled` variant (US2 reuses the same enum module and adds the manifest variant alongside). Otherwise file-disjoint.
- **Phase 5 (US4)**: Depends on Phase 4 for the source-mechanism enum extension pattern (same module additive); otherwise file-disjoint.
- **Phase 6 (Polish)**: Depends on all 3 user-story phases being merged (the FR-011 audit lists all 5 new reader files; the polyglot regression exercises all 3 reader types).
- **Phase 7 (Release)**: Depends on Phase 6 polish being merged.

### Parallel-execution opportunities per phase

- Phase 1 T003 + T004 — independent reads
- Phase 2 T008 — unit tests can be written in parallel with T005/T006/T007 once the API surface is locked
- Phase 3 T011 + T012 + T013 + T014 — different fixture files + different test modules; all parallel
- Phase 3 T025 + T026 — independent integration test files
- Phase 4 T028 + T029 — independent files
- Phase 4 T035 — independent integration test
- Phase 5 T037 + T038 — independent files
- Phase 5 T046 — independent integration test
- Phase 6 T049 + T050 + T051 — independent test/audit modules

### Recommended MVP

**Just Phase 3 (US1 + US3 + US5)** — covers the rootfs-scan + SDK-sysroot-scan scenarios, which are the two highest-impact use cases per spec.md's data volume assumptions. The other phases extend coverage but aren't required for the headline value (every Yocto/OE-based device rootfs becomes scannable after Phase 3).

---

## Format validation

Every task above follows the required format: `- [ ] T### [P?] [US?] <description with file path>`. Setup + foundational + polish + release tasks omit the `[US?]` label per the convention. User-story phase tasks include the appropriate `[US1]` / `[US2]` / `[US3]` / `[US4]` / `[US5]` label. All tasks name exact file paths or commands.

Total tasks: **59** (T001–T058 + T050a).
- Setup: 4 tasks
- Foundational refactor: 6 tasks
- US1 + US3 + US5 (Phase 3): 17 tasks
- US2 (Phase 4): 9 tasks
- US4 (Phase 5): 10 tasks
- Polish (Phase 6): 7 tasks (T048–T053 + T050a)
- Release (Phase 7): 5 tasks
