---
description: "Task list — milestone 041 rpm FILEDIGESTS cross-reference"
---

# Tasks: Rpm FILEDIGESTS Cross-Reference

**Input**: spec.md ✅, plan.md ✅, checklists/requirements.md ✅.

**Tests**: included — inline coverage for the new accessor + the
threading + a gated smoke test.

**Organization**: Single user story (P1). Three atomic commits.

## Format: `[ID] [P?] [Story?] Description`

---

## Phase 1: Setup

- [ ] T001 `./scripts/pre-pr.sh` clean before any changes (baseline).
- [ ] T002 Confirm milestone 040's distroless / alpine / fedora pipelines still resolve via gated smoke run (or local manual `mikebom sbom scan --image fedora:40`).

---

## Phase 2: Commit `feat(041/extract-filedigests)`

- [ ] T003 [US1] Add `pub const TAG_FILEDIGESTS: u32 = 1035;` and `pub const TAG_FILEDIGESTALGO: u32 = 5011;` to `mikebom-cli/src/scan_fs/package_db/rpmdb_sqlite/rpm_header.rs` (alongside the existing TAG_BASENAMES etc.).
- [ ] T004 [US1] Add `pub fn file_digests(&self) -> Option<RpmFileDigests>` to `RpmHeader` returning `{ algo: RpmDigestAlgo, values: Vec<&str> }`. Looks up FILEDIGESTS via `string_array` and FILEDIGESTALGO via `int32_array().first()`. Algorithm decoding: `1=md5, 2=sha1, 8=sha256, 9=sha384, 10=sha512`. Absent / `0` defaults to MD5 per the rpm spec.
- [ ] T005 [US1] Add `pub enum RpmDigestAlgo { Md5, Sha1, Sha256, Sha384, Sha512 }` with a `pub fn name(&self) -> &'static str` returning the lowercase form (`"md5"`, `"sha1"`, etc.).
- [ ] T006 [US1] Add inline test `file_digests_decodes_sha256_payload` in `rpm_header.rs::tests`: synthetic header with FILEDIGESTS=["abc...", "def..."], FILEDIGESTALGO=8; assert returned struct has algo=Sha256 and values match.
- [ ] T007 [US1] Add inline test `file_digests_defaults_to_md5_when_algo_absent` in `rpm_header.rs::tests`: synthetic header with FILEDIGESTS but no FILEDIGESTALGO; assert algo=Md5.
- [ ] T008 [US1] Add inline test `file_digests_returns_none_when_filedigests_absent` in `rpm_header.rs::tests`: synthetic header with no FILEDIGESTS; assert None.
- [ ] T009 [US1] Add inline test `file_digests_returns_none_for_unknown_algo` in `rpm_header.rs::tests`: synthetic header with FILEDIGESTALGO=99; assert None (defensive — unknown algo treated as no cross-ref rather than mis-prefixed).
- [ ] T010 [US1] Update `iter_rpmdb` in `mikebom-cli/src/scan_fs/package_db/rpm.rs`: extend the visitor signature from `FnMut(PackageDbEntry, Vec<PathBuf>)` to `FnMut(PackageDbEntry, Vec<PathBuf>, Option<RpmFileDigests<'_>>)` (or owned variant). Existing call sites add an `_` for the new arg.
- [ ] T011 [US1] Update `pub fn read_file_lists` in `rpm.rs`: change return type from `HashMap<String, Vec<String>>` to `HashMap<String, Vec<RpmFileEntry>>` where `RpmFileEntry { path: String, digest: Option<String> }`. The digest is the algorithm-prefixed form, e.g. `"sha256:<hex>"`. Empty-or-mismatched-length per-file digests yield `digest: None`.
- [ ] T012 [P] [US1] Add inline test in `rpm.rs::tests` for `read_file_lists` returning per-file digests using the existing fixture-builder. (Use `build_test_header` with FILEDIGESTS + FILEDIGESTALGO=8.)
- [ ] T013 [US1] Existing `collect_claimed_paths` and the `read` function visitor adapter need updating (they ignore the new arg).
- [ ] T014 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T015 [US1] Commit: `feat(041/extract-filedigests): rpm FILEDIGESTS + FILEDIGESTALGO accessor on HeaderBlob`.

---

## Phase 3: Commit `feat(041/thread-and-emit)`

- [ ] T016 [US1] Add `rpm_file_digest: Option<String>` field to `mikebom_common::resolution::FileOccurrence`. `#[serde(default, skip_serializing_if = "Option::is_none")]` for backwards-compat.
- [ ] T017 [US1] Update `hash_rpm_package_files` in `file_hashes.rs`: change signature from `&[String]` to `&[RpmFileEntry]`. Inside the per-file loop, copy `entry.digest` onto the resulting `FileOccurrence.rpm_file_digest`.
- [ ] T018 [US1] Update existing `hash_rpm_package_files_*` inline tests to construct `RpmFileEntry`s with `digest: None`.
- [ ] T019 [US1] Edit `mikebom-cli/src/generate/cyclonedx/evidence.rs`: when serializing per-occurrence `additionalContext`, if `o.rpm_file_digest.is_some()`, include `"rpm_filedigest": "<value>"` alongside the existing keys.
- [ ] T020 [US1] Update the call site in `mikebom-cli/src/scan_fs/mod.rs`: `rpm_file_lists` now yields `Vec<RpmFileEntry>` not `Vec<String>`. Pass through unchanged.
- [ ] T021 [US1] Update existing `evidence.rs` inline tests that construct `FileOccurrence`s — add `rpm_file_digest: None` to each.
- [ ] T022 [P] [US1] Add inline test `hash_rpm_package_files_threads_digest_when_provided` in `file_hashes.rs::tests`: pass an `RpmFileEntry` with `digest: Some("sha256:<hex>")`; assert resulting occurrence's `rpm_file_digest` matches.
- [ ] T023 [US1] [optional] Extend `mikebom-cli/tests/oci_registry_smoke.rs` with a gated test that pulls `fedora:40`, asserts `> 0` rpm occurrences carry an `rpm_filedigest` key in `additionalContext`. Gated on `MIKEBOM_OCI_NETWORK_TESTS=1` like the existing alpine smoke.
- [ ] T024 [US1] Run goldens regen: `MIKEBOM_UPDATE_*_GOLDENS=1 cargo +stable test -p mikebom --test '*'`. Confirm zero diff under `mikebom-cli/tests/fixtures/27/` (the goldens use `--no-deep-hash` so they're insulated).
- [ ] T025 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T026 [US1] Commit: `feat(041/thread-and-emit): rpm_filedigest cross-ref in additionalContext alongside sha256`.

---

## Phase 4: Commit `docs(041)` + PR

- [ ] T027 [US1] Update `docs/user-guide/cli-reference.md` rpm paragraph: drop the "FILEDIGESTS as a future extension" qualifier (now closed); note that `rpm_filedigest` carries the algorithm-prefixed form.
- [ ] T028 [US1] Add CHANGELOG Unreleased entry summarizing milestone 041 — closes the Q1 deferral from milestone 040; rpm now has full cross-ref symmetry with deb (md5) and apk (sha1).
- [ ] T029 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T030 [US1] Commit: `docs(041): rpm_filedigest cross-ref — user-guide + CHANGELOG`.
- [ ] T031 Push branch.
- [ ] T032 Open PR titled `feat(041): rpm FILEDIGESTS cross-reference (closes milestone-040 Q1 deferral)`.
- [ ] T033 Verify all 3 CI lanes green.
