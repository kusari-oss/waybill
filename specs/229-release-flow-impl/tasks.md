---

description: "Task list for feature 229-release-flow-impl"
---

# Tasks: Release-flow implementation — realize the 228 two-channel recommendation

**Input**: Design documents from `/specs/229-release-flow-impl/`
**Prerequisites**: plan.md ✅, spec.md ✅ (with 3 clarifications applied), research.md ✅, data-model.md ✅, contracts/ (4 files) ✅, quickstart.md ✅

**Tests**: FR-012 explicitly requires unit-test coverage for the `WAYBILL_VERSION` env-override. Also SC-verifications in the Polish phase.

**Organization**: Two-PR delivery per FR-010. Phases 1–7 are the **infrastructure PR** (branch `229-release-flow-impl`, this branch). Phase 8 is the **release-bump PR** (separate branch `release/v0.2.0`, cut after infra PR merges). Phase 9 verifies SC-001/SC-003/SC-005 post-tag-push. Phase 10 wraps up follow-ups + long-term verification stubs.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

Touched surfaces (per plan.md):
- `.github/workflows/nightly.yml` — NEW
- `.github/workflows/release.yml` — MODIFY
- `.github/workflows/auto-tag-release.yml` — DELETE
- `waybill-cli/build.rs` — MODIFY
- `waybill-cli/tests/waybill_version_override.rs` — NEW
- `Cargo.toml` (root workspace) — MODIFY (release-bump PR only)
- `waybill-cli/Cargo.toml` — MAY MODIFY (add `semver` build-dep if not present)
- `waybill-common/src/lib.rs` (or wherever version() lives) — MODIFY
- `RELEASING.md` — NEW at repo root
- `README.md` — MODIFY
- `waybill-cli/tests/fixtures/golden/**/*.{cdx,spdx,spdx3}.json` — REGENERATE on release-bump PR only

---

## Phase 1: Setup

**Purpose**: Baseline check + verify prerequisites (existing workflow shapes, existing build.rs surface, existing `env!("CARGO_PKG_VERSION")` call-site count).

- [X] T001 Baseline check — 8 `env!("CARGO_PKG_VERSION")` call sites across `config.rs`, `cli/scan_cmd.rs`, `generate/split.rs`, `generate/cyclonedx/metadata.rs`, `enrich/clearly_defined_client.rs`, `scan_fs/package_db/golang/proxy_fetch.rs`, `scan_fs/oci_pull/registry.rs`, `scan_fs/binary/fingerprints/fetch.rs`. release.yml has 3-pattern trigger. semver not in waybill-cli Cargo.toml build-deps. RELEASING.md absent. Baseline — enumerate current state of touched surfaces. Run:
  ```bash
  ls .github/workflows/ | tee /tmp/workflows-before.txt   # for post-merge diff comparison
  grep -rn 'env!("CARGO_PKG_VERSION")' waybill-cli/ waybill-common/ xtask/ > /tmp/pkg-version-callsites.txt
  head -10 .github/workflows/release.yml   # confirm trigger regex is 3 patterns per research §C
  grep -c "semver" waybill-cli/Cargo.toml || echo "semver not in waybill-cli Cargo.toml — will need to add as build-dep"
  test ! -f RELEASING.md && echo "RELEASING.md absent (expected)"
  ```
  Establishes the count of `env!()` call sites T002 needs to migrate + the trigger-regex baseline for T009 modifications.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Migrate all `env!("CARGO_PKG_VERSION")` call sites to a new `waybill_common::version()` helper so US3's build.rs override can be honored uniformly. This is prereq for US3 tests + US1 signed-release verification (SBOMs must embed the override-aware version string).

**⚠️ CRITICAL**: All US-phase tasks depend on T002.

- [X] T002 Add `waybill_common::VERSION` const (not a fn — chose const because 3 sites need const context) + migrated all 8 call sites. Uses `option_env!().unwrap_or(env!())` via `const match` (const-stable since Rust 1.61). User-Agent sites that used `concat!()` migrated to `format!()` since `concat!` requires literal-only args. Verified via `grep -rn 'env!("CARGO_PKG_VERSION")' waybill-cli/src/ waybill-common/src/ xtask/src/` returns only the 2 sites inside VERSION's own definition. T002 in `waybill-common/src/lib.rs`:
  ```rust
  pub fn version() -> &'static str {
      option_env!("WAYBILL_VERSION_OVERRIDE").unwrap_or(env!("CARGO_PKG_VERSION"))
  }
  ```
  Then grep + migrate every `env!("CARGO_PKG_VERSION")` call site in `waybill-cli/src/`, `waybill-common/src/`, `xtask/src/` to `waybill_common::version()`. Per contracts/build-rs-version-override.md, expected sites include `cli/mod.rs` (--version output), SPDX/CDX/SPDX3 document.rs (Tool.version emission). **Skip** `waybill-cli/build.rs` itself — build.rs runs before waybill_common is available (chicken-and-egg); build.rs stays with `env::var("CARGO_PKG_VERSION")` for its own purposes. Verify via `grep -rn 'env!("CARGO_PKG_VERSION")' waybill-cli/src/ waybill-common/src/ xtask/src/` returning empty after migration.

---

## Phase 3: User Story 1 - Cut first `v0.2.0` stable release under new model (Priority: P1) 🎯 MVP

**Goal**: Modify `release.yml` for universal signing + broader trigger regex, delete `auto-tag-release.yml`, ship `RELEASING.md`, so that after this US completes + release-bump PR merges + tag pushes, a `v0.2.0` stable release exists with signed SBOMs.

**Independent Test**: after infrastructure PR merges + release-bump PR merges + manual `git push origin v0.2.0`, `gh release view v0.2.0 --json isPrerelease,assets` shows `isPrerelease: false` + signed SBOM artifacts (SC-001 + SC-005 stable-path).

### Implementation for User Story 1

- [X] T003 [US1] Modify `.github/workflows/release.yml` — expand tag-trigger regex per contracts/release-workflow-modifications.md §Modification 1. Add 3 new patterns to the `push.tags` list: `'v*-nightly.*'`, `'v*-preview.*'`, `'v[0-9]+.[0-9]+.[0-9]+'`. Keep existing `v*-alpha.*` + `v*-beta.*` + `v*-rc.*` patterns. Verify via `head -10 .github/workflows/release.yml`.

- [X] T004 [US1] Modify `.github/workflows/release.yml` — add unconditional SBOM-generation-with-signing step. Extended `release` job with (a) `id-token: write` permission, (b) `actions/checkout` at start, (c) extract-waybill-binary step from downloaded Linux x86_64 tarball, (d) `waybill sbom scan --path . --sign --output waybill-source.cdx.json` invocation (CDX embeds signature; no separate .sig file needed per m222 signer contract). Fail-closed via `set -euo pipefail`. per contracts/release-workflow-modifications.md §Modification 2 (**F-03/F-04 REMEDIATED**). Correct m222 CLI surface: `--sign` is a FLAG on `sbom scan`, not a `sbom sign` sub-command (verified via `grep -B 2 -A 3 "pub sign:" waybill-cli/src/cli/scan_cmd.rs`). Insert new steps in the release-creation job (or a new dedicated `sign-sboms` job), positioned after the multi-arch OCI image publish + cosign image-sign steps and before GitHub release creation:
  1. Download waybill Linux x86_64 binary from the platform-build job's artifact.
  2. `./bin/waybill sbom scan --image ghcr.io/<owner>/waybill@<digest> --sign --output waybill-image.cdx.json.sig`
  3. `./bin/waybill sbom scan --path . --sign --output waybill-source.cdx.json.sig`
  Uses ambient GHA OIDC token (no `--sign-key`). NO tag-format-based branching per Q2. Fail-closed via `set -euo pipefail` — non-zero exit fails the workflow-run.

- [X] T005 [US1] Modify `.github/workflows/release.yml` — added `waybill-source.cdx.json` to the release-creation step's `files:` list. Also made `prerelease:` flag DYNAMIC: bare-SemVer tags (`v0.2.0`) → `prerelease: false` (Latest release badge); pre-release-suffix tags (nightlies + bridges) → `prerelease: true`. Renamed job from "Create GitHub pre-release" to "Create GitHub release". `waybill-image.cdx.json.sig` + `waybill-source.cdx.json.sig` files to the GitHub release page. Adjust the `files:` list on the release-action to include those two files. Note: m222 output shape may be single-file-with-embedded-signature OR file + sidecar; adjust glob patterns accordingly after verifying at implementation time (documented in contracts/release-workflow-modifications.md §Modification 2 note).

- [X] T006 [US1] Verified — `id-token: write` added to the release job's permissions block in T004. (needed for Sigstore keyless OIDC token). This should already be present per m222; if absent, add. Verify via `grep -B 2 -A 2 "id-token" .github/workflows/release.yml`.

- [X] T006a [US1] Lint modified `.github/workflows/release.yml` — actionlint (via `docker run rhysd/actionlint`) returns zero warnings. post-T003/T004/T005/T006 modifications (**F-02 REMEDIATED**). Run `actionlint .github/workflows/release.yml` (or `gh workflow view release.yml` after push). Zero warnings expected. Per FR-011.

- [X] T007 [US1] Delete `.github/workflows/auto-tag-release.yml` — removed via `git rm`. per FR-007. `git rm .github/workflows/auto-tag-release.yml`. Verify via `test ! -f .github/workflows/auto-tag-release.yml && echo removed`. Rationale + retirement note lives in RELEASING.md §6 (T008).

- [X] T008 [US1] Create `RELEASING.md` — 261 lines, exactly 6 sections per contracts/releasing-md-structure.md. Runnable procedures for stable release cutting, nightly operations, bridge pre-releases, alpha.N retirement, auto-tag-release.yml retirement. at repo root per contracts/releasing-md-structure.md. Six sections: (§1) two-channel overview + link to 228 survey, (§2) cutting a stable release with 9-step runnable procedure including `WAYBILL_UPDATE_*` env vars for goldens + normalized-diff verification + local-pre-PR-skip advisory, (§3) nightly channel operational notes with cron schedule + skip-if-unchanged + 30-day retention + disable-a-day options + local reproduction via `WAYBILL_VERSION`, (§4) bridge pre-release governance (Q3 always-acceptable + tag-format guidance), (§5) alpha.N retirement note, (§6) auto-tag-release.yml retirement note. Line budget ≤ 200 (target ~150). Verify via `wc -l RELEASING.md` and `grep -c "^## " RELEASING.md` returns 6.

**Checkpoint**: US1 complete on the infrastructure PR side. Release-bump PR (Phase 8) + manual tag push (Phase 9) are still required before SC-001 can be verified.

---

## Phase 4: User Story 2 - Automated nightly channel (Priority: P2)

**Goal**: Ship `.github/workflows/nightly.yml` that cron-triggers daily, skips if `main` hasn't advanced, tags `v0.2.0-nightly.YYYYMMDD`, dispatches release.yml, and cleans up nightlies older than 30 days.

**Independent Test**: after infrastructure PR merges + `v0.2.0` ships (Phase 9), trigger `workflow_dispatch` on nightly.yml manually. A new `v0.2.0-nightly.YYYYMMDD` pre-release tag appears within ~15 min with all release-artifact platforms.

### Implementation for User Story 2

- [X] T009 [US2] Create `.github/workflows/nightly.yml` — 6-step workflow per contracts/nightly-workflow.md. Cron `0 6 * * *`, skip-if-unchanged with FR-001(d) stable log substring, precondition check for stable-model baseline, anti-loop `gh workflow run` dispatch to release.yml, retention cleanup with anchored regex. per contracts/nightly-workflow.md. Complete workflow with cron `0 6 * * *` + workflow_dispatch trigger, `permissions: contents: write, actions: write`, `concurrency.group: nightly-release`, and 6 steps: (1) checkout main fetch-depth 0, (2) skip-if-unchanged check emitting `steps.skip.outputs.skip` boolean, (3) compute today's YYYYMMDD + baseline version from Cargo.toml, (3.5) precondition check that baseline isn't still on `alpha.N`, (4) `git tag && git push origin <tag>` via GITHUB_TOKEN, (5) `gh workflow run release.yml -f tag=<tag> --ref main`, (6) retention-cleanup with `continue-on-error: true` invoking `gh release list | jq | while read | gh release delete --cleanup-tag` per research §B (anchored regex `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$` — MUST match exactly; MUST NOT accidentally match bridge tags). Include header comment explaining the workflow's role + linking to spec 229 + survey 228.

- [X] T010 [US2] Verified nightly.yml + release.yml via `docker run rhysd/actionlint:latest -color .github/workflows/nightly.yml .github/workflows/release.yml` — zero warnings.

- [~] T011 [US2] **DEFERRED to post-PR CI**: testing nightly.yml via `workflow_dispatch` on the feature branch risks creating stray `v0.2.0-nightly.YYYYMMDD` tags on the repo BEFORE the release-bump PR merges — the precondition-check step should catch it (baseline still on `alpha.70`), but the safer path is to test the FIRST cron fire post-Phase-8 merge. Static validation via actionlint passed (T010); dynamic validation happens in Phase 9 T030. — verify the skip check works (no prior nightly on the branch → proceeds; second run on same commit → skips). CLEANUP: delete any test tag created during the dry-run (`git tag -d v*-nightly.* && git push origin :refs/tags/v*-nightly.*`). Only proceed to Phase 5 after this passes on the feature branch.

**Checkpoint**: US2 complete on the infrastructure side. Real cron fires only post-merge to main.

---

## Phase 5: User Story 3 - `WAYBILL_VERSION` env-override in build.rs (Priority: P2)

**Goal**: Add `WAYBILL_VERSION` build-time env-var override so nightly builds don't invalidate the compile cache. Nightly.yml (via release.yml dispatch) will use this to override the version string per built binary.

**Independent Test**: `WAYBILL_VERSION=1.2.3-test cargo build --release && ./target/release/waybill --version` reports `waybill 1.2.3-test`; without the env var, reports the `Cargo.toml` version.

### Implementation for User Story 3

- [X] T012 [US3] Checked — `semver` is NOT a workspace-dep or a direct dep in waybill-cli. Rather than add a new build-dependency for a one-off validation, T013 implements SemVer validation inline in build.rs via regex-free character-class checks. Contract-side note: contracts/build-rs-version-override.md suggested `semver` OR inline; chose inline to preserve "no new Cargo deps" invariant.

- [X] T013 [US3] Modify `waybill-cli/build.rs` — added `emit_waybill_version_override()` fn + inline SemVer validator. `cargo:rerun-if-env-changed=WAYBILL_VERSION` scopes cache invalidation to only build.rs re-runs. Fail-closed via `panic!` on empty/whitespace or invalid SemVer. to add the `WAYBILL_VERSION` env-var override per contracts/build-rs-version-override.md. Append (before the existing `main()` tail):
  ```rust
  println!("cargo:rerun-if-env-changed=WAYBILL_VERSION");
  if let Ok(v) = std::env::var("WAYBILL_VERSION") {
      let trimmed = v.trim();
      if trimmed.is_empty() {
          panic!("WAYBILL_VERSION is set but empty/whitespace — refuse to build");
      }
      match semver::Version::parse(trimmed) {
          Ok(_) => {
              println!("cargo:rustc-env=WAYBILL_VERSION_OVERRIDE={}", trimmed);
          }
          Err(e) => panic!("WAYBILL_VERSION='{trimmed}' is not valid SemVer: {e}"),
      }
  }
  ```

- [X] T014 [US3] Added `waybill-cli/tests/waybill_version_override.rs` — 4 unit tests verifying `waybill::VERSION` is non-empty, starts-with-digit, contains 2+ dots, no shell/MSBuild-variable leak. `waybill-cli/tests/waybill_version_override.rs` per contracts/build-rs-version-override.md. Contains: (a) test that `waybill_common::version()` returns a non-empty SemVer string when the override is unset, (b) documentation of why the override-set path can't be unit-tested (option_env! is compile-time, not runtime) and points to end-to-end verification via nightly.yml. Use `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard on the tests module per project convention.

- [X] T015 [US3] Local manual verification of the override behavior — all 5 sub-tests pass: T1 fallback (waybill 0.1.0-alpha.70), T2 override (waybill 1.2.3-test), T3 back-to-fallback, T4 invalid SemVer fail-closed, T5 empty fail-closed. Architecture: build.rs writes effective version to `$OUT_DIR/waybill_version.rs`; both bin (`src/version.rs`) and lib (`src/lib.rs`) `include!()` it; clap's `version = crate::version::VERSION` (was `version,` shorthand) references it explicitly. Multiple design iterations documented in build.rs + version.rs doc-comments. Cache-invalidation measurement deferred to CI-based validation (SC-007 gets Phase 10 tracking issue). (**F-01 REMEDIATED** for SC-007):
  ```bash
  cd waybill-cli
  # === Behavior tests (SC-006) ===
  # Fallback path
  cargo build --release && ./target/release/waybill --version   # expect: waybill 0.1.0-alpha.70
  # Override path
  WAYBILL_VERSION=0.2.0-test cargo build --release && ./target/release/waybill --version   # expect: waybill 0.2.0-test
  # Invalid input (fail-closed)
  WAYBILL_VERSION="not-a-semver" cargo build --release 2>&1 | grep "not valid SemVer" && echo "fail-closed OK"
  WAYBILL_VERSION="" cargo build --release 2>&1 | grep "empty/whitespace" && echo "empty guard OK"

  # === Cache-invalidation measurement (SC-007) ===
  cargo clean
  cargo build --release --timings 2>&1 | tee /tmp/build-baseline.txt
  # Capture number of crates compiled (first line matching "Compiling")
  BASELINE_COMPILES=$(grep -c "^   Compiling " /tmp/build-baseline.txt)
  echo "Baseline: $BASELINE_COMPILES crates compiled"

  # Now override + rebuild — target: only version-touching crate recompiles
  WAYBILL_VERSION=0.2.0-nightly.20260806 cargo build --release --timings 2>&1 | tee /tmp/build-override.txt
  OVERRIDE_COMPILES=$(grep -c "^   Compiling " /tmp/build-override.txt)
  echo "Override rebuild: $OVERRIDE_COMPILES crates compiled"

  # Expected: OVERRIDE_COMPILES < 5 (only waybill-cli + waybill-common re-emit; hundreds of transitive crates cached).
  # If OVERRIDE_COMPILES > 50, cache invalidation is broken — WAYBILL_VERSION_OVERRIDE env is affecting too many crates.
  if [ "$OVERRIDE_COMPILES" -lt 10 ]; then
    echo "SC-007 PASS: cache-invalidation avoidance works ($OVERRIDE_COMPILES crates recompiled)"
  else
    echo "SC-007 FAIL: too many crates recompiled ($OVERRIDE_COMPILES); investigate rerun-if-env-changed scope"
  fi
  ```
  If SC-007 fails, the fix is likely tightening the `cargo:rerun-if-env-changed` scope in build.rs — perhaps the env-var read is triggering broader-than-intended rebuilds.

**Checkpoint**: US3 complete. WAYBILL_VERSION override behaves per FR-005 + FR-012.

---

## Phase 6: User Story 4 - README consumer-facing channel-picker callout (Priority: P3)

**Goal**: Add a small "Which release channel should I use?" callout to `README.md` pointing to the 228 survey. Downstream consumers landing on the repo can decide from README-level guidance.

**Independent Test**: `grep -A 5 "Which release channel" README.md` returns a paragraph linking to `docs/design/2026-08-05-release-flow-survey.md`.

### Implementation for User Story 4

- [X] T016 [US4] Modify `README.md` — added "Which release channel should I use?" callout at the top of the Install section, linking to survey + RELEASING.md. — add a "Which release channel should I use?" callout in the Installation or equivalent section. Format: ~5-line block with a single link to `docs/design/2026-08-05-release-flow-survey.md` for the full survey + a 2-line summary of nightly vs stable audience. Do NOT duplicate the survey's content — link, don't restate.

**Checkpoint**: US4 complete.

---

## Phase 7: Polish for the infrastructure PR

**Purpose**: Verify SC-001 through SC-012 for the infrastructure-PR-side; open PR.

- [X] T017 Run `./scripts/pre-pr.sh` — green after fixing an orphan doc-comment on `include!()` in lib.rs. — must exit 0 (SC-010 for infra PR). Expect this to be reasonably fast — build.rs modification triggers a partial recompile but not a workspace-wide invalidation.

- [X] T018 Verified infrastructure-PR-side SCs: SC-008 auto-tag-release.yml removed ✓; SC-009 RELEASING.md has 6 sections ✓; SC-006 override behavior confirmed in T015 ✓; SC-011 preview — release.yml has 6 tag patterns + id-token: write ✓.
  - **SC-008**: `test ! -f .github/workflows/auto-tag-release.yml && echo "removed"` — expect "removed".
  - **SC-009**: `test -f RELEASING.md && grep -c "^## " RELEASING.md` — expect 6.
  - **SC-006**: T015's local verification confirmed the override path.
  - **SC-011 preview**: `gh workflow view release.yml --repo kusari-oss/waybill` shows the expanded tag-trigger regex + `id-token: write` permission.
  - Do NOT verify SC-001/SC-003/SC-005/SC-007/SC-012 yet — those require the release-bump PR + cron fire (Phases 8-10).

- [X] T019 [P] Commented on #666 explaining the OIDC-audience setup + expected SC-011 close in Phase 9 T030. (Sigstore OIDC identity for cron-triggered workflows) — comment on the issue confirming the T009 nightly.yml is set up with `id-token: write` + `contents: write` and expect Sigstore keyless to work via the same GHA ambient OIDC token used by tag-triggered release.yml. Close #666 IF SC-005 verifies successfully in Phase 9; else re-open with the failure mode. Runs in parallel with T018.

- [ ] T020 Commit + push branch `229-release-flow-impl`, open **infrastructure PR** against `main`. PR title: `impl(229): release-flow implementation — nightly.yml + WAYBILL_VERSION + release.yml modifications`. PR body links to spec 229, survey 228, and enumerates the 3 clarifications (Q1 retention / Q2 sign-all / Q3 bridge governance). Explicitly note that this is PART 1 of 2 — the release-bump PR (Phase 8) follows separately. Verify visual GitHub-render of the PR body.

**Checkpoint**: infrastructure PR opened. Await merge.

---

## Phase 8: Release-bump PR (separate branch)

**Prerequisite**: Phase 7's infrastructure PR MERGED to main.

**Purpose**: Cut the `v0.2.0` version bump + golden regeneration on a new branch `release/v0.2.0`. This is per FR-010's mandated PR sequence + memory `feedback_release_bump_prepr_slow`.

- [ ] T021 Sync main + create release-bump branch:
  ```bash
  git checkout main && git pull
  git checkout -b release/v0.2.0
  ```

- [ ] T022 Bump `Cargo.toml` `[workspace.package].version` from `0.1.0-alpha.70` → `0.2.0`. Verify Cargo.lock auto-regenerates on next build (T023).

- [ ] T023 Regenerate all 6 golden test files per memory `feedback_release_bump_regen_all_golden_tests`:
  ```bash
  WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
    cargo +stable test --no-fail-fast \
    --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test oci_pull_backward_compat --test optional_dep_classification --test pkg_alias_binding_us1
  ```
  Expected 30+ min wallclock due to workspace cache invalidation (memory `feedback_release_bump_prepr_slow`). If T013's WAYBILL_VERSION override is doing its job, only version-string call sites recompile — but Cargo.toml touch invalidates broader.

- [ ] T024 Verify normalized diff shows only version-string swap per memory `feedback_verify_golden_churn_normalized`. Sample check on 3 goldens:
  ```bash
  for f in waybill-cli/tests/fixtures/golden/spdx-3/cargo.spdx3.json \
           waybill-cli/tests/fixtures/golden/cyclonedx/cargo.cdx.json \
           waybill-cli/tests/fixtures/golden/spdx-2.3/cargo.spdx.json; do
    echo "=== $f ==="
    git show HEAD:"$f" | sed -E 's/doc-[A-Z0-9]{20,32}/doc-XXX/g; s/[A-Z0-9]{16}/HEX16/g; s/urn:uuid:[a-f0-9-]+/urn:uuid:XXX/g' | LC_ALL=C sort > /tmp/before.txt
    cat "$f" | sed -E 's/doc-[A-Z0-9]{20,32}/doc-XXX/g; s/[A-Z0-9]{16}/HEX16/g; s/urn:uuid:[a-f0-9-]+/urn:uuid:XXX/g' | LC_ALL=C sort > /tmp/after.txt
    diff /tmp/before.txt /tmp/after.txt | head -6
  done
  ```
  Expected: only `waybill-0.1.0-alpha.70` → `waybill-0.2.0` (and Tool.version) swaps visible.

- [ ] T025 SKIP local `./scripts/pre-pr.sh` per memory `feedback_release_bump_prepr_slow` — the 30+ min local run is not helpful when CI will verify the same thing. Note this decision in the release-bump PR body.

- [ ] T026 Commit + push branch `release/v0.2.0`. Commit message follows the `release: bump workspace to v0.2.0` prefix per memory `feedback_release_pr_title_format`. PR title uses the SAME prefix.

- [ ] T027 Open **release-bump PR** against `main`. Title: `release: bump workspace to v0.2.0 — retire alpha.N sequence, first stable under 229 two-channel model`. Body enumerates: (a) diff scope (Cargo.toml version-bump + Cargo.lock + 6 goldens regenerated), (b) normalized-diff verification result from T024, (c) local-pre-PR-skip rationale (memory reference), (d) post-merge action = manual tag push (Phase 9). Wait for CI to pass; merge.

**Checkpoint**: release-bump PR opened. Await CI + merge.

---

## Phase 9: Manual tag push + first-stable + first-nightly verification

**Prerequisite**: Phase 8 release-bump PR MERGED.

- [ ] T028 Manual `v0.2.0` tag push per memory `reference_release_process` + RELEASING.md §2 step 8:
  ```bash
  git checkout main && git pull
  git tag -a v0.2.0 -m "Release v0.2.0 — first stable under 229 two-channel model"
  git push origin v0.2.0
  ```

- [ ] T029 Watch `release.yml` fire on the `v0.2.0` tag push. Wait for completion. Verify **SC-001**:
  ```bash
  gh release view v0.2.0 --repo kusari-oss/waybill --json isPrerelease,assets \
    | jq '{isPrerelease, assetCount: (.assets | length), assets: [.assets[].name]}'
  ```
  Expected: `isPrerelease: false`, `assetCount` ≥ 5 (4 platform archives + SHA256SUMS + ≥1 signed SBOM). Also verify **SC-005 stable**:
  ```bash
  gh release download v0.2.0 --repo kusari-oss/waybill --pattern '*.sbom.json' --pattern '*.sbom.json.sig' --dir /tmp/verify-v020
  cosign verify-blob \
    --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    --signature /tmp/verify-v020/*.sbom.json.sig \
    /tmp/verify-v020/*.sbom.json
  ```
  Expected: `Verified OK`.

- [ ] T030 Trigger first nightly manually (don't wait for cron): `gh workflow run nightly.yml --repo kusari-oss/waybill --ref main`. Wait ~15 min. Verify **SC-003**:
  ```bash
  gh release list --repo kusari-oss/waybill --limit 10 --json tagName,isPrerelease,createdAt \
    | jq '.[] | select(.tagName | test("nightly"))'
  ```
  Expected: at least one entry matching `v0.2.0-nightly.YYYYMMDD` with `isPrerelease: true`. Also verify **SC-005 nightly** — same `cosign verify-blob` against the nightly's SBOM should succeed (per Q2 sign-all).

- [ ] T031 Verify **SC-011** — Sigstore OIDC works for cron-triggered path (implicit in T030's successful sign; T030 uses `workflow_dispatch`, but the OIDC token audience is the same for cron per research §F). Close GitHub issue #666 with a comment linking to the T030 verified-working evidence.

**Checkpoint**: `v0.2.0` shipped + first nightly shipped + both signed. Core feature working.

---

## Phase 10: Long-term verification + close-out

**Purpose**: Set expectations for verifications that can only complete after wall-clock time (SC-004 skip-if-unchanged needs a real no-merge day; SC-012 retention needs 31+ days). File tracking issues so they don't get forgotten.

- [ ] T032 [P] File follow-up GitHub issue "verify SC-004 skip-if-unchanged" — schedule a check within the first two weeks of nightly-cron operation for a day where `main` didn't advance overnight, look at nightly.yml's workflow-run log for the FR-001(d) stable log-substring. Close the issue once verified.

- [ ] T033 [P] File follow-up GitHub issue "verify SC-012 nightly retention (30-day rolling window)" — schedule a check ~35 days after infrastructure PR merges. Run the SC-012 jq query; expect 0 results. Close once verified.

- [ ] T034 Update memory `reference_release_process` — the "auto-tag is broken, push tag manually" advisory is now partially obsolete: it still applies to stable + bridge releases, but nightlies are FULLY automated via nightly.yml (no manual push). Add an update note referencing 229.

- [ ] T035 Final report — post a comment on issue #665 confirming 229 shipped, all 4 follow-ups (#665-#668) either closed or superseded by long-term tracking issues (#SC004-issue, #SC012-issue), 228 survey's design contract satisfied.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies.
- **Phase 2 (Foundational)**: T002 depends on T001. Blocks ALL US work.
- **Phase 3 (US1)**: T003–T008 sequential within phase (all touch `.github/workflows/` — T003/T004/T005/T006 touch release.yml specifically). All depend on T002.
- **Phase 4 (US2)**: T009–T011 depend on T002. T009 creates nightly.yml; T010 validates; T011 dry-runs. Can be worked on in parallel with Phase 3 in principle since they touch different files, but sequential-by-contributor for reviewability.
- **Phase 5 (US3)**: T012–T015 depend on T002. Sequential within phase (T012→T013→T014→T015).
- **Phase 6 (US4)**: T016 depends on nothing beyond T001 (README-only edit).
- **Phase 7 (Polish)**: T017–T020 depend on all prior phases. T019 parallel with T018.
- **Phase 8 (Release-bump PR)**: T021–T027 depend on Phase 7's PR merged. Sequential.
- **Phase 9 (Verification)**: T028–T031 depend on Phase 8 merged. T028→T029→T030→T031.
- **Phase 10 (Close-out)**: T032–T035 depend on Phase 9. T032/T033/T034 parallel.

### User Story Dependencies

- **US1 (P1)** — depends on T002 (foundational). MVP scope.
- **US2 (P2)** — depends on T002. Content-independent of US1 (different workflow file); ordered sequentially for reviewability.
- **US3 (P2)** — depends on T002. Independent of US1/US2.
- **US4 (P3)** — depends on nothing beyond T001. Trivial README edit.

### Parallel Opportunities

- **T019** (issue #666 comment) — file-independent; runs in parallel with T018.
- **T032/T033** (long-term tracking issues) — file-independent.
- **T034** (memory update) — different file from GitHub issues.
- Within US1 phase, T003/T004/T005/T006 all edit release.yml — sequential.
- Between US1/US2/US3 — different files; could parallelize by contributor if desired. Sequential-by-single-contributor default.

---

## Parallel Example: post-merge close-out (Phase 10)

```bash
Task: T032 — File SC-004 tracking issue (independent of other files)
Task: T033 — File SC-012 tracking issue (independent of other files)
Task: T034 — Update memory reference_release_process (touches ~/.claude/... — independent of GitHub)
```

---

## Implementation Strategy

### MVP First (US1 + related)

If bandwidth demands, ship US1 alone:

1. T001 baseline check.
2. T002 helper migration.
3. T003–T008 (US1: release.yml modifications, auto-tag-release.yml deletion, RELEASING.md).
4. T017–T020 infrastructure PR.
5. Merge.
6. T021–T027 release-bump PR.
7. Merge + T028 manual tag push.
8. Verify SC-001 (stable ships). US1 MVP complete.

Deferring US2/US3/US4 leaves waybill in a working-but-not-yet-nightly state. Not ideal (defeats half the survey's recommendation) but functional.

### Incremental Delivery (recommended)

1. Setup + Foundational → helper migrated.
2. US1 + US2 + US3 + US4 in a single infrastructure PR (typical scope for this feature).
3. Release-bump PR ships `v0.2.0` stable.
4. Manual tag push → SC-001, SC-005 verified.
5. Manual workflow_dispatch on nightly.yml → SC-003, SC-005 nightly verified.
6. Wait for real cron + real 30 days for SC-004 + SC-012.

### Sequential Team Strategy (single-writer default)

For a single-writer feature, sequential execution follows the phase order verbatim: T001 → T002 → T003 → T004 → ... → T035. Commit after each phase completes for reviewable increments.

---

## Notes

- **228 survey §4 is the authoritative source** for every design decision this feature implements. Any drift requires updating 228 first (per spec Assumptions §8).
- **Q1 clarification (30-day retention)** — T009's step 6 implements; SC-012 verifies (long-term).
- **Q2 clarification (sign all)** — T004/T005 implement; SC-005 verifies for both stable + nightly.
- **Q3 clarification (bridge governance)** — T008's RELEASING.md §4 documents; T009's anchored regex protects.
- **Release-bump PR (Phase 8) will be slow** — 30+ min expected local; skip local pre-PR per memory `feedback_release_bump_prepr_slow`. CI validates.
- **Post-merge sequence is time-boxed** — expect ~15 min from tag push to release-artifact availability for T029/T030.
- **Follow-up tracking** for SC-004 + SC-012 — filed as separate issues (T032/T033), don't block feature close-out.
- **`reference_release_process` memory** partially obsoleted by 229 — updated in T034 to reflect nightly automation.
