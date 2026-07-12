---
description: "Task list for m186 OCI Referrers API SBOM discovery"
---

# Tasks: OCI Referrers API SBOM discovery (m186)

**Input**: Design documents from `/specs/186-oci-referrers-sbom/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓, quickstart.md ✓

**Tests**: Included — spec.md and data-model.md §6 explicitly enumerate unit + integration test coverage (7 unit tests, 11 integration tests). TDD ordering applied within each phase (tests before implementation).

**Organization**: Tasks grouped by user story (US1 either-mode P1 → US2 strict-mode P1 → US3 backward-compat P1). All three stories are P1 because they collectively encode the FR-015 byte-identity gate — no story can ship without the others.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- Rust workspace root at repository root; production code under `mikebom-cli/src/`; integration tests under `mikebom-cli/tests/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline verification + zero-new-deps gate anchoring.

- [X] T001 Capture pre-m186 `cargo tree --workspace | wc -l` line count baseline in `specs/186-oci-referrers-sbom/artifacts/cargo-tree-pre.txt` (regenerable) for SC-008 verification at end of Phase 6.
- [X] T002 Verify `oci-spec = "0.9"` workspace dep already has `features = ["distribution", "image"]` at `Cargo.toml` (workspace root). If missing, adding `distribution` feature is the ONLY workspace `Cargo.toml` edit permitted this milestone per FR-016.

**Checkpoint**: Baseline captured; workspace deps confirmed sufficient.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The `SbomSourceMode` enum + module scaffolding all three stories depend on. Type-driven correctness gate per Principle IV.

**⚠️ CRITICAL**: US1 / US2 / US3 all block on Phase 2 completion.

- [X] T003 [P] Create new module file `mikebom-cli/src/scan_fs/oci_pull/referrers.rs` with SBOM_MEDIA_TYPES, media_type_for_mikebom_format, pick_sbom_descriptor (already implemented — carried T014 forward), plus 7 unit tests (T010–T013 all satisfied in the same pass).
- [X] T004 Add `mod referrers;` to `mikebom-cli/src/scan_fs/oci_pull/mod.rs` module declaration list.
- [X] T005 [P] Add `SbomSourceMode` enum to `mikebom-cli/src/cli/scan_cmd.rs` per data-model.md §1.1 (clap `ValueEnum` derive + `#[derive(Default)]` = Scan). Colocate with the existing `ImageSource` enum.
- [X] T006 Add `#[arg(long = "sbom-source", value_enum, default_value_t = SbomSourceMode::Scan)] pub sbom_source: SbomSourceMode` field to `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs` per data-model.md §1.2. Test helper `enrich_args` updated to include the new field.
- [X] T007 Add `pub async fn try_fetch_referrer_sbom(...) -> anyhow::Result<Option<ReferrerSbom>>` to `mikebom-cli/src/scan_fs/oci_pull/mod.rs` — full 5-step pipeline (already implements T017 forward: reference-parse + platform-resolve + fetch-manifest + fetch-referrers + pick-descriptor + fetch-verify blob). Also added `ReferrerSbom` struct, `DEFAULT_REFERRER_MAX_BYTES` constant, `resolve_referrer_max_bytes` helper, `sha2_hex` helper.
- [X] T008 Add `pub(super) async fn fetch_referrers(...) -> anyhow::Result<Option<oci_spec::image::ImageIndex>>` + `pub(super) async fn fetch_manifest_body(...)` to `mikebom-cli/src/scan_fs/oci_pull/registry.rs` — full impl (T015 forward): probe for HTTP 404, delegate to `fetch_with_auth_retry` on other paths, deserialize as `ImageIndex`. Also added `referrers_url` builder.
- [X] T009 Run `cargo +stable build --workspace --all-targets` — verified 0 errors (dead-code warnings for unwired-yet dispatch cleared in T018+).

**Checkpoint**: All three stories can now begin in parallel. Flag surface exists; dispatch pipeline stubs exist; type-driven correctness gate closed.

---

## Phase 3: User Story 1 — `either` mode: prefer referrer, fall through to scan (Priority: P1) 🎯 MVP

**Goal**: Operators can invoke `mikebom sbom scan --image <ref> --sbom-source either --format cyclonedx-json --output out.cdx.json` and get a referrer SBOM byte-identically when one exists, or a scanner-derived SBOM when it doesn't.

**Independent Test**: spec.md §User Story 1 Acceptance 1 (referrer present → byte-identical emit) + Acceptance 3 (no referrer → scan fall-through) + Acceptance 4 (HTTP 404 → silent fall-through). Verifiable via `mikebom-cli/tests/oci_referrers_either_mode.rs` (4 tests) against a wiremock server per research.md Decision 5.

### Tests for User Story 1 (write FIRST, ensure they FAIL before implementation) ⚠️

- [X] T010 [P] [US1] Unit test `pick_sbom_descriptor_prefers_format_match` in `referrers.rs` — passing.
- [X] T011 [P] [US1] Unit test `pick_sbom_descriptor_cdx_first_fallback` in `referrers.rs` — passing.
- [X] T012 [P] [US1] Unit tests `pick_sbom_descriptor_returns_none_on_empty_index` + `_returns_none_on_non_sbom_types` + `_skips_oversize_descriptors` + `_skips_attestation_envelopes` (F8 remediation bonus) + `_first_descriptor_tiebreaker` — 5 tests passing.
- [X] T013 [P] [US1] Unit test `media_type_for_mikebom_format_maps_cdx_and_spdx23` — passing.

### Implementation for User Story 1

- [X] T014 [US1] Implement `pick_sbom_descriptor` in `referrers.rs` — full priority-tier + size-cap filter.
- [X] T015 [US1] Implement `RegistryClient::fetch_referrers` in `registry.rs` — HTTP 404 probe short-circuit, delegate to `fetch_with_auth_retry` for auth flow.
- [X] T016 [US1] Add `fetch_manifest_body` to `RegistryClient` — used for SHA-256 manifest-digest derivation from tag references.
- [X] T017 [US1] Implement `try_fetch_referrer_sbom` in `oci_pull/mod.rs` — full 5-step pipeline + `MIKEBOM_REFERRER_MAX_BYTES` env read.
- [X] T018 [US1] Wire `either` mode dispatch in `scan_cmd.rs` — INFO provenance log with `sbom_source`, `descriptor_digest`, `media_type`, `output_path`, `bytes`.
- [X] T019 [US1] `mikebom-cli/tests/oci_referrers_either_mode.rs` — 4 tests all passing, including SC-005 provenance stderr assertions and F3 fall-through log assertion.

**Checkpoint**: US1 fully functional. `mikebom sbom scan --image <ref> --sbom-source either` works for all 4 acceptance scenarios. Ship as MVP.

---

## Phase 4: User Story 2 — `referrer` strict mode: require referrer or fail (Priority: P1)

**Goal**: Operators can invoke `mikebom sbom scan --image <ref> --sbom-source referrer --format cyclonedx-json --output out.cdx.json` and get a hard fail if no matching referrer is available. Fail-closed guarantee per Principle III.

**Independent Test**: spec.md §User Story 2 Acceptance 1 (referrer present → emit) + Acceptance 2 (no referrer → non-zero exit with actionable error) + Acceptance 3 (HTTP 404 → non-zero exit). Verifiable via `mikebom-cli/tests/oci_referrers_strict_mode.rs` (4 tests).

### Tests for User Story 2 (write FIRST) ⚠️

- [X] T020 [P] [US2] `referrer_mode_emits_matching_referrer` — passing with SC-005 provenance stderr assertions.
- [X] T021 [P] [US2] `referrer_mode_errors_on_no_match` — passing, asserts "no matching SBOM referrer found" + no output file.
- [X] T022 [P] [US2] `referrer_mode_errors_on_404_registry` — passing.
- [X] T023 [P] [US2] `referrer_mode_errors_on_size_cap` — passing.

### Implementation for User Story 2

- [X] T024 [US2] `referrer` mode dispatch branch wired in `scan_cmd.rs` — errors with actionable message; no output file written.
- [X] T025 [US2] Error classification via `anyhow::Context` chain — the underlying `try_fetch_referrer_sbom` error carries the FR-008 (a–e) reason class in its message.

**Checkpoint**: US2 fully functional. Fail-closed guarantee for compliance workflows. Combined US1+US2 covers the FR-008 + FR-009 contract pair.

---

## Phase 5: User Story 3 — `scan` mode (default): backward compatibility (Priority: P1)

**Goal**: All pre-m186 invocations (no `--sbom-source` flag OR explicit `--sbom-source scan`) produce byte-identical output. Zero network activity on the Referrers endpoint. FR-015 + SC-004 gate.

**Independent Test**: spec.md §User Story 3 Acceptance 1 (referrer available but scan mode → scan-derived output) + Acceptance 2 (default equals scan). Verifiable via `mikebom-cli/tests/oci_referrers_backward_compat.rs` (3 tests) + zero drift on existing golden fixtures.

### Tests for User Story 3 (write FIRST) ⚠️

- [X] T026 [P] [US3] `scan_mode_never_calls_referrers_endpoint` — passing; wiremock `.expect(0)` gate + manual received-requests audit.
- [X] T027 [P] [US3] `default_flag_absence_equivalent_to_scan_mode` — passing; verifies bytes DIVERGE from a live referrer and no FR-007 audit log appears.
- [X] T028 [P] [US3] `sbom_source_rejected_on_local_path_input` — passing; both `referrer` and `either` rejected on `--path`.
- [X] T028a [P] [US3] `referrers_endpoint_honors_insecure_registry_flag` — passing; verifies Referrers endpoint is reached via plain HTTP + byte-identity + provenance log.

### Implementation for User Story 3

- [X] T029 [US3] `scan` mode dispatch — `matches!(args.sbom_source, Referrer | Either)` outer gate skips the referrer path entirely under `Scan`; verified by T026's wiremock `.expect(0)`.
- [X] T030 [US3] FR-011 input-type guard implemented in `scan_cmd.rs` — rejects both `--path` and local-tarball `--image` under `Referrer|Either`.
- [X] T031 [US3] Golden zero-drift verification — see Phase 6 T034 (cargo tree) + full `pre-pr.sh` run in T035; no existing fixture invokes `--sbom-source`, so byte-identity is preserved by construction.

**Checkpoint**: US3 fully functional. Backward compatibility guaranteed. All three P1 stories complete.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final gates — pre-PR verification, cargo-tree zero-drift, docs.

- [X] T032 [P] README.md updated — new "OCI Referrers API (milestone 186 / #442)" subsection under §Scan an image with three worked examples + pointer to quickstart.md.
- [X] T033 [P] docs/design-notes.md — no OCI-pull section exists; skip per task spec.
- [X] T034 SC-008 zero-new-deps gate: `cargo tree --workspace | wc -l` = **1136 → 1136** — IDENTICAL. Zero new deps added.
- [X] T035 `./scripts/pre-pr.sh` — **PASSED**. `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. `cargo +stable test --workspace --no-fail-fast` — **238 test binaries all `test result: ok`, 0 failed**, EXIT=0.
- [X] T036 US1 byte-identity smoke test covered by `either_prefers_referrer_when_available` (verifies emitted bytes == referrer blob verbatim) + `default_flag_absence_equivalent_to_scan_mode` (verifies scan-mode default). No manual quickstart needed — the 12 wiremock integration tests exercise all 5 quickstart examples.
- [ ] T037 Commit + open PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001, T002 can run in parallel. No dependencies. ~5min.
- **Foundational (Phase 2)**: Depends on Setup. T003 + T005 can run in parallel; T004 depends on T003; T006 depends on T005; T007 + T008 can run in parallel after T004; T009 runs last. ~30min. **BLOCKS all user stories.**
- **US1 (Phase 3)**: Depends on Foundational completion.
- **US2 (Phase 4)**: Depends on Foundational completion. Can run in parallel with US1 if staffed, but shares `scan_cmd.rs` file with US1 → sequential in practice.
- **US3 (Phase 5)**: Depends on Foundational completion. Shares `scan_cmd.rs` with US1/US2 → sequential.
- **Polish (Phase 6)**: Depends on US1 + US2 + US3 completion.

### User Story Dependencies

- **US1 (P1)**: Foundational only. No dependency on US2 or US3.
- **US2 (P1)**: Foundational only. Reuses US1's `try_fetch_referrer_sbom` + `pick_sbom_descriptor` — so if US2 is implemented BEFORE US1, the shared implementations must ship as part of US2. Recommended order: US1 first (implements the shared code), then US2 (adds the strict-mode dispatch branch), then US3 (adds the input-type guard).
- **US3 (P1)**: Foundational only. Depends on US1/US2 for the `scan_cmd.rs` dispatch scaffold (US3 adds the FR-011 guard + the default-branch verification).

### Within Each User Story

- Tests FIRST (T010–T013, T020–T023, T026–T028) — verify they FAIL before implementation.
- Then implementation (T014–T019 for US1, T024–T025 for US2, T029–T031 for US3).
- Golden regen (T031) is the last US3 gate before Phase 6.

### Parallel Opportunities

- **Phase 1**: T001 || T002.
- **Phase 2**: T003 || T005 first; then T004 depends on T003, T006 depends on T005; T007 || T008 after T004; T009 last.
- **Phase 3 US1 tests**: T010 || T011 || T012 || T013 (all in the same file `referrers.rs` `#[cfg(test)]` block — must merge sequentially into that file if committed separately, but can be authored in parallel by separate contributors).
- **Phase 4 US2 tests**: T020 || T021 || T022 || T023 (same file — same caveat).
- **Phase 5 US3 tests**: T026 || T027 || T028 || T028a (same file — same caveat).
- **Phase 6**: T032 || T033 (different files).

---

## Parallel Example: User Story 1 tests

```bash
# Launch all 4 US1 unit test authoring tasks together:
Task: "Unit test pick_sbom_descriptor_prefers_format_match in referrers.rs #[cfg(test)] block"
Task: "Unit test pick_sbom_descriptor_cdx_first_fallback in referrers.rs #[cfg(test)] block"
Task: "Unit tests pick_sbom_descriptor_returns_none_on_empty_index / non_sbom_types / oversize in referrers.rs"
Task: "Unit test media_type_for_mikebom_format_maps_cdx_and_spdx23 in referrers.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001, T002) — ~5min.
2. Complete Phase 2: Foundational (T003–T009) — ~30min. **CRITICAL: blocks all stories.**
3. Complete Phase 3: US1 (T010–T019) — ~4h.
4. **STOP and VALIDATE**: Run `cargo +stable test --workspace` — verify US1 tests pass.
5. Optional: pause here for review; US1 alone delivers the `either` mode which is the most common use case.

### Incremental Delivery

1. Complete Setup + Foundational → foundation ready.
2. Add US1 → Test independently → validate `--sbom-source either` end-to-end. **This is the MVP demo point.**
3. Add US2 → Test independently → validate `--sbom-source referrer` fail-closed guarantee.
4. Add US3 → Test independently → validate backward compat + FR-011 guard.
5. Polish (Phase 6) → SC-008 gate + pre-PR gate + PR open.

### Sequential Team Strategy

With one developer:
- Days 1: Phase 1 + Phase 2 (setup + scaffolding).
- Day 1-2: Phase 3 US1 (write tests → implement → verify).
- Day 2: Phase 4 US2 (strict-mode dispatch + error templates).
- Day 2: Phase 5 US3 (backward compat + FR-011 guard).
- Day 3: Phase 6 (docs + pre-PR + PR).

### Parallel Team Strategy

With two developers post-Foundational:
- Dev A: US1 (T010–T019) — owns `referrers.rs` + `try_fetch_referrer_sbom` + `fetch_referrers`.
- Dev B: US2 tests (T020–T023) + US3 tests (T026–T028) in parallel with A's implementation (tests are file-independent from A's work).
- Dev A + Dev B rendezvous at US2/US3 implementation (both touch `scan_cmd.rs`) — one owns the merge.
- Both converge on Phase 6.

---

## Notes

- Each task has a concrete file path and a specific instruction — no vague "implement X" tasks.
- Tests before implementation per data-model.md §6 test-contract commitment.
- Every US phase includes an integration test file so each story is independently verifiable.
- Commit after each task or logical group. Avoid amending; use new commits per CLAUDE.md.
- `#[cfg(test)] mod tests { #[cfg_attr(test, allow(clippy::unwrap_used))] }` guard is REQUIRED in `referrers.rs` — the crate root denies `clippy::unwrap_used` per Constitution Principle IV; test code must opt out explicitly.
- Do NOT skip T031 (golden regen zero-drift verification) — FR-015 / SC-004 is the byte-identity gate and any drift here is a shippable-blocker regression.
- Do NOT skip T034 (cargo-tree zero-drift verification) — SC-008 zero-new-deps is a Constitution Principle I gate.
