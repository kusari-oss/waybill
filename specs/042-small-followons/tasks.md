---
description: "Task list — milestone 042 post-041 small follow-ons"
---

# Tasks: Post-041 Small Follow-Ons

**Input**: spec.md ✅, checklists/requirements.md ✅. (No plan / research /
data-model / quickstart — both stories mirror prior-milestone patterns
exactly; the spec.md is concrete enough.)

**Tests**: included — inline coverage for the new Debian sidecar reader.

**Organization**: Two unrelated user stories. Three commits.

## Format: `[ID] [P?] [Story?] Description`

---

## Phase 1: Setup

- [ ] T001 `./scripts/pre-pr.sh` clean before any changes (baseline).

---

## Phase 2: Commit `fix(042/us1)` — stale comment cleanup

- [ ] T002 [US1] Edit `mikebom-cli/src/scan_fs/binary/predicates.rs` (around line 131): rewrite the comment block. Drop the "RPM file-list extraction from HeaderBlob BASENAMES/DIRNAMES/DIRINDEXES is deferred to a follow-on milestone" claim. Preserve the presume-owned heuristic's defense-in-depth rationale (it's still correct as a fallback even with the authoritative extraction now in place).
- [ ] T003 [US1] Verify SC-001: `grep -rn 'extraction from HeaderBlob BASENAMES.*deferred\|file-list extraction.*deferred to a follow-on milestone' mikebom-cli/src/` returns zero matches.
- [ ] T004 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T005 [US1] Commit: `fix(042/us1): drop stale "rpm HeaderBlob extraction deferred" comment in binary/predicates.rs`.

---

## Phase 3: Commit `feat(042/us2)` — Maven sidecar Debian layout

- [ ] T006 [US2] Add `pub(crate) struct DebianSidecarIndex { by_basename: HashMap<String, PathBuf> }` to `mikebom-cli/src/scan_fs/package_db/maven_sidecar.rs`. Mirrors the existing `FedoraSidecarIndex` shape so the maven module's call sites can swap or layer them with no signature change.
- [ ] T007 [US2] Add `pub(crate) fn DebianSidecarIndex::build(rootfs: &Path) -> Self`: walks `<rootfs>/usr/share/maven-repo/` recursively, collecting `<group-path>/<artifact>/<version>/<artifact>-<version>.pom` entries. The basename key strips the `-<version>.pom` suffix and lowercases. The recursive walk has a depth cap to avoid infinite-symlink loops (cap = 8 segments, comfortably above any realistic GAV depth).
- [ ] T008 [US2] Add `pub(crate) fn DebianSidecarIndex::lookup_for_jar(&self, jar_path: &Path) -> Option<&Path>`: same shape as the Fedora variant; strips trailing `-<version>` from the JAR filename before matching.
- [ ] T009 [US2] Add `pub(crate) fn DebianSidecarIndex::is_empty(&self) -> bool` (test-only) and `pub(crate) fn len(&self) -> usize` for scan-summary logging.
- [ ] T010 [US2] Edit the call site in `mikebom-cli/src/scan_fs/package_db/maven.rs` that invokes `FedoraSidecarIndex::build`: also build a `DebianSidecarIndex` and consult both during JAR-coordinate recovery. Fedora wins on basename collision (FR-005).
- [ ] T011 [P] [US2] Add inline test `debian_sidecar_index_extracts_canonical_gav` in `maven_sidecar.rs::tests`: synthetic rootfs with `/usr/share/maven-repo/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.pom`; assert the index has the expected basename → path mapping AND `lookup_for_jar` for `commons-lang3-3.12.0.jar` resolves.
- [ ] T012 [P] [US2] Add inline test `debian_sidecar_index_handles_multi_segment_groups` in `maven_sidecar.rs::tests`: synthetic POM at `org/apache/maven/plugins/maven-compiler-plugin/3.11.0/maven-compiler-plugin-3.11.0.pom`; assert the multi-level group resolves correctly.
- [ ] T013 [P] [US2] Add inline test `debian_sidecar_index_handles_version_with_build_suffix` in `maven_sidecar.rs::tests`: POM at `<...>/foo/1.0.0-SNAPSHOT/foo-1.0.0-SNAPSHOT.pom`; assert the trailing `-SNAPSHOT` doesn't break the basename-stripping logic.
- [ ] T014 [P] [US2] Add inline test `debian_sidecar_index_returns_empty_for_missing_directory` in `maven_sidecar.rs::tests`: tempdir with no `/usr/share/maven-repo/`; assert `is_empty()` is true (FR-006).
- [ ] T015 [P] [US2] Add inline test `debian_sidecar_index_returns_empty_for_empty_directory` in `maven_sidecar.rs::tests`: tempdir with `/usr/share/maven-repo/` directory but no contents; assert `is_empty()` is true (FR-006).
- [ ] T016 [P] [US2] Add inline test `fedora_sidecar_wins_on_basename_collision` in `maven_sidecar.rs::tests`: synthetic rootfs with both layouts containing the same basename `commons-lang3`; assert the Fedora-layout entry wins (FR-005).
- [ ] T017 [US2] Run `cargo +stable test -p mikebom --bin mikebom scan_fs::package_db::maven_sidecar` and confirm 6 new tests pass alongside any existing.
- [ ] T018 [US2] Run goldens regen: `MIKEBOM_UPDATE_*_GOLDENS=1 cargo +stable test -p mikebom --test '*'`. Confirm zero diff under `mikebom-cli/tests/fixtures/` (FR-008).
- [ ] T019 [US2] `./scripts/pre-pr.sh` clean.
- [ ] T020 [US2] Commit: `feat(042/us2): Maven sidecar Debian /usr/share/maven-repo/ GAV-layout reader`.

---

## Phase 4: Commit `docs(042)` + PR

- [ ] T021 Update `docs/user-guide/cli-reference.md` to mention Debian-shaped Java images now resolve to maven PURLs (one-line addition or a behaviour note).
- [ ] T022 Add CHANGELOG Unreleased entry summarizing the milestone.
- [ ] T023 `./scripts/pre-pr.sh` clean.
- [ ] T024 Commit: `docs(042): Maven Debian sidecar — user-guide note + CHANGELOG`.
- [ ] T025 Push branch.
- [ ] T026 Open PR titled `feat(042): post-041 small follow-ons (stale comment + Maven Debian sidecar)`.
- [ ] T027 Verify all 3 CI lanes green.
