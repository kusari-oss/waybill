# Feature Specification: Release-flow implementation — realize the 228 two-channel recommendation

**Feature Branch**: `229-release-flow-impl`
**Created**: 2026-08-06
**Status**: Draft
**Input**: Turn the [2026-08-05 release-flow survey](../228-release-flow-exploration/spec.md)'s two-channel recommendation into concrete workflow YAML + `Cargo.toml` + `build.rs` changes.

## Clarifications

### Session 2026-08-06

- Q: What retention policy governs nightly prereleases on the GitHub release page? → A: Auto-delete nightlies older than 30 days via a companion cleanup step in nightly.yml. Stables kept forever. Recent-week pinning still works; older date-pins are treated as best-effort and may break silently after the 30-day boundary.
- Q: Which release-tag formats trigger the `--sign` (Sigstore keyless) path in release.yml? → A: Sign ALL releases — stables, bridge pre-releases, AND nightlies. Rationale: CISA 2026 Author-Signature mandate (constitution Principle V) applies uniformly to every released SBOM; keyless signing is ~1-2s per artifact via Fulcio with no rate limits at this cadence. **This overrides 228 §4.4's per-channel decision** (which had chosen unsigned nightlies on speculative operational-overhead grounds); the override closes 228 §7's "revisit if a real consumer demands signed nightlies" follow-up in the affirmative — the compliance-consumer IS every SBOM consumer.
- Q: What policy governs when a maintainer may cut a bridge (retiring-model) release? → A: **No policy gate — always acceptable.** Anyone with release-cutting access may cut a bridge pre-release for any reason they consider justified: internal-testing / feature-preview / hotfix / CVE / etc. The bridge mechanism is a governance escape hatch, not a rationed resource. Bridge tags follow SemVer pre-release syntax (e.g., `v0.1.0-alpha.71`, `v0.3.0-preview.20260814`) — the specific tag format is at the release cutter's discretion, subject to (a) not colliding with the nightly regex `v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}`, (b) valid SemVer, (c) not accidentally looking like a stable (must carry a pre-release suffix). All bridge releases go through the same signed release.yml path per Q2. Ship `.github/workflows/nightly.yml` (cron `0 6 * * *`, skip-if-unchanged, tag `v0.2.0-nightly.YYYYMMDD`, `GITHUB_TOKEN` with `contents: write`, unsigned per 228 §4.4). Add `WAYBILL_VERSION` env-override in `build.rs` to avoid golden-fixture cache invalidation on nightly builds. Integrate `--sign` into `release.yml` for stable-channel Sigstore keyless signing. Retire the `alpha.N` sequence and cut `v0.2.0` as the first stable under the new model. Delete `auto-tag-release.yml`. Full survey at [`docs/design/2026-08-05-release-flow-survey.md`](../../docs/design/2026-08-05-release-flow-survey.md); §4 is the authoritative source for the 5 recommendation fields (channel manifest, cadence, tag convention, signing decision, migration path).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Waybill maintainer cuts the first `v0.2.0` stable release under the new model (Priority: P1)

The maintainer has an approved 2-channel recommendation from the 228 survey. They want to cut `v0.2.0` as the first stable release under the new model — with all the associated cleanup (retire `alpha.N`, drop the broken `auto-tag-release.yml`, integrate `--sign` into the release workflow, update `RELEASING.md` to document the new manual tag-push flow). Once this ships, the `alpha.N` era is closed.

**Why this priority**: nothing else in this feature can be validated end-to-end without a real stable release cut under the new model. The nightly channel (US2) matters conceptually but the concrete first shipping event is US1.

**Independent Test**: after merging this feature and following the documented steps, a person unfamiliar with waybill's release history can cut a `v0.2.0` tag by hand, watch `release.yml` produce signed 4-platform binaries + multi-arch OCI image + Sigstore-signed SBOMs, and confirm the release page shows `v0.2.0` as the first stable release with the alpha.N sequence retired.

**Acceptance Scenarios**:

1. **Given** the current `main` is stable-worthy per maintainer judgment, **When** the maintainer runs the release-cut steps documented in the updated `RELEASING.md`, **Then** a `v0.2.0` tag exists in the repo, `release.yml` produced all 4 platform binaries + multi-arch OCI image + Sigstore-signed SBOMs, and the GitHub release page shows `v0.2.0` as a full (non-prerelease) release.
2. **Given** a downstream consumer had pinned `v0.1.0-alpha.70` in their CI pipeline, **When** they read the `v0.2.0` release notes, **Then** they can identify the model transition, understand `alpha.N` is retired, and follow the recommended pin migration to `v0.2.0`.
3. **Given** the maintainer needs to ship an emergency bugfix against `alpha.70` before `v0.2.0` is stable-worthy, **When** they follow the documented "bridge release" path, **Then** they can still cut `v0.1.0-alpha.71` under the retiring model without breaking the new model's assumptions.

---

### User Story 2 - Waybill emits a nightly release automatically every day the code changes (Priority: P2)

Once US1 has cut the first `v0.2.0`, US2 turns on the automated nightly channel. Every morning, a cron-triggered workflow checks whether `main` has advanced since the last nightly. If yes, it builds + tags + pushes `v0.2.0-nightly.YYYYMMDD`. If no, it no-ops. Nightlies are unsigned (per 228 §4.4) but produce the same 4-platform binaries + OCI image as stable, so a consumer pinning `nightly` can `waybill sbom scan` at daily cadence without waiting for a stable cut.

**Why this priority**: the nightly channel is the survey's chief novel contribution. But it can't produce a coherent artifact until US1's `v0.2.0` baseline exists (the nightly tag format `v0.2.0-nightly.YYYYMMDD` presupposes a `v0.2.0` stable line). So US2 is P2 — critical but sequenced.

**Independent Test**: after US2 lands and one day passes with any `main`-merging activity, an operator can visit the releases page and see a fresh `v0.2.0-nightly.YYYYMMDD` prerelease tag. On a day with no merges to `main`, no new nightly appears. Downloading and running the nightly binary produces a working `waybill` build.

**Acceptance Scenarios**:

1. **Given** the nightly.yml workflow is deployed and `main` has advanced since the last nightly, **When** the cron fires at 06:00 UTC, **Then** a new `v0.2.0-nightly.YYYYMMDD` tag appears on the repo + release page within ~10 minutes, and all 4 platform tarballs + OCI image are downloadable.
2. **Given** the nightly.yml workflow is deployed and `main` has NOT advanced since the last nightly, **When** the cron fires, **Then** the workflow exits cleanly without creating a duplicate tag or empty release; the workflow-run log explicitly names the skip reason ("no new commits since last nightly at `<tag>`").
3. **Given** a downstream consumer has pinned their CI pipeline to `waybill nightly` (via a GitHub release-atom-feed filter or `--head` install), **When** a new nightly appears, **Then** their pipeline receives the fresh binary within one workflow-run cycle and produces a working SBOM against their scan target.

---

### User Story 3 - Waybill developers avoid the 30+ min golden-fixture cache-invalidation cost when bumping the version for nightly (Priority: P2)

The 228 survey identified that the current release-bump PR ceremony triggers a 30+ min cache invalidation because changing `[workspace.package].version` in `Cargo.toml` cascades through every content-hash input (memory `feedback_release_bump_prepr_slow`). The nightly channel would suffer this cost EVERY DAY without a mitigation. US3 adds a `WAYBILL_VERSION` env-override read by `build.rs` — the nightly workflow sets this env var to override the version string at build time without touching `Cargo.toml`. Stable releases still bump `Cargo.toml` (once, as before), but daily nightlies don't invalidate the cache.

**Why this priority**: without US3, the nightly channel would be so expensive that operators would either skip it or resent it. US3 is what makes daily nightlies actually feasible. P2 because it's an optimization for US2, not a standalone user outcome.

**Independent Test**: an engineer can, on their local machine, run `WAYBILL_VERSION=0.2.0-nightly.20260806 cargo build --release` from a clean checkout at commit X, then re-run without the env var, and observe the two builds produce artifacts whose SBOMs report `0.2.0-nightly.20260806` vs the `Cargo.toml`-declared version respectively — without the second run triggering a full recompile of unrelated crates.

**Acceptance Scenarios**:

1. **Given** a `build.rs` reading `WAYBILL_VERSION` env var, **When** the nightly.yml workflow sets `WAYBILL_VERSION=0.2.0-nightly.20260806` and runs `cargo build --release`, **Then** the resulting binary's `waybill --version` output is `0.2.0-nightly.20260806` and the compile-cache reuse rate for shared crate compilation is high enough that the total build time is comparable to a non-version-bump build (target: < 8 min on standard GitHub runner).
2. **Given** the same `build.rs`, **When** an operator runs `cargo build --release` locally WITHOUT setting `WAYBILL_VERSION`, **Then** the binary's `--version` reports the `Cargo.toml`-declared version (i.e., the env-var behavior is opt-in, doesn't leak into normal development).
3. **Given** two independent nightly builds of the SAME `main` SHA, **When** both invoke `cargo build --release` with the same `WAYBILL_VERSION`, **Then** they produce byte-identical binaries (reproducibility guarantee per 228 §4.6d + survey §7 risk 3).

---

### User Story 4 - Downstream consumer picks the right channel from the release page (Priority: P3)

Downstream consumers landing on the GitHub releases page or the waybill README need to be able to tell nightly from stable at a glance, understand the tradeoff, and pick correctly. This isn't a new writing task (the 228 survey already produced the docs); it's a light polish pass on the existing release-artifact naming + `README.md` release-flow-mention update after US1 cuts `v0.2.0`.

**Why this priority**: consumer-facing correctness matters but the risk is low — 228 already documented the channel semantics in `docs/design/2026-08-05-release-flow-survey.md` and `docs/reference/reading-a-mikebom-sbom.md`. This story adds README-level cross-links + confirms release-artifact naming distinguishes nightly from stable.

**Independent Test**: a first-time waybill visitor lands on `https://github.com/kusari-oss/waybill/releases`. They see BOTH a `v0.2.0` (marked "Latest") AND a `v0.2.0-nightly.YYYYMMDD` (marked "Pre-release") near the top of the page. From the release-page names alone, they can tell which is stable vs nightly. README also has a "Which release channel should I use?" section pointing to the survey doc.

**Acceptance Scenarios**:

1. **Given** both `v0.2.0` and a fresh `v0.2.0-nightly.YYYYMMDD` are shipped, **When** a first-time visitor loads the releases page, **Then** they see the "Latest" green badge only on `v0.2.0` and the "Pre-release" gray badge on the nightly — matching standard GitHub prerelease semantics.
2. **Given** a new-user reading `README.md`, **When** they scan the "Installation" or equivalent section, **Then** they find a link or brief callout pointing to `docs/design/2026-08-05-release-flow-survey.md` for channel guidance.

---

### Edge Cases

- **First nightly cut before first `v0.2.0` stable**: the nightly.yml workflow expects a `v0.2.0` baseline to exist so the tag format `v0.2.0-nightly.YYYYMMDD` is coherent. If nightly.yml is enabled BEFORE US1 completes, the first nightly would tag against a non-existent stable baseline. Mitigation: sequence US1 → US2 strictly; document in `RELEASING.md`.
- **Cron fires while a stable release is mid-cut**: nightly and stable both push tags via `release.yml` (nightly.yml delegates the artifact-build step to release.yml for consistency). If the cron fires DURING a stable-release build, both workflows compete for the same artifact-build queue. Mitigation: nightly.yml checks whether a release-in-progress marker exists (e.g., a temporary tag or a workflow-run "release-in-progress" concurrency group); no-ops if so.
- **`WAYBILL_VERSION` env var is set during a stable-release build**: unclear which wins. Contract: `WAYBILL_VERSION` overrides `Cargo.toml` ONLY when set. Stable releases must NOT set it. Mitigation: `release.yml`'s tag-triggered path unsets the env var explicitly; document in `RELEASING.md`.
- **A nightly build races against a same-day stable bump**: if the maintainer merges a stable-worthy commit at 05:59 UTC and the cron fires at 06:00 UTC, the nightly captures a snapshot that might duplicate what the maintainer intends to release as stable within the same day. Not harmful (nightly and stable are distinct tags) but worth documenting the semantic.
- **`GITHUB_TOKEN` with `contents: write` fails to push a tag due to branch-protection rules on `main`**: nightly.yml doesn't push to `main`, only creates a tag. If tag creation is somehow gated (rare on GitHub), the workflow must fail loudly rather than silently.
- **Sigstore keyless signing fails during a release** (stable OR nightly, per Q2 clarification): per constitution Principle III (fail-closed) + FR-004, the release MUST fail rather than ship unsigned — no channel-specific escape hatch. For nightlies specifically, a failed sign means today's nightly is skipped; the cron retries tomorrow. Operator can inspect the failed workflow run.
- **`--sign` invocation produces an unexpectedly-large signed SBOM (bundling issue)**: this is an m222/sigstore-rs bug class documented in memory `feedback_sigstore_rs_011_email_limitation`. If US1 hits it during the `v0.2.0` cut, the fix is out of scope for 229 — bounce to a sigstore-rs bump or config change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Add `.github/workflows/nightly.yml` scheduled workflow triggered by cron at `0 6 * * *` (06:00 UTC daily) plus `workflow_dispatch` for manual runs. The workflow MUST: (a) fetch full git history; (b) determine the last nightly tag (glob `v*-nightly.*`); (c) compare current `HEAD` SHA to the SHA the last nightly tag pointed at; (d) if identical, exit success with a log line matching the stable substring "no new commits since last nightly at "; (e) if different, compute today's date-stamp (`YYYYMMDD`), create tag `v0.2.0-nightly.YYYYMMDD` via `git tag && git push origin <tag>`, using `GITHUB_TOKEN` with `contents: write` workflow-permission (no separate `RELEASE_TAG_TOKEN` needed).

- **FR-002**: The nightly.yml workflow MUST delegate the actual multi-arch build + release-page creation to the existing `release.yml` workflow via one of: (a) a `workflow_call` dispatch, (b) a `workflow_run` trigger on the tag-push event, or (c) `release.yml`'s existing tag-triggered path being naturally invoked by the tag push. Approach chosen in Phase-0 research; either way the nightly.yml doesn't duplicate release.yml's build steps.

- **FR-003**: **Every release tag** (stable, bridge alpha, nightly) MUST produce Sigstore-signed SBOMs. Per Q2 clarification, `release.yml` invokes `waybill sbom scan --sign` UNCONDITIONALLY on the release-artifact SBOMs — no tag-format-based branching. This overrides 228 §4.4's per-channel unsigned-nightly decision on CISA-2026-compliance grounds (constitution Principle V). Nightly signing latency is expected to add ~2-4s per release-workflow run (4 platforms × ~1-2s each), which is acceptable given the daily cron cadence.

- **FR-004**: The `--sign` invocation is a hard-required step in `release.yml`. If keyless signing fails at any point during a release (Fulcio unavailable, OIDC identity token mismatch, transparency-log rejection), the release MUST fail-closed (constitution Principle III) — no unsigned fallback, no partial-release semantics. The failed workflow-run is the operator's signal to investigate; the tag remains on the repo but the GitHub release stays in "draft"/incomplete state until re-run succeeds.

- **FR-005**: Add `WAYBILL_VERSION` env-var support in `waybill-cli/build.rs` (or equivalent build-time script). When set, this env var overrides the `Cargo.toml` `[workspace.package].version` string at build time. When unset, `build.rs` falls back to `env!("CARGO_PKG_VERSION")` (current behavior). The override affects only the version string reported by `waybill --version` and embedded in emitted SBOM metadata; it MUST NOT trigger a full compile-cache invalidation.

- **FR-006**: Bump `Cargo.toml` `[workspace.package].version` from `0.1.0-alpha.70` → `0.2.0`. Regenerate all 6 golden test files (per memory `feedback_release_bump_regen_all_golden_tests`): `cdx_regression`, `spdx_regression`, `spdx3_regression`, `oci_pull_backward_compat`, `optional_dep_classification`, `pkg_alias_binding_us1`. Verify normalized diff shows only version-string swap (per memory `feedback_verify_golden_churn_normalized`).

- **FR-007**: Delete `.github/workflows/auto-tag-release.yml` (the broken `RELEASE_TAG_TOKEN` workflow). Document its removal in the release-bump PR body.

- **FR-008**: Update `RELEASING.md` (or create it if absent) documenting the new two-channel manual-plus-cron flow: (a) how to cut a stable release manually — `git tag v<X.Y.Z> && git push origin v<X.Y.Z>`; (b) how the nightly channel operates automatically; (c) how to cut a **bridge pre-release** (per Q3 clarification — always acceptable, no policy gate; tag format at release-cutter's discretion subject to SemVer + non-collision with the nightly regex; historically `-alpha.N`, may also be `-rc.N`, `-preview.YYYYMMDD`, etc.); (d) how to disable a nightly for a given day (comment out the cron, force-push a temporary revert, or delete the last nightly tag). ALL bridge pre-releases go through the same signed release.yml path per FR-003.

- **FR-009**: Update `README.md` (if it has an install-oriented section, else `docs/index.md`) with a brief "Which release channel should I use?" callout pointing to `docs/design/2026-08-05-release-flow-survey.md` §4.

- **FR-010**: The bumping of `Cargo.toml` version + golden regeneration is a SINGLE release-bump PR titled `release: bump workspace to v0.2.0` (matching memory `feedback_release_pr_title_format` prefix). The nightly.yml + build.rs + release.yml changes are a SEPARATE non-release PR that lands BEFORE the release-bump PR. Sequence: infrastructure PR → release-bump PR → manual tag push (per memory `reference_release_process`).

- **FR-011**: All new / modified workflow YAML MUST pass GitHub Actions workflow linter (`gh workflow view` or equivalent) without warnings.

- **FR-011a**: nightly.yml MUST include a cleanup step (per Q1 clarification) that deletes nightly prereleases + their associated tags older than 30 days. Deletion applies ONLY to tags matching the nightly regex `v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}`. Stables (tags without a pre-release suffix) MUST NEVER be auto-deleted by this workflow. **Bridge pre-release tags (any tag with a non-nightly pre-release suffix per Q3 — e.g., `v0.1.0-alpha.*`, `v0.3.0-rc.*`, `v0.3.0-preview.*`) MUST NEVER be auto-deleted.** The cleanup step's inclusion filter is anchored: match must satisfy the exact nightly regex OR the tag is preserved. Cleanup runs in the same nightly.yml workflow invocation as the tag-push step; if cleanup fails, the workflow logs the failure but does NOT fail the run (tag push succeeded is the primary success signal).

- **FR-012**: The `WAYBILL_VERSION` env-var behavior MUST have unit test coverage in `waybill-cli` verifying: (a) override applies when set, (b) fallback to `CARGO_PKG_VERSION` when unset, (c) invalid version strings (non-SemVer, empty, whitespace) produce a build-time compile error rather than a silent bad artifact.

### Key Entities *(include if feature involves data)*

- **Release channel**: `nightly` OR `stable` — the two channels named in 228 §4. Each has its own tag format (`v0.2.0-nightly.YYYYMMDD` vs `v0.2.0`), signing decision (unsigned vs Sigstore keyless), and cadence (cron vs manual).
- **Nightly tag**: a lightweight or annotated git tag matching regex `v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}`. Points at a `main`-tree commit. Referenced by `release.yml` (via delegation) and by consumers.
- **Stable tag**: a git tag matching regex `v[0-9]+\.[0-9]+\.[0-9]+` (no pre-release suffix). Points at a `main`-tree commit. Triggers the signed-release path in `release.yml`.
- **`WAYBILL_VERSION` env var**: build-time override for the version string. Consumed by `build.rs`. Not related to any runtime configuration.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A `v0.2.0` stable release ships from `main` with all 4 platform binaries + multi-arch OCI image + Sigstore-signed SBOMs. The GitHub releases page shows `v0.2.0` as "Latest" (non-prerelease). Verification: `gh release view v0.2.0 --json isPrerelease` returns `false`; `gh release view v0.2.0 --json assets` lists ≥ 4 platform archives + `SHA256SUMS` + at least one signed SBOM artifact.

- **SC-002**: The `alpha.N` sequence is retired. `Cargo.toml` shows `version = "0.2.0"`. The GitHub releases page's latest `alpha.N` release (`v0.1.0-alpha.70`) is preserved but marked pre-release (already is). No new `alpha.N` tags are cut after `v0.2.0` unless the documented bridge-release path is invoked.

- **SC-003**: The nightly.yml workflow runs successfully on its first cron fire after `v0.2.0` ships, producing a `v0.2.0-nightly.YYYYMMDD` prerelease tag and attached artifacts. Verification: after 24 hours of typical `main`-merging activity, at least one nightly tag exists on the release page marked "Pre-release".

- **SC-004**: The nightly.yml workflow correctly skips when `main` hasn't advanced since the last nightly. Verification: after a 24-hour period with no `main` merges (test with a synthetic empty period), the cron fires but no new nightly tag is created; the workflow-run log contains the FR-001(d) stable substring.

- **SC-005**: **ALL release artifacts are Sigstore-signed** (stables + nightlies + bridge alphas alike). Verification: `cosign verify-blob --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" --certificate-oidc-issuer https://token.actions.githubusercontent.com <sbom-path> --signature <sbom-path>.sig` succeeds for a nightly, a stable, and a bridge alpha (if one exists). No `--sign` skip path exists in `release.yml` (per FR-003 override of 228 §4.4).

- **SC-006**: `WAYBILL_VERSION` env-var override works. Verification: `WAYBILL_VERSION=1.2.3-test cargo build && ./target/debug/waybill --version` reports `1.2.3-test`; `cargo build` without the env var reports `0.2.0` (or whatever `Cargo.toml` says).

- **SC-007**: Setting `WAYBILL_VERSION` on an already-built target does NOT trigger a full recompile of unrelated crates. Verification: `cargo build --release` clean + subsequent `WAYBILL_VERSION=0.2.0-nightly.20260806 cargo build --release` completes with only version-touching crate re-compiled (measured by `cargo build --timings` or the run-log's "Compiling ..." line count).

- **SC-008**: `auto-tag-release.yml` is removed from the repo. Verification: `test ! -f .github/workflows/auto-tag-release.yml`.

- **SC-009**: `RELEASING.md` documents the new flow. Verification: file exists; grep matches for "nightly", "stable", "bridge release", "manual tag push", and "auto-tag-release.yml (removed)".

- **SC-010**: Pre-PR gate green on the infrastructure PR AND on the release-bump PR. Verification: `./scripts/pre-pr.sh` exit 0 on both branches.

- **SC-011**: Follow-up issue #666 (Sigstore OIDC identity for cron-triggered workflows) is either closed as verified-working during implementation OR the fix is spec'd separately. Verification: #666 status.

- **SC-012**: Nightly retention policy honored (per Q1 clarification): after ≥ 31 days of nightly.yml operation, the release page shows nightlies dated within the last 30 days only. Any nightly dated > 30 days ago has been auto-deleted along with its tag. Stables and bridge alphas are unaffected. Verification: `gh release list --exclude-drafts --limit 100 --json name,createdAt,isPrerelease | jq '[.[] | select(.isPrerelease and (.name | test("nightly")) and (.createdAt | fromdate < (now - 30*86400)))] | length'` returns 0.

## Assumptions

- **Sequence: infrastructure PR before release-bump PR** (FR-010): the nightly.yml + build.rs + release.yml modifications land as one PR; the version bump to `v0.2.0` + golden regeneration is a separate PR that follows. This avoids conflating "does the new machinery work?" with "does the version bump land correctly?".
- **No pre-`v0.2.0` nightly**: nightly.yml is deployed but the cron doesn't fire meaningfully until `v0.2.0` exists as a baseline. The `v0.2.0-nightly.YYYYMMDD` tag format presupposes a `v0.2.0` stable line.
- **Manual tag push remains the stable-release trigger** (per memory `reference_release_process`): auto-tag-release.yml is deleted, not replaced with a fixed version. Stable releases still require a human `git push origin v<X.Y.Z>`. Nightlies are the ONLY automated tag-pushing path.
- **Existing release.yml stays largely intact**: this feature adds a conditional `--sign` invocation + a conditional nightly-vs-stable branch to release.yml. It does NOT rewrite the multi-arch build pipeline, does NOT change the artifact naming convention (per 228 §6 future-compat invariant), and does NOT change the OCI-image publishing target.
- **Sigstore keyless already works via m222 flow**: the current `release.yml` doesn't invoke `--sign`, but the m222 `waybill sbom scan --sign` CLI path is proven-working. This feature just wires it into the release workflow.
- **`WAYBILL_VERSION` scope**: build-time only, never runtime. The env var is consumed by `build.rs` (which sets a `env::set` for the compilation), NOT read at runtime. Runtime version reporting comes from the compiled-in `env!("CARGO_PKG_VERSION")` (or the override baked in at build time).
- **Golden regeneration cost**: FR-006's `v0.2.0` bump triggers the full 6-file golden regen per memory `feedback_release_bump_regen_all_golden_tests`. This is one-time pain; nightlies avoid it via WAYBILL_VERSION override. The release-bump PR is expected to take 30+ min per memory `feedback_release_bump_prepr_slow` — that's expected.
- **228 survey is the authoritative source**: the 5 recommendation fields in 228 §4 (channel manifest, cadence, tag convention, signing decision, migration path) are the design contract this feature implements. Any implementation drift from those 5 fields requires an update to the survey doc first, not silent divergence.
- **CI test coverage**: FR-012 mandates unit test coverage for the `WAYBILL_VERSION` override. Nightly.yml + release.yml modifications are validated by an end-to-end dry-run on a test branch before merge (per FR-011).
- **Consumer-facing docs already exist**: 228 shipped the operator-facing consumer guide via `docs/design/2026-08-05-release-flow-survey.md`. This feature adds a README-level cross-link (FR-009) but doesn't duplicate content.
- **229 does NOT touch the middle channel (beta/RC)**: 228 §4 deferred beta/RC as a strictly-additive future step. If consumer demand for beta materializes post-229, that becomes a separate spec (`230-add-beta-channel`).
