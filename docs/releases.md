# Releases

## Overview

waybill follows semantic versioning with an `-alpha.N` suffix during pre-1.0 development. Release cadence is 2-3 per week, driven by maintainer discretion — typically bundling a batch of milestones (m165-171 style) that have merged since the last release.

Publishing a release involves two steps:
1. Merge a "release: bump workspace to v..." PR.
2. GitHub Actions `auto-tag-release.yml` fires on merge, extracts the new version from `Cargo.toml`, and pushes an annotated `v0.1.0-alpha.<N>` tag to `origin`.
3. The tag push triggers `release.yml`, which builds Linux/macOS/Windows binaries + Docker images and publishes the GitHub release.

If any of these steps break, refer to the § Failure playbook below.

## The release PR

### Title format

The PR title MUST start with `release: bump workspace to v` — for example, `release: bump workspace to v0.1.0-alpha.54`. This exact prefix is what `auto-tag-release.yml`'s `if:` guard matches (see the workflow's `startsWith(github.event.pull_request.title, ...)` check).

If the title uses any other format (`release: v0.1.0-alpha.<N>`, `chore: bump version`, etc.), the auto-tag workflow silently skips the PR and the release doesn't tag automatically. Recovery is a manual `git tag && git push` — see § Failure playbook.

**Origin of this convention**: `auto-tag-release.yml`'s title-match gate was tightened during milestone 171 (closes #519) after the m053/m054 releases exposed the title-format ambiguity.

### Version bump

Edit exactly ONE line in `Cargo.toml`:

```diff
 [workspace.package]
-version = "0.1.0-alpha.53"
+version = "0.1.0-alpha.54"
```

Then run `cargo update -p waybill -p waybill-common` to refresh `Cargo.lock` with the new version. Commit both together.

### Golden regen (mandatory)

The workspace version is embedded in every SBOM emitted by waybill:

- **CDX 1.6**: `metadata.tools[0].version` — 2-line change per golden.
- **SPDX 2.3**: `annotations[].annotator = "Tool: waybill-0.1.0-alpha.<N>"` — dozens of lines per golden.
- **SPDX 3.0.1**: same annotator pattern + content-hashed SPDXIDs and `documentNamespace` cascade — hundreds of lines per golden.

A release PR MUST regenerate all 33 goldens (11 per format × 3 formats):

```sh
grep -rlE "0\.1\.0-alpha\.<PREV>" waybill-cli/tests/fixtures/    # sweep-detect
WAYBILL_UPDATE_CDX_GOLDENS=1   cargo +stable test -p waybill --test cdx_regression
WAYBILL_UPDATE_SPDX_GOLDENS=1  cargo +stable test -p waybill --test spdx_regression
WAYBILL_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p waybill --test spdx3_regression
grep -rlE "0\.1\.0-alpha\.<PREV>" waybill-cli/tests/fixtures/ | grep -v ".actual.json"   # re-sweep, MUST return empty
```

Any stray fixture that embeds the version stamp outside the standard `golden/` tree (e.g., `tests/fixtures/pkg_alias_binding/image-baz.cdx.json`) MUST also be regenerated per the sweep. Skipping this fires ~11 macOS-lane `cdx_regression` panics — the fail-diff (`- "version": "0.1.0-alpha.<PREV>"` vs `+ "version": "0.1.0-alpha.<NEW>"`) is the smoking gun.

### Local pre-PR: skip

The version bump invalidates the whole compile cache. Local `./scripts/pre-pr.sh` takes 30+ minutes on a release PR (vs 3-5 min normally). Skip the local gate and let CI verify — this trade-off is fine because the release PR's changes are (a) small, (b) uniform (goldens are all version-string substitutions), (c) low-risk (no code paths change).

## The auto-tag mechanism

### Happy path

Post-milestone-171, `auto-tag-release.yml` uses a fine-grained personal access token stored as repo secret `RELEASE_TAG_TOKEN` (not the default `GITHUB_TOKEN` which is silently narrowed to read-only by the org's workflow-permissions policy).

Sequence when a release PR merges:

1. Workflow's `if:` guard matches on the PR title.
2. `actions/checkout` step checks out the merge commit using `RELEASE_TAG_TOKEN`.
3. `Extract version + create + push tag` step:
   - Guards on empty `GH_TOKEN` (emits `::error::` if the secret is missing).
   - Parses the new version from `Cargo.toml`.
   - Idempotency check: if the tag already exists on origin, skips.
   - Creates an annotated tag pointing at the merge commit.
   - Pushes to origin via `https://x-access-token:${GH_TOKEN}@github.com/kusari-oss/waybill.git`.
4. `release.yml` fires on the `push: tags: 'v*-alpha.*'` trigger.

### Secret ownership + rotation

**`RELEASE_TAG_TOKEN` is a fine-grained PAT** scoped to `contents: write` on `kusari-oss/waybill` ONLY. No other repos, no other permissions.

**Current owner** (as of milestone 171 rollout, 2026-07-07): the token was provisioned as an interim on Michael Lieberman's (`mlieberman85`) personal account. A follow-up milestone will migrate ownership to a proper `kusari-oss-bot` service account once one is provisioned.

**Rotation cadence**: annual, before GitHub's 365-day fine-grained PAT max lifetime. Set a calendar reminder for 10 months out (2026-05-07 for the current provisioning). Runbook: `specs/171-fix-auto-tag-perms/contracts/secret-provisioning.md`.

**Emergency revocation** (compromise / accidental exposure):
1. https://github.com/settings/tokens → find the token → Revoke immediately.
2. Follow the § Failure playbook manual-recovery path until a new PAT is provisioned.
3. Post-incident, audit `gh api /user/tokens/xxx/log` (or the org security log) for any writes made by the compromised PAT.

## Failure playbook

### Detecting a failed release

The auto-tag workflow's failure surfaces on the merged PR page as a red status check (verified empirically in milestone 171 T015 against PR #518). No inbox notification is sent — releases are infrequent enough that a maintainer merging a release PR is expected to be present at merge time and can react to the red X immediately.

Commands to diagnose:

```sh
# Was the tag pushed?
git ls-remote --tags origin | grep v0.1.0-alpha.<N>

# If not, what did auto-tag-release.yml say?
gh run list --repo kusari-oss/waybill --workflow auto-tag-release.yml --limit 5

# View the failing log:
gh run view <run-id> --repo kusari-oss/waybill --log-failed
```

### Manual recovery

If auto-tag fails for any reason (missing PAT, expired PAT, network error, permission narrowing regression), the release still needs to publish. Manual playbook:

```sh
# Substitute <MERGE_SHA> with the release PR's merge commit and <N> with the new version.
git tag -a v0.1.0-alpha.<N> -m 'Release v0.1.0-alpha.<N>' <MERGE_SHA>
git push origin v0.1.0-alpha.<N>
```

The tag push fires `release.yml`, which builds and publishes as if the auto-tag had worked. From a downstream consumer's perspective, the release is indistinguishable — same tag, same commit, same artifacts.

Post-recovery: file a follow-up issue if the auto-tag failure indicates a structural problem (org-permission change, PAT rotation missed, workflow YAML regression) so the next release doesn't hit the same blocker.

### When to rotate the PAT

- **Scheduled**: 10 months after last provisioning (calendar reminder).
- **On-suspicion-of-compromise**: immediately after the compromise is suspected.
- **After extended absence**: if the account owning the PAT has been dormant > 6 months, consider rotating proactively.
- **On ownership transfer**: when migrating from Michael's account to a service account (planned follow-up milestone), rotate as part of the migration.

### Retroactive audit

Milestone 171's rollout included a retroactive audit of `auto-tag-release.yml` — see `specs/171-fix-auto-tag-perms/audit.md` for the historical run classification (which prior releases succeeded via workflow, which required manual recovery, and why).

## Reference

- Milestone 171 spec + plan + tasks: `specs/171-fix-auto-tag-perms/`.
- Issue #519: original bug report + fix-mechanism analysis.
- Memory `feedback_release_pr_title_format` — the title-format convention.
- Memory `feedback_release_bump_regen_goldens` — the golden-regen sweep.
- Memory `feedback_release_bump_prepr_slow` — why local pre-PR is skipped.
