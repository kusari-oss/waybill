# Contract: `RELEASING.md` structure

**Feature**: 229-release-flow-impl
**Phase**: 1

Pins the required section layout + content contract for the new `RELEASING.md` at the repo root.

## File placement

`/RELEASING.md` — repo root, not `docs/`. Rationale: matches convention for release-cutting docs across the peer projects surveyed in 228 (syft has `RELEASE.md` at repo root; trivy documents in workflow YAML comments; nodejs has `doc/contributing/releases.md` but they're a much larger project). Small-project convention: repo-root `RELEASING.md` is discoverable by maintainers on first `ls`.

## Required section layout

```text
# Releasing waybill

<TOC — 6 sections>

## 1. Two-channel model overview
## 2. Cutting a stable release
## 3. Nightly channel — operational notes
## 4. Cutting a bridge pre-release
## 5. Retirement of the `alpha.N` sequence (transition note)
## 6. Retirement of `auto-tag-release.yml`
```

Line budget: ~150 lines total.

## Per-section content contract

### §1 — Two-channel model overview

Content MUST include:

- One-paragraph summary: 2 channels (nightly + stable), plus bridge escape hatch.
- Link to `docs/design/2026-08-05-release-flow-survey.md` for the full survey.
- Link to `docs/design/2026-08-05-release-flow-survey.md#4-recommendation` for the specific channel manifest.
- Table row summary: channel | cadence | tag format | signed?

Content MUST NOT:

- Duplicate the survey's design rationale — link, don't restate.

### §2 — Cutting a stable release

Content MUST be a runnable procedure. Step order:

1. Ensure `main` is stable-worthy (maintainer judgment; no automated gate).
2. Cut a release-bump PR titled `release: bump workspace to v<X.Y.Z>` per memory `feedback_release_pr_title_format`.
3. In the PR, bump `Cargo.toml` `[workspace.package].version` from current to new value.
4. Regenerate ALL 6 golden test files per memory `feedback_release_bump_regen_all_golden_tests`:

   ```bash
   WAYBILL_UPDATE_CDX_GOLDENS=1 \
   WAYBILL_UPDATE_SPDX_GOLDENS=1 \
   WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
     cargo +stable test --no-fail-fast \
     --test cdx_regression --test spdx_regression --test spdx3_regression \
     --test oci_pull_backward_compat --test optional_dep_classification \
     --test pkg_alias_binding_us1
   ```

5. Verify normalized diff shows only version-string swap per memory `feedback_verify_golden_churn_normalized`.
6. Pre-PR gate: **SKIP local pre-PR** per memory `feedback_release_bump_prepr_slow` — version bump invalidates compile cache; local run takes 30+ min. Let CI verify.
7. Merge the release-bump PR.
8. Post-merge: manually push the tag: `git checkout main && git pull && git tag v<X.Y.Z> && git push origin v<X.Y.Z>`.
9. Verify release.yml fires + all 4 platform artifacts + multi-arch OCI image + signed SBOMs appear on the release page.

Content MUST NOT:

- Recommend the retired `auto-tag-release.yml` — that workflow is DELETED (see §6).
- Recommend skipping the golden regen — memory shows that fires 11+ macOS test panics.

### §3 — Nightly channel — operational notes

Content MUST include:

- Cron schedule (`0 6 * * *` UTC) and workflow-file location (`.github/workflows/nightly.yml`).
- Skip-if-unchanged behavior — no-op on days without `main` merges.
- 30-day retention — nightlies auto-deleted after 30 days per Q1 clarification + FR-011a.
- How to disable a specific day's nightly: three options — (a) temporarily comment out the cron, (b) delete the just-created nightly tag before release.yml completes, (c) force-push a `main` revert (rare; only for regressions caught fast).
- How to reproduce a nightly build locally: `WAYBILL_VERSION=0.2.0-nightly.YYYYMMDD cargo build --release` (per FR-005 + build-rs-version-override contract).

Content MUST NOT:

- Encourage manual nightly cuts (violates the cron-triggered contract; if a manual nightly is truly needed, the workflow_dispatch escape hatch on nightly.yml handles it).

### §4 — Cutting a bridge pre-release

Content MUST include:

- Q3 governance: "always acceptable, no policy gate".
- Tag-format guidance: MUST be valid SemVer with a pre-release suffix; MUST NOT collide with nightly regex `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$`.
- Example tag formats: `v0.2.0-rc.1`, `v0.2.0-preview.20260814`, `v0.1.0-alpha.71` (bridge into retiring model).
- Procedure: identical to §2's stable-release procedure except the tag has a pre-release suffix (release.yml treats all pre-release tags the same as stable per FR-003 unconditional sign).
- Signing invariant: every bridge pre-release is Sigstore-signed per Q2 + FR-003.

Content MUST NOT:

- Restrict the reasons for bridge releases (Q3 explicitly allows any reason).

### §5 — Retirement of the `alpha.N` sequence (transition note)

Content MUST include:

- One-time note that `v0.1.0-alpha.70` is the LAST alpha release under the retiring model; `v0.2.0` is the FIRST stable under the new model.
- Consumers who pinned `v0.1.0-alpha.70` explicitly are unaffected (that tag persists forever).
- Consumers who pinned `latest` transition to `v0.2.0` automatically on next fetch.
- Bridge alphas via §4's mechanism are still permitted (Q3 always-acceptable) but discouraged for non-emergency work post-`v0.2.0`.

### §6 — Retirement of `auto-tag-release.yml`

Content MUST be a brief historical note:

- Prior to feature 229, `auto-tag-release.yml` was intended to auto-create tags on release-bump PR merge; it consistently failed on missing `RELEASE_TAG_TOKEN` secret.
- Feature 229 deletes the workflow.
- Manual `git push origin <tag>` is now the canonical trigger for stables + bridges.
- Nightlies are automated via `nightly.yml` using `GITHUB_TOKEN` (no `RELEASE_TAG_TOKEN` dependency).

## Cross-reference invariant

- Every section that mentions a workflow file MUST link to the actual file (relative path from repo root).
- Every section that mentions a memory (e.g., `feedback_release_pr_title_format`) is fine to name the memory — memories are project-artifacts, not consumer-facing.

## Validation

- `wc -l RELEASING.md` ≤ 200 (some headroom over the 150-line target).
- `grep -c "^## " RELEASING.md` = 6 (exactly 6 top-level sections).
- Every workflow-file reference resolves to a real file at merge time (T-task in tasks.md verifies).
