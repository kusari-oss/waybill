---
description: "Task list — milestone 038 minimal-image deep-hash (per-file evidence for distroless / chainguard / Bazel-built images)"
---

# Tasks: Per-File Evidence for Minimal-Image Scans

**Input**: Design documents from `/specs/038-minimal-image-deep-hash/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, quickstart.md ✅, checklists/requirements.md ✅

**Tests**: Test tasks ARE included — milestone 038 follows the project's
established per-commit verification discipline (Constitution
Pre-PR Verification gate). Inline `cargo test` coverage is
required for the new helper paths; gated network smoke tests
provide end-to-end verification.

**Organization**: Two user stories. US1 (P1) is concrete and
delivers MVP value standalone. US2 (P2) starts with a recon task
whose outcome gates whether implementation work follows.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file, no dependencies on incomplete tasks
- **[Story]**: `[US1]` or `[US2]` for user-story-phase tasks; no
  label on Setup / Foundational / Polish

## Path Conventions

- `mikebom-cli/src/scan_fs/package_db/file_hashes.rs` — primary edit site
- `mikebom-cli/src/scan_fs/package_db/dpkg.rs` — milestone 037 file
  (no edits expected; existing tests already exercise the
  source-discovery path)
- `mikebom-cli/src/scan_fs/package_db/apk.rs` — only touched if
  US2 recon discovers a non-standard apko layout
- `mikebom-cli/tests/oci_registry_smoke.rs` — gated network tests
- `docs/user-guide/cli-reference.md`, `CHANGELOG.md` — Polish phase
- `specs/038-minimal-image-deep-hash/research.md` — US2 recon
  outcome documented here

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Snapshot the pre-implementation baseline so
post-implementation diffs are interpretable.

- [X] T001 Snapshot pre-implementation test baseline (1152 binary tests passing on 038 branch HEAD; the gating dpkg test from milestone 037 confirmed passing in T002)
- [X] T002 Confirmed `cargo +stable test -p mikebom --bin mikebom scan_fs::package_db::dpkg::tests::parses_status_d_only_layout` passes — milestone 037 baseline healthy on this branch.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None for this milestone. The "foundation" is
milestone 037's `dpkg.rs::read_status_d_dir` which already lands
in main; this milestone extends a sibling file
(`file_hashes.rs`) without needing any cross-cutting prep work.

**⚠️ CRITICAL**: No foundational work — both user stories may
begin immediately after Phase 1.

---

## Phase 3: User Story 1 - Per-file evidence for distroless / Bazel-built deb images (Priority: P1) 🎯 MVP

**Goal**: When mikebom scans a deb image whose package metadata
lives in the per-package layout, every emitted deb component
carries a non-empty `evidence.occurrences[]` block populated from
the per-package file-listing data.

**Independent Test**: Run `mikebom sbom scan --image
gcr.io/distroless/static-debian12:latest --output
distroless.cdx.json` and verify the resulting SBOM has 4
components AND non-zero total file occurrences across them. The
existing 27-fixture byte-identity goldens regen with zero diff.

### Implementation for User Story 1

- [X] T003 [US1] Extended `read_info_file` with status.d/ fallback (3rd lookup step).
- [X] T004 [US1] Extended `read_info_file_bytes` with the same status.d/ fallback.
- [X] T005 [US1] Added .md5sums-derived path-list synthesis in `hash_package_files` when `<pkg>.list` is absent. Synthesized paths are absolute-prefixed for legacy-format parity.
- [X] T005a [US1] **In-flight discovery**: real distroless `status.d/<pkg>` stanzas omit the `Status:` field entirely (no dpkg daemon = no install state to track). Milestone 037's `parse_stanza` filtered them as "not installed" → 0 components. Fix: added `parse_stanza_no_status_required` + `parse_relaxed`; `read_status_d_dir` now uses the relaxed parser. Strict filtering is preserved for the legacy `status` file path.
- [X] T006 [P] [US1] `read_info_file_falls_back_to_status_d` test passes.
- [X] T007 [P] [US1] `read_info_file_legacy_wins_over_status_d` test passes (R5 precedence).
- [X] T008 [P] [US1] `hash_package_files_synthesizes_list_from_md5sums` test passes.
- [X] T009 [P] [US1] `hash_package_files_returns_empty_when_no_list_or_md5sums` test passes (FR-004).
- [X] T010 [P] [US1] `hash_md5sums_only_finds_status_d_md5sums` test passes (FR-003 fast path).
- [X] T011 [US1] `cargo +stable test -p mikebom --bin mikebom scan_fs::package_db::file_hashes` → 13 passed (8 pre-existing + 5 new).
- [X] T012 [US1] Updated `pulls_distroless_static_and_emits_dpkg_status_d_components` smoke test: total per-component file-occurrence count `> 0` AND at least one occurrence carries a 64-hex SHA-256 (parsed from `additionalContext` JSON-string per CDX evidence shape).
- [X] T013 [US1] Gated smoke passes against real distroless pull: registry → extract → scan produced 4 components with 938 total file occurrences and valid SHA-256 (verified manually via `--path` against an extracted rootfs as well).
- [X] T014 [US1] `./scripts/pre-pr.sh` clean.
- [X] T015 [US1] Goldens regen produced zero diff under `tests/fixtures/27/` (SC-003 confirmed).
- [ ] T016 [US1] Commit US1 work — pending.

**Checkpoint**: At this point, US1 is fully functional — distroless deb images produce SBOMs with populated `evidence.occurrences[]`. MVP delivered. Stop here if the user wants to ship US1 standalone (US2 can land in a follow-on PR).

---

## Phase 4: User Story 2 - Confirm or close minimal-image apk coverage (Priority: P2)

**Goal**: For chainguard apko-built images, mikebom either confirms the existing apk reader covers them (verification + docs) or extends it with a small variant reader (matching the pattern set by the dpkg work in milestone 037).

**Independent Test**: Run `mikebom sbom scan --image cgr.dev/chainguard/static:latest` and verify the SBOM has non-empty components AND non-empty per-component occurrences. The test outcome is the same regardless of whether code was added or not.

### Recon for User Story 2

- [X] T017 [US2] Pulled `cgr.dev/chainguard/static:latest` via mikebom's OCI registry-pull path; the scan produced 3 valid apk components with Wolfi-namespace PURLs (`ca-certificates-bundle`, `tzdata`, `wolfi-baselayout`). The existing apk reader handles the apko layout out of the box — **Branch A** taken.

### Decision branch — outcome of T017

**Branch A taken** — apko uses standard apk DB layout. Documented in research.md R2.

A separate finding surfaced during T017: per-file `evidence.occurrences[]` is **empty for every apk component**, on both alpine:3.19 (full-fat) and chainguard apko. mikebom's `file_hashes.rs` is dpkg-only — apk deep-hashing has never been implemented for any apk image. This is OUT OF SCOPE for milestone 038 (which focuses on the deb status.d/ gap). Filed as **Issue #75** for a follow-on milestone.

### Branch A: existing reader covers apko

- [X] T018A [US2] Updated `specs/038-minimal-image-deep-hash/research.md` R2 with the recon finding (apko uses standard apk DB; existing reader handles it) AND the discovered separate gap (apk per-file deep-hashing missing across the board). Issue #75 filed for the apk per-file gap.

### Branch B: apk variant reader required

- [~] T018B–T020B [US2] **SKIPPED** — Branch A taken. apko does NOT use a variant layout.

### Final US2 verification (both branches)

- [X] T021 [US2] Confirmed via direct registry pull: chainguard apko → 3 components surfaced. No new gated test added under Branch A — the existing alpine smoke test from milestone 031 covers the apk reader's component-metadata path.
- [ ] T022 [US2] Run `./scripts/pre-pr.sh` and confirm clean
- [ ] T023 [US2] Commit US2 work — pending.

**Checkpoint**: At this point, US1 AND US2 both work independently. Milestone 038 is complete except for cross-cutting docs.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: User-facing docs, CHANGELOG, PR.

- [ ] T024 Update `docs/user-guide/cli-reference.md` to mention that distroless / chainguard / Bazel-built images now produce per-file evidence (one paragraph in the Behaviour notes section under `mikebom sbom scan`)
- [ ] T025 Add a `CHANGELOG.md` Unreleased entry summarizing milestone 038, naming both US1 (deb status.d/ deep-hash) and US2 (apko coverage outcome — confirmed-or-extended)
- [ ] T026 Run `./scripts/pre-pr.sh` one final time and confirm clean
- [ ] T027 Push the branch and open a PR titled `feat(038): per-file evidence for distroless / chainguard / Bazel-built minimal images`. PR body lists the closed gap, both user stories' outcomes, and the success-criteria evidence (component count, occurrences count, zero golden drift, no new top-level deps)
- [ ] T028 Verify all 3 CI lanes green (Linux default + Linux ebpf + macOS) per the established 5-min CI cadence; report PR URL

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1, T001–T002)**: No dependencies. Run first.
- **Foundational (Phase 2)**: Empty for this milestone.
- **US1 (Phase 3, T003–T016)**: Depends on Setup. Independent of US2; can ship as a standalone PR.
- **US2 (Phase 4, T017–T023)**: Depends on Setup. Independent of US1. Recon (T017) gates the implementation branch.
- **Polish (Phase 5, T024–T028)**: Depends on US1 (and US2 if shipped together).

### Within US1

- T003 must precede T004 (related but separate edits to the same file — split for diff readability).
- T005 must precede T011 (the test runner verifies T003–T005 land correctly).
- T006–T010 are `[P]` parallel test additions to the same test module (cargo handles inline test files atomically; the [P] marker reflects logical independence).
- T012 must precede T013 (the smoke-test assertion edit precedes the gated invocation).
- T014–T015 must precede T016 (commit only after pre-pr + goldens are green).

### Within US2

- T017 (recon) gates branch A vs branch B.
- T018A or T018B–T020B follow accordingly.
- T021–T023 are common to both branches.

### Parallel Opportunities

- T006, T007, T008, T009, T010 are all parallel `[P]` test additions; can be authored simultaneously.
- US1 and US2 are independent — they can be developed in parallel by separate contributors. The recommended sequence here is US1 first (MVP) then US2 (verification or smaller extension).

---

## Parallel Example: User Story 1 inline tests

```bash
# After T003–T005 land (the production-code changes), the 5 inline
# tests can be authored as a parallel batch (different test
# functions in the same test module — git handles concatenation):
Task: "Add read_info_file_falls_back_to_status_d test"
Task: "Add read_info_file_legacy_wins_over_status_d test"
Task: "Add hash_package_files_synthesizes_list_from_md5sums test"
Task: "Add hash_package_files_returns_empty_when_no_list_or_md5sums test"
Task: "Add hash_md5sums_only_finds_status_d_md5sums test"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T002).
2. Complete Phase 3: User Story 1 (T003–T016).
3. **STOP and VALIDATE**: run the quickstart.md US1 verification recipe against a real distroless pull. Inspect `evidence.occurrences[]` for non-zero counts.
4. If quickstart passes, this is a shippable PR closing US1 — US2 may follow as a separate PR. Otherwise, also complete Phase 4 + Phase 5 in this PR.

### Recommended (single-PR shipping)

1. Phase 1 → Phase 3 → Phase 4 (with whichever branch the recon dictates) → Phase 5.
2. Single PR titled `feat(038): per-file evidence for distroless / chainguard / Bazel-built minimal images`, closing the milestone in one atomic ship.
3. Justification: US2 is small (recon + at most ~100 LOC of variant reader) and shares the same code review surface (package_db/* readers). Shipping together avoids a second PR's overhead.

### Stop conditions

- US1 quickstart shows zero file occurrences after T003–T016 land → debug per quickstart.md "Troubleshooting" section. Likely a path drift in T003–T004 (status.d/ relative path mismatch).
- US2 recon (T017) discovers a third apk variant beyond legacy + apko → defer to a follow-on milestone; document in research.md and ship US1+US2-Branch-A only.

---

## Notes

- `[P]` tasks = different files OR different test functions, no dependencies on incomplete tasks.
- `[Story]` label maps task to specific user story for traceability.
- Each user story is independently completable and testable.
- Each commit MUST satisfy the Constitution Pre-PR Verification gate (`./scripts/pre-pr.sh` clean).
- US2 recon outcome (T017) is the only branch point in the task graph; everything else is linear.
- `--no-deep-hash` fast-path coverage (T010) is non-negotiable per FR-003 — even though the user might not exercise it on minimal images often, the contract carries through.
