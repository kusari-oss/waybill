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

- [ ] T011 [P] [US1] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/opkg_basic/` mimicking a Yocto rootfs: `var/lib/opkg/status` (5 synthetic stanzas using `mikebom-fixture-*` package names), `usr/lib/opkg/info/<pkg>.list` files enumerating a couple of paths each. Per the milestone-106 lesson, all names use synthetic `mikebom-fixture-*` to dodge CVE flagging.
- [ ] T012 [P] [US3] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/yocto_sysroot/` mimicking an SDK sysroot: `environment-setup-mikebom-fixture-target` (empty file in the parent dir), `sysroots/mikebom-fixture-target/var/lib/opkg/status` (3 synthetic stanzas including one `nativesdk-mikebom-fixture-tool` to exercise FR-006).
- [ ] T013 [P] [US1] Add contract tests in `mikebom-cli/src/scan_fs/package_db/opkg.rs::tests`: `emits_basic_components`, `claims_files_from_info_dot_list`, `nativesdk_prefix_forces_build_scope`, `missing_version_emits_status_annotation`, `unknown_fields_silently_ignored`, `licenses_flow_through_to_pipeline`.
- [ ] T014 [P] [US3] Add contract tests in `mikebom-cli/src/scan_fs/package_db/yocto/context.rs::tests`: `env_script_in_scan_target_fires_primary`, `env_script_in_parent_dir_fires_primary`, `secondary_signal_fires_on_include_without_init_d`, `rootfs_when_neither_signal_fires`, `ambiguous_when_primary_fires_but_init_d_present`.

### Implementation

- [ ] T015 [US3] Create `mikebom-cli/src/scan_fs/package_db/yocto/mod.rs` (dispatcher entry) declaring `pub(super) mod context;` and stub modules for the manifest/recipe readers (added in later phases). Wire `pub mod yocto;` into `package_db/mod.rs`.
- [ ] T016 [US3] Create `mikebom-cli/src/scan_fs/package_db/yocto/context.rs` per `contracts/sysroot-context.md`: define `ScanContext` enum (`Sysroot { primary_signal, secondary_signal }` / `Rootfs` / `AmbiguousSysroot { reason }`); implement `pub(super) fn detect_scan_context(rootfs: &Path) -> ScanContext` with the two-signal logic (env-script glob primary; include + no-init.d secondary).
- [ ] T017 [US1] Create `mikebom-cli/src/scan_fs/package_db/opkg.rs` skeleton: define `pub fn read(rootfs: &Path) -> Vec<PackageDbEntry>` + `pub fn collect_claimed_paths(...)` signatures mirroring `dpkg.rs`. Returns empty when `/var/lib/opkg/status` is absent.
- [ ] T018 [US1] Implement opkg `read()`: reads `<rootfs>/var/lib/opkg/status`, delegates to `super::control_file::parse_stanzas`, calls `yocto::context::detect_scan_context(rootfs)` once, then iterates stanzas building `PackageDbEntry` values per `data-model.md`'s opkg field-mapping.
- [ ] T019 [US1] Implement PURL derivation: `pkg:opkg/<name>@<version>?arch=<arch>` via `Purl::new` + `encode_purl_segment`. Names/versions percent-encoded; arch passed verbatim (Yocto values like `cortexa7t2hf-neon-vfpv4` survive intact).
- [ ] T020 [US1/US5] Implement FR-006 per-stanza lifecycle-scope override: if stanza name starts with `nativesdk-` OR stanza arch matches a known host-arch literal (`x86_64`, `i686`, `aarch64`, `arm64`), tag the entry with `LifecycleScope::Build` regardless of `ScanContext` value.
- [ ] T021 [US1] Implement `opkg::collect_claimed_paths(rootfs, &mut claimed, &mut claimed_inodes)`: reads each `<rootfs>/usr/lib/opkg/info/<pkg>.list` and inserts each path into the binary-walker claim set + inode set. Mirrors `dpkg::collect_claimed_paths`.
- [ ] T022 [US1] Wire opkg into the dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs::read_all`: add `out.extend(opkg::read(rootfs));` after the existing dpkg call and `opkg::collect_claimed_paths(...)` after `dpkg::collect_claimed_paths(...)`.
- [ ] T023 [US3] Implement `ScanContext::AmbiguousSysroot` diagnostic emission: when context is ambiguous, push a `mikebom:scan-ambiguity` entry into the `ScanDiagnostics` collector so it surfaces in the emitted SBOM's `metadata.properties[]`.
- [ ] T024 [US1/US3] Add `SourceMechanism::OpkgInstalled` variant to the milestone-105 source-mechanism enum (or local equivalent). Update the precedence table per FR-010: `OpkgInstalled` outranks `YoctoImageManifest` outranks `BitbakeRecipe`. Add a `canonical_str` arm returning `"opkg-installed"`.

### Integration tests

- [ ] T025 [P] [US1] Add integration test `mikebom-cli/tests/scan_opkg.rs`: end-to-end binary scan of `opkg_basic/` fixture; assert expected `pkg:opkg/mikebom-fixture-*@<version>?arch=<arch>` PURLs present; license fields flow through; claim-path collection prevented duplicate `pkg:generic/*` emissions.
- [ ] T026 [P] [US3] Add integration test `mikebom-cli/tests/scan_yocto_sysroot.rs`: end-to-end binary scan of `yocto_sysroot/` fixture; assert every emitted opkg component carries CDX `scope: "excluded"` (proves the milestone-052 lifecycle-scope path correctly translated `LifecycleScope::Build` to the CDX scope field); assert the nativesdk component also carries scope=excluded; assert the SBOM metadata contains no `scan-ambiguity` annotation (primary signal fires from the fixture's `environment-setup-*` file; secondary signal is absent in the minimal fixture but that's a non-ambiguous case per FR-005a's table — only conflicting signals are flagged as ambiguous).

### Polyglot + PR

- [ ] T027 [US1/US3] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(opkg): add opkg-installed-DB reader + sysroot context detection (closes #NEW1)` — file new GitHub issues if none exist yet to track this US bundle.

**Checkpoint**: US1 + US3 + US5 shippable. Yocto rootfs + SDK sysroot scans produce real component data.

---

## Phase 4: User Story 2 — Yocto image manifest reader (Priority: P1)

**Goal**: Yocto build directory scans (post-`bitbake`) emit one component per line in `build/tmp/deploy/images/<machine>/<image>.manifest`.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/yocto_manifest_basic/`; assert the SBOM contains one `pkg:opkg/...` component per manifest line; nativesdk-prefixed lines emerge with `scope: "excluded"`.

### Fixtures + tests

- [ ] T028 [P] [US2] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/yocto_manifest_basic/build/tmp/deploy/images/qemux86-64/mikebom-fixture-image.manifest` with 5 lines (4 target packages + 1 nativesdk).
- [ ] T029 [P] [US2] Add contract tests in `mikebom-cli/src/scan_fs/package_db/yocto/manifest.rs::tests`: `emits_one_component_per_line`, `nativesdk_lines_tagged_build`, `wrong_token_count_warns_and_skips`, `empty_lines_ignored`.

### Implementation

- [ ] T030 [US2] Create `mikebom-cli/src/scan_fs/package_db/yocto/manifest.rs` per `contracts/yocto-image-manifest.md`: implement `pub(super) fn read(rootfs: &Path) -> Vec<PackageDbEntry>` walking `build/tmp/deploy/images/*/*.manifest` via `walkdir`. For each `.manifest`, line-iterate; split each non-empty non-`#` line on whitespace; emit one entry per 3-token line; skip + warn on wrong token count.
- [ ] T031 [US2] Implement PURL derivation: `pkg:opkg/<name>@<version>?arch=<arch>` — same shape as opkg-installed reader (the dedup pipeline collapses cross-source emissions on canonical PURL).
- [ ] T032 [US2] Implement FR-006 per-line lifecycle-scope override: lines where `name` starts with `nativesdk-` tag with `LifecycleScope::Build`. Other lines carry no lifecycle scope (default runtime per FR-005's manifest interpretation).
- [ ] T033 [US2] Wire `yocto::manifest::read` into the dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs::read_all`: call from `yocto::read` (or inline in `read_all` after the opkg call). Order matters for the milestone-105 dedup precedence — opkg-installed (Phase 3) must precede yocto-manifest in the dispatch order.
- [ ] T034 [US2] Add `SourceMechanism::YoctoImageManifest` variant with `canonical_str` returning `"yocto-image-manifest"`. Precedence: BELOW `OpkgInstalled`, ABOVE `BitbakeRecipe`.

### Integration test

- [ ] T035 [P] [US2] Add integration test `mikebom-cli/tests/scan_yocto_manifest.rs`: end-to-end binary scan of `yocto_manifest_basic/` fixture; assert 5 components emerge; assert the nativesdk-prefixed line has CDX `scope: "excluded"`.

### Polyglot + PR

- [ ] T036 [US2] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(yocto): add Yocto image manifest reader (closes #NEW2)`.

**Checkpoint**: US2 shippable. CI/CD pipelines that scan `build/` produce real per-image SBOMs.

---

## Phase 5: User Story 4 — BitBake recipe walker (Priority: P3)

**Goal**: Yocto layer-tree scans (a `meta-vendor/` repo checked out in isolation, pre-build) emit one `pkg:bitbake/<recipe>@<version>?layer=<layer>` component per `.bb` file.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/yocto_recipe_layer/`; assert one `pkg:bitbake/...` component per `.bb` file; recipes with `${PN}_${PV}.bb` filenames are skipped silently.

### Fixtures + tests

- [ ] T037 [P] [US4] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/yocto_recipe_layer/meta-mikebom-fixture/` with 4 `recipes-*/<name>/<name>_<version>.bb` files (one with `+git<sha>` version suffix, one with no `_<version>` segment to exercise the missing-version annotation, two normal). Add one `${PN}_${PV}.bb` to verify the silent-skip path. Include a `conf/layer.conf` (empty is fine) so the layer dir looks authentic.
- [ ] T038 [P] [US4] Add contract tests in `mikebom-cli/src/scan_fs/package_db/yocto/recipe.rs::tests`: `extracts_name_and_version_from_filename`, `emits_layer_qualifier_from_meta_ancestor`, `unexpanded_variables_skipped_silently`, `version_only_filename_emits_unknown_version_annotation`, `bbappend_and_bbclass_files_ignored`.

### Implementation

- [ ] T039 [US4] Create `mikebom-cli/src/scan_fs/package_db/yocto/recipe.rs` per `contracts/bitbake-recipe.md`: implement `pub(super) fn read(rootfs: &Path) -> Vec<PackageDbEntry>` walking the scan tree (max_depth=8 via `walkdir`) for `.bb` files matching the filename regex; skip `.bbappend` and `.bbclass`.
- [ ] T040 [US4] Implement the filename-extraction regex: `^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>[a-zA-Z0-9_\-\+\.\~]+)\.bb$`. Filenames containing `${` (literal unexpanded BitBake variable) match the skip-with-warn path per FR-008.
- [ ] T041 [US4] Implement layer-root detection: walk UP from the recipe's directory looking for the enclosing `meta-<name>/` directory; the basename becomes the `?layer=<layer>` PURL qualifier. Fall back to "path component above first `recipes-*/` directory" if no `meta-*/` ancestor exists.
- [ ] T042 [US4] Implement PURL derivation: `pkg:bitbake/<name>@<version>?layer=<layer_name>` via `Purl::new` + `encode_purl_segment`. Layer name passed verbatim into the qualifier.
- [ ] T043 [US4] Implement `mikebom:version-status: "missing"` annotation for `.bb` files with no `_<version>` segment (rare but legal — e.g. `helloworld.bb`).
- [ ] T044 [US4] Wire `yocto::recipe::read` into the dispatcher. Order in `read_all`: AFTER yocto-manifest (Phase 4), maintaining the FR-010 precedence order (opkg-installed > yocto-image-manifest > bitbake-recipe).
- [ ] T045 [US4] Add `SourceMechanism::BitbakeRecipe` variant with `canonical_str` returning `"bitbake-recipe"`. Lowest-precedence among the 107 milestone's three new variants.

### Integration test

- [ ] T046 [P] [US4] Add integration test `mikebom-cli/tests/scan_yocto_recipe.rs`: end-to-end binary scan of `yocto_recipe_layer/` fixture; assert 3 components emerge (4 well-formed recipes minus the 1 that's skipped with no-version annotation = 3 valid; or assert 4 components emerge with one carrying `mikebom:version-status` = "missing" — depends on T043's exact handling); assert `${PN}_${PV}.bb` recipe is silently skipped; assert PURL `?layer=meta-mikebom-fixture` qualifier present on all emitted entries.

### Polyglot + PR

- [ ] T047 [US4] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(yocto): add BitBake recipe walker for layer-tree scans (closes #NEW3)`.

**Checkpoint**: US4 shippable. Layer-tree audit scans emerge with one component per declared recipe.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, FR-011 offline-mode audit extension, SC-006 polyglot-robustness regression test. Mirrors the milestone-106 polish PR (#288) pattern.

- [ ] T048 Update `docs/ecosystems.md`: add a new top-level `## yocto` section covering all four new readers (opkg-installed, yocto-manifest, bitbake-recipe, sysroot-context) with format docs + PURL shape + lifecycle-scope behavior + out-of-scope items. Update the coverage matrix at the top of the file with a new `[yocto](#yocto)` row.
- [ ] T049 [P] FR-011 offline-mode audit: add `mikebom-cli/tests/offline_mode_audit_ecosystem_107.rs` that greps the 5 new reader source files (`opkg.rs`, `yocto/mod.rs`, `yocto/manifest.rs`, `yocto/recipe.rs`, `yocto/context.rs`) for forbidden substrings (`reqwest::`, `tokio::net::`, `hyper::`, `Command::new("curl"|"wget"|"http"`, `TcpStream::`, `TcpListener::`, `std::net::TcpStream/Listener`). Any match fails the build. Asserts FR-011 independently of the readers' own behavior.
- [ ] T050 [P] FR-014 SC-006 polyglot-robustness regression: add `mikebom-cli/tests/polyglot_robustness_ecosystem_107.rs` mirroring milestone-106's pattern. Build a temp fixture with well-formed manifests from all 3 new ecosystems (a valid opkg DB, a valid `.manifest`, a valid `.bb` layer) AND deliberately-malformed siblings (opkg DB with garbage stanzas, `.manifest` with wrong token counts, `.bb` files with unparseable names). Assert: scan exits 0; each well-formed manifest still emits its representative component despite the sibling malformed files; the milestone-106 ecosystems (uv/Bun/Gradle/NuGet/Yarn) ALSO still emit from their fixtures if present (cross-milestone regression check).
- [ ] T050a [P] SC-007 cross-reader dedup determinism regression: add `mikebom-cli/tests/cross_reader_dedup_ecosystem_107.rs`. Build a fixture containing BOTH an opkg-installed DB (`var/lib/opkg/status`) AND a Yocto image manifest (`build/tmp/deploy/images/mikebom-fixture-machine/mikebom-fixture-image.manifest`) that name the same canonical PURL — `pkg:opkg/mikebom-fixture-coord@1.2.3?arch=mikebom-fixture-arch`. Scan the fixture; assert: (a) exactly ONE component emerges with that canonical PURL (collapsed by the milestone-105 dedup pipeline); (b) the loser's source-mechanism value (`"yocto-image-manifest"`) appears in the surviving component's `mikebom:also-detected-via` annotation; (c) the surviving component's lifecycle-scope tag came from the higher-precedence reader (`OpkgInstalled` > `YoctoImageManifest`). Locks the FR-010 precedence contract against regression.
- [ ] T051 [P] SC-003 performance check: re-run the golden-fixture scan suite, compare wall-clock to the T003 baseline. If delta exceeds 5%, profile + optimize. The expected delta is negligible (opkg reader uses the shared dpkg parser; manifest reader is line-oriented; recipe walker short-circuits on file extension).
- [ ] T052 Run the `quickstart.md` Scenario 1-4 end-to-end against representative real-world inputs: a publicly-downloadable Yocto qemux86-64 reference image (rootfs scan), a public Yocto build directory if one is available in CI fixtures, an OpenSTLinux SDK sysroot if accessible, a public `meta-*/` GitHub repo (layer scan). Confirm each scenario produces the expected component-count ranges from spec.md §Data Volume Assumptions.
- [ ] T053 Run `./scripts/pre-pr.sh` clean. Open polish PR titled `docs+test: milestone 107 polish — ecosystem docs + FR-011 audit + SC-006 robustness`.

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
