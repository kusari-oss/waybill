---
description: "Task list for milestone 075 — strip userinfo credentials from auto-detected git URLs"
---

# Tasks: Strip userinfo credentials from auto-detected git URLs

**Input**: Design documents from `/specs/075-strip-id-credentials/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/credential-strip.md, quickstart.md

**Tests**: Spec references SC-001 through SC-008 plus 7 unit tests for `sanitize_userinfo`. Test tasks are included.

**Organization**: Four user stories (3× P1, 1× P2). The implementation is small enough that all stories share a single test file and most production-code tasks. Phases group tasks by what blocks what.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 / US4 (story-phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace; all 075 changes inside `mikebom-cli` plus a small workspace-Cargo.toml change.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Promote the `url` crate to a direct workspace dep.

- [X] T001 Add `url = "2"` to `[workspace.dependencies]` in `/Users/mlieberman/Projects/mikebom/Cargo.toml`. Add `url = { workspace = true }` to `[dependencies]` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/Cargo.toml`. Run `cargo build --workspace` and verify `Cargo.lock` is unchanged (the crate is already at `url 2.5.8` transitively per research §2). If lockfile churn appears, investigate before proceeding.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Ship the core sanitization helper and updated auto-detect signatures. After this phase, the production code is sanitization-aware but no integration tests have been written yet.

**⚠️ CRITICAL**: User-story phases depend on these.

- [X] T002 Implement `pub fn sanitize_userinfo(url: &str) -> SanitizedUrl` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/binding/identifiers/auto_detect.rs`. Behavior per data-model.md §`sanitize_userinfo`: parse via `url::Url::parse`; on Err return passthrough; on success check `username().is_empty() == false || password().is_some()`; if userinfo present call `set_username("")` then `set_password(None)`; on either setter Err return passthrough; on success return `SanitizedUrl { original, sanitized: parsed.to_string(), was_sanitized: true }`. Add the private `SanitizedUrl` struct in the same file. Add 7 unit tests per contracts/credential-strip.md "Test contract" unit-test list (covering: user+password, user-only, empty userinfo, port preservation, parse failure passthrough, no-userinfo passthrough, SSH-form passthrough). Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

- [X] T003 Implement private helper `fn redact_userinfo_for_log(url_str: &str) -> String` in the same file. Replaces userinfo with the literal string `<userinfo redacted>` while preserving scheme/host/path/query/fragment. Used by the info-log call sites in T004 + T005. Add unit tests for: with-userinfo URL gets `<userinfo redacted>` substituted; no-userinfo URL passes through; parse-failure case returns the input unchanged.

- [X] T004 Update `auto_detect_repo_identifier` signature in the same file to add the trailing `keep_credentials: bool` parameter per data-model.md "Updated public signatures". Wire into the body: after `discover_repo_url` succeeds, call `sanitize_userinfo(&url)` (or skip and use passthrough when `keep_credentials == true`); use `sanitized.sanitized` as the identifier value; emit `tracing::info!` per FR-006 when `was_sanitized == true`; augment `source_label` with ` (credentials stripped)` per research §3 when `was_sanitized == true`. Update existing unit tests in `auto_detect.rs` to pass `false` (default behavior) for the new param so they keep covering source-tier behavior. Verify all existing source-tier tests still pass.

- [X] T005 Update `auto_detect_build_tier_identifiers` signature to take the same `keep_credentials: bool` param. Wire into the body for both the `repo:` construction (mirrors T004) and the `git:` construction (sanitize the URL portion BEFORE appending `#<sha>`, per VR-075-005). At the top of the function, log `tracing::info!("--keep-credentials-in-identifiers set; userinfo in auto-detected identifiers will be preserved verbatim")` when `keep_credentials == true` per FR-007. Update existing tests to pass `false`. Verify all existing build-tier tests still pass.

- [X] T006 [P] Add the `--keep-credentials-in-identifiers` flag to `ScanArgs` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/cli/scan_cmd.rs` (clap `Args`-derive: `#[arg(long)]` `pub keep_credentials_in_identifiers: bool`). Pass `args.keep_credentials_in_identifiers` to the existing `auto_detect_repo_identifier` call (around `scan_cmd.rs:1467`). Help text per contracts/credential-strip.md "Help-text shape".

- [X] T007 [P] Add the same `--keep-credentials-in-identifiers` flag to `RunArgs` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/cli/run.rs`. Pass `args.keep_credentials_in_identifiers` to the existing `auto_detect_build_tier_identifiers` call. Help text per contracts/credential-strip.md.

**Checkpoint**: production code is sanitization-aware. CLI flags wired. All existing tests pass with the default-false flag value. Both user-story phases can begin.

---

## Phase 3: User Story 1 — Source-tier auto-detect strips credentials by default (Priority: P1)

**Goal**: `mikebom sbom scan --path .` in a git checkout with a credentialed origin URL produces a source SBOM with the userinfo stripped from the `repo:` identifier value, the `source_label` augmented, and an info log emitted.

**Independent Test**: Build a tempdir git fixture with `origin` set to `https://USER:TOKEN@github.com/foo/bar.git`. Run `mikebom sbom scan --path <fixture> --output out.cdx.json`. Assert: emitted `repo:` URL is `https://github.com/foo/bar.git`; literal token string occurs zero times in `out.cdx.json`; `comment` field contains `(credentials stripped)`.

### Tests for User Story 1

- [X] T008 [US1] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/identifiers_credential_strip.rs` with the tempdir-based git fixture builder helper (same pattern as `mikebom-cli/tests/identifiers_build_tier_autodetect.rs`'s helper from milestone 074). Add tests:
  - (a) `source_tier_strips_credentials_from_https_origin` — fixture has credentialed origin; assert `repo:` value sanitized + zero literal-token occurrences in emitted JSON.
  - (b) `source_tier_source_label_carries_stripped_suffix` — same fixture; assert `comment` field contains `(credentials stripped)`.
  - (c) `source_tier_emits_redacted_info_log` — capture log output (test-tracing or similar pattern); assert info-level log line contains `<userinfo redacted>` and host but NOT the literal token.
  - (d) `source_tier_ssh_form_unchanged` — fixture has SSH-form origin; assert emitted `repo:` URL is byte-identical to the original; `comment` field has NO `(credentials stripped)` suffix.

  Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

**Checkpoint**: US1 passes. Source-tier flow correctly sanitizes by default.

---

## Phase 4: User Story 2 — Build-tier auto-detect strips credentials by default (Priority: P1)

**Goal**: `mikebom trace run` in a credentialed git checkout produces a build-tier SBOM with both `repo:` and `git:` identifiers sanitized, source_label augmented, info logs emitted for each.

**Independent Test**: Same fixture as US1 plus a HEAD commit. Invoke `auto_detect_build_tier_identifiers` directly (as in milestone 074's tests) and assert both identifier slots sanitized; literal token string absent.

### Tests for User Story 2

- [X] T009 [US2] Add to `mikebom-cli/tests/identifiers_credential_strip.rs`:
  - (a) `build_tier_strips_credentials_from_repo_and_git` — fixture has credentialed origin + a commit; assert both auto-detected `repo:` and `git:` identifiers have URLs without userinfo; literal token string occurs zero times in the resulting `Vec<Identifier>` (serialize each `value` and grep). The test invokes `auto_detect_build_tier_identifiers` directly per the milestone-074 pattern (eBPF unavailable in standard test env).
  - (b) `build_tier_git_value_has_sha_appended_after_sanitization` — VR-075-005: assert the `git:` value matches `git:https://github.com/<...>.git#<40-hex-sha>` and contains zero `@` characters in the URL portion (because credentials stripped).

**Checkpoint**: US2 passes. Build-tier flow correctly sanitizes both identifier slots.

---

## Phase 5: User Story 3 — Manual identifier flags emit verbatim (Priority: P1)

**Goal**: Operator-supplied `--repo`, `--git-ref`, or `--id repo=...` values flow through the SBOM unchanged. No sanitization applied to manual input.

**Independent Test**: `mikebom sbom scan --path /tmp/non-git --repo https://USER:TOKEN@github.com/foo.git --output out.cdx.json` produces an SBOM where the `repo:` value is byte-identical to the operator's input.

### Tests for User Story 3

- [X] T010 [US3] Add to `mikebom-cli/tests/identifiers_credential_strip.rs`:
  - (a) `manual_repo_emits_verbatim_with_credentials` — `--repo https://USER:TOKEN@github.com/foo.git` against a non-git tempdir; assert emitted `repo:` value is byte-identical to the input including the userinfo.
  - (b) `manual_repo_overrides_strip_with_credentials_in_value` — fixture has credentialed origin AND operator passes `--repo` with a different credentialed value; assert the manual value wins (per milestone 074's manual-precedence rule) and emits verbatim. The auto-detected sanitized value does NOT appear; the manual credentialed value DOES appear.

**Checkpoint**: US3 passes. The auto/manual boundary is correctly enforced.

---

## Phase 6: User Story 4 — Opt-out flag preserves credentials (Priority: P2)

**Goal**: When `--keep-credentials-in-identifiers` is passed, auto-detected URLs emit verbatim. Source-tier and build-tier both honor the flag.

**Independent Test**: Same credentialed fixture as US1; with the opt-out flag, assert the `repo:` URL preserves userinfo and the `comment` field has no `(credentials stripped)` suffix.

### Tests for User Story 4

- [X] T011 [US4] Add to `mikebom-cli/tests/identifiers_credential_strip.rs`:
  - (a) `keep_credentials_flag_preserves_userinfo_source_tier` — credentialed fixture + `--keep-credentials-in-identifiers`; assert `repo:` URL contains userinfo verbatim; `comment` field is the unchanged source-tier label (no `(credentials stripped)` suffix).
  - (b) `keep_credentials_flag_preserves_userinfo_build_tier` — same fixture invoked through `auto_detect_build_tier_identifiers(..., keep_credentials=true)`; assert both `repo:` and `git:` values preserve userinfo.
  - (c) `keep_credentials_flag_emits_acknowledgment_log` — assert the FR-007 info log line `--keep-credentials-in-identifiers set; ...` is emitted exactly once per scan invocation.

- [X] T012 [US4] Add edge-case test `parse_failure_falls_through_to_user_defined` (FR-009): an exotic non-RFC-3986 URL value reaches `sanitize_userinfo` (passthrough), then milestone 073's existing `validate_for_scheme` validator soft-fails to `UserDefined`. Assert the resulting `Identifier` has `kind == IdentifierKind::UserDefined`.

**Checkpoint**: US4 passes. All four user stories covered.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T013 [P] Update `/Users/mlieberman/Projects/mikebom/docs/reference/identifiers.md`: add a new subsection (around §4 or wherever auto-detection is currently documented) titled "Credential sanitization" covering: default-strip behavior, the `(credentials stripped)` suffix, the `--keep-credentials-in-identifiers` opt-out flag, and SSH-form passthrough. Add an explicit "What gets stripped vs not" table covering HTTPS-with-userinfo, HTTPS-no-userinfo, SSH-form, and `git://` cases. Reference quickstart.md Recipe 5 for the cross-tier picture.

- [X] T014 Run pre-PR gate per CLAUDE.md: (a) `cargo +stable clippy --workspace --all-targets -- -D warnings` zero warnings; (b) `cargo +stable test --workspace` every suite `0 failed`. Convenience: `./scripts/pre-pr.sh`. A failing per-crate `cargo test -p mikebom` does NOT discharge this requirement.

- [X] T015 Manually validate quickstart.md recipes 1–5 end-to-end against a real local build of milestone 075. Confirm log-line phrasing matches contracts/credential-strip.md exactly. Time Recipe 1 with `time` to verify SC-005 (<1ms sanitization overhead).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 depends on nothing.
- **Phase 2 (Foundational)**: T002 → T003 → T004 → T005 are sequential (same file, building up). T006 [P] and T007 [P] are parallel with each other and parallel-safe with T004/T005 since they touch different files (`scan_cmd.rs` and `run.rs`); however they CALL the new function signatures from T004/T005, so they must land AFTER those land in `auto_detect.rs`. In practice: T002 → T003 → T004 → T005 → (T006 [P] || T007 [P]).
- **Phase 3 (US1)** through **Phase 6 (US4)**: depend on Phase 2 complete. T008 (US1 test file creation) must precede T009 / T010 / T011 / T012 (those add tests to the file T008 created). T009 / T010 / T011 / T012 can run in any order against the existing test file.
- **Phase 7 (Polish)**: depends on Phases 1-6 complete. T013 [P] (docs) parallel with T014 (gate). T015 last.

### User Story Dependencies

All four stories share the production code from Phase 2. The tests are independent test functions in a shared file; merge conflicts limited to the file's `mod tests` import block.

### Parallel Opportunities

- T006 [P] and T007 [P] in Phase 2 — different files (`scan_cmd.rs` vs `run.rs`), no inter-dep.
- T013 [P] in Phase 7 — different file (docs), parallel with T014/T015.

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1)

1. Phase 1 setup (T001).
2. Phase 2 foundational (T002–T007).
3. Phase 3 US1 (T008).
4. **STOP and VALIDATE**: at this checkpoint the source-tier strip path is fully working with tests. The build-tier path is also working (T005 wired it) but untested. This is *not* shippable as MVP because US2 (build-tier) is also P1 — both half-shipped is worse than both finished.
5. Continue through Phases 4-7.

### Incremental Delivery

Single PR. Milestone is small (~14 tasks, all in one file or its tests, no refactors, no new modules). Splitting into multiple PRs would create transient states where the source-tier ships sanitization but build-tier doesn't, and vice versa — same-day asymmetry.

### Parallel Team Strategy

Single developer + reviewer. Total estimated effort: <1 person-day.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- All four user stories share production code; test surface splits by US.
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean. Cite both in the PR description.
- Tests in `identifiers_credential_strip.rs` MUST guard their `mod tests` items with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- Total estimated tasks: 15. Total estimated effort: <1 person-day.
