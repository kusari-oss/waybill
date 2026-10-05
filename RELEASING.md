# Releasing waybill

This document is the release-cutting reference for maintainers with push
access to `kusari-oss/waybill`. Consumers picking a release channel to
integrate against should read
[`docs/design/2026-08-05-release-flow-survey.md`](docs/design/2026-08-05-release-flow-survey.md)
first for the design rationale + channel-picker guidance.

Feature 229 (this document's introducer) is the implementation of the
228 two-channel release-flow recommendation. Q1/Q2/Q3 clarifications
from 229's clarify session codified: **30-day nightly retention**, **all
releases signed** (universal Sigstore keyless), and **bridge pre-releases
always acceptable** (no policy gate).

**Table of contents**:

1. [Two-channel model overview](#1-two-channel-model-overview)
2. [Cutting a stable release](#2-cutting-a-stable-release)
3. [Nightly channel — operational notes](#3-nightly-channel--operational-notes)
4. [Cutting a bridge pre-release](#4-cutting-a-bridge-pre-release)
5. [Retirement of the `alpha.N` sequence](#5-retirement-of-the-alphan-sequence)
6. [Retirement of `auto-tag-release.yml`](#6-retirement-of-auto-tag-releaseyml)

---

## 1. Two-channel model overview

Two release channels + one escape hatch:

| Channel | Cadence | Tag format | Trigger | Signed? |
|---|---|---|---|---|
| **stable** | manual, 1× per 1–4 wk | `v<X>.<Y>.<Z>` (bare SemVer) | maintainer `git push origin <tag>` | YES (Sigstore keyless via m222) |
| **nightly** | 1×/day scheduled, skip-if-unchanged | `v<X>.<Y>.<Z>-nightly.YYYYMMDD` | `.github/workflows/nightly.yml` cron `0 6 * * *` | YES (Sigstore keyless) |
| **bridge** (escape hatch) | ad-hoc, any reason (Q3) | any valid SemVer pre-release (e.g., `v0.2.0-rc.1`, `v0.2.0-preview.20260814`, `v0.1.0-alpha.71`) | maintainer `git push origin <tag>` | YES (Sigstore keyless) |

The full design rationale + comparison with peer OSS projects is in
[`docs/design/2026-08-05-release-flow-survey.md`](docs/design/2026-08-05-release-flow-survey.md) §4.

---

## 2. Cutting a stable release

Full end-to-end procedure. Follow every step in order.

### Step 1 — decide `main` is stable-worthy

Maintainer judgment. No automated gate. Rule of thumb: recent CI green,
no in-flight destabilizing PRs, changelog contains a coherent set of
features/fixes since the last stable.

### Step 2 — cut a release-bump PR

Create branch `release/v<X>.<Y>.<Z>`. PR title MUST start with
`release: bump workspace to v<X>.<Y>.<Z>` (per memory
`feedback_release_pr_title_format`).

### Steps 3–5 — bump the version, regenerate goldens, verify the diff

```bash
git checkout -b release/v<X>.<Y>.<Z>
./scripts/release-bump.sh <X>.<Y>.<Z>
```

`scripts/release-bump.sh` does the three things these steps used to describe by hand:

- **Version:** sets `[workspace.package] version` in `Cargo.toml`.
- **Lockfile:** runs `cargo update --workspace`, so only waybill's own crates change in `Cargo.lock`. A plain `cargo update` would upgrade every dependency as a side effect of a version bump.
- **Goldens and check:** regenerates every golden (`scripts/regen-goldens.sh`), then runs `scripts/check-version-bump.py`. That check fails if anything outside `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and test fixtures changed, or if a fixture differs from `HEAD` by anything other than the tool version (`waybill-<v>`, `"version": "<v>"`) and the content-addressed IDs derived from it.

Commit any new scripts or docs **before** running it, since the check treats them as unexpected changes. Then move the CHANGELOG's `[Unreleased]` entries under `## [<X>.<Y>.<Z>] - <date>`.

### Step 6 — SKIP local pre-PR gate

Per memory `feedback_release_bump_prepr_slow`, a workspace version bump
invalidates the compile cache; local `./scripts/pre-pr.sh` takes 30+
min. Skip locally; let CI validate. Note this decision in the PR body.

### Step 7 — commit + open PR + wait for CI + merge

```bash
git add Cargo.toml Cargo.lock waybill-cli/tests/fixtures/
git commit -m "release: bump workspace to v<X>.<Y>.<Z>"
git push -u origin release/v<X>.<Y>.<Z>
gh pr create --title "release: bump workspace to v<X>.<Y>.<Z>" --body "..."
# wait for CI green; merge to main
```

### Step 8 — manually push the tag

Per memory `reference_release_process`. Manual because `auto-tag-release.yml`
was retired in feature 229 (see §6):

```bash
git checkout main && git pull
git tag -a v<X>.<Y>.<Z> -m "Release v<X>.<Y>.<Z>"
git push origin v<X>.<Y>.<Z>
```

### Step 9 — verify

`release.yml` fires on the tag push. Wait for completion (~10 min):

```bash
gh release view v<X>.<Y>.<Z> --json isPrerelease,assets
# expect: isPrerelease: false; assets includes 4 platform archives +
# SHA256SUMS + waybill-source.cdx.json + waybill-source.cdx.json.bundle

# Download the SBOM + Sigstore bundle from the release, then verify.
# If your cosign install has a stale TUF cache (last used against
# Rekor v1), refresh the trust root first — v0.2.0+ bundles are logged
# to Rekor v2 and need the newer log key:
cosign initialize

cosign verify-blob \
  --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --bundle waybill-source.cdx.json.bundle \
  waybill-source.cdx.json
# expect: Verified OK
```

### Step 10 — begin the next development version

This step is automated. When `release.yml` succeeds for a stable tag push, `.github/workflows/post-release.yml`:
1. checks that `main` is still at `<X>.<Y>.<Z>`;
2. runs `./scripts/release-bump.sh <X>.<Y>.<Z+1>` on `release/begin-v<X>.<Y>.<Z+1>`;
3. opens the PR "chore(release): begin <X>.<Y>.<Z+1> development";
4. dispatches `ci.yml` on that branch;
5. merges the PR when CI passes.

If any of these fails, it files a `[release] post-release version bump failed` issue and leaves any PR open.

**Requirement:** the repository setting *Settings → Actions → General → Workflow permissions → "Allow GitHub Actions to create and approve pull requests"* must be on. `GITHUB_TOKEN` cannot open a PR otherwise.

**Limits:** a PR opened with `GITHUB_TOKEN` triggers no `pull_request` workflows, so `ci.yml` is the only gate it runs; the merge commit triggers no `push` workflows. #1136 tracks replacing the token with a scoped bot identity.

**Check the job without pushing.** Dispatch `post-release.yml` with `tag: v<X>.<Y>.<Z>` and `dry_run: true`. It bumps, regenerates and checks, then stops. Dry-run is the default for manual dispatches.

**Manual fallback,** when the workflow is unavailable:

```bash
git checkout -b release/begin-v<X>.<Y>.<Z+1>
./scripts/release-bump.sh <X>.<Y>.<Z+1>
# commit, open PR "chore(release): begin <X>.<Y>.<Z+1> development", merge when green
```

The nightly version comes from `Cargo.toml` (§3). Without this step, `main` still says the version just released, so the next nightly is `v<X>.<Y>.<Z>-nightly.<date>`. SemVer orders that **before** the stable `v<X>.<Y>.<Z>`, which makes a newer build look older (#1133). If the next release turns out to be a minor or major release, its own bump (Steps 3–5) moves past the patch version.

---

## 3. Nightly channel — operational notes

### How it works

`.github/workflows/nightly.yml` runs at **06:00 UTC daily** via cron.
Behavior:

1. Checks `main`'s HEAD SHA against the last nightly tag's SHA.
2. If identical → no-op with log line "no new commits since last
   nightly at `<tag>`".
3. If different → tags `v<X>.<Y>.<Z>-nightly.YYYYMMDD` (baseline
   `<X>.<Y>.<Z>` read from `Cargo.toml`, with any pre-release suffix
   removed: `0.10.0-alpha.2` gives `v0.10.0-nightly.YYYYMMDD`, so a
   bridge pre-release never pauses the nightly channel) + `gh workflow run release.yml`
   to build + sign artifacts.
4. Cleanup step deletes nightly prereleases + tags older than 30 days
   (Q1 clarification). **Only** nightly tags (regex-anchored) — stables
   and bridge pre-releases are preserved forever.

### How to disable a specific day's nightly

Three options in order of least-invasive to most-invasive:

- **Comment out the cron temporarily** — edit `.github/workflows/nightly.yml`,
  comment out the `schedule.cron` line, commit, then re-enable when
  the concern is resolved.
- **Delete the just-created nightly tag before release.yml completes** —
  `git push --delete origin v<X>.<Y>.<Z>-nightly.YYYYMMDD`. The
  release.yml build may have already finished by the time you notice;
  in that case, additionally run
  `gh release delete v<X>.<Y>.<Z>-nightly.YYYYMMDD --yes`.
- **Force-push a `main` revert** — rare; only for regressions caught
  fast. Reverts the offending commit; the NEXT cron cycle will produce
  a fresh nightly against the reverted state.

### How to reproduce a nightly build locally

Per FR-005, waybill honors a `WAYBILL_VERSION` build-time env override:

```bash
git checkout <the-nightly-commit-SHA>
WAYBILL_VERSION=0.2.0-nightly.20260806 cargo build --release
./target/release/waybill --version    # → waybill 0.2.0-nightly.20260806
```

The override bypasses the `Cargo.toml` version and doesn't invalidate
the full compile cache (only version-string-touching crates recompile).

### Retention

Nightlies older than 30 days are auto-deleted by the cleanup step in
`nightly.yml`. Older-date-pins in downstream CI pipelines will
silently break after the 30-day boundary — this is documented in the
consumer-guide callout at `docs/reference/reading-a-mikebom-sbom.md`
(m227 design-tier docs) + the survey doc.

---

## 4. Cutting a bridge pre-release

Q3 clarification: **always acceptable, no policy gate**. Any reason —
internal-testing, feature-preview, hotfix, CVE, or maintainer whim.

Tag format: valid SemVer with a pre-release suffix. MUST NOT collide
with the nightly regex `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$`.

Common formats:

- `v0.2.0-rc.1` — release-candidate-like
- `v0.2.0-preview.20260814` — feature-preview
- `v0.1.0-alpha.71` — bridge into retiring model (see §5)
- `v0.3.0-xyz.1` — maintainer-picked-suffix (as long as it's valid
  SemVer + not `-nightly.YYYYMMDD`-shaped)

Procedure is identical to §2's stable-release procedure with these
differences:

- The tag has a pre-release suffix.
- Step 3's `Cargo.toml` bump uses the pre-release version string (e.g.,
  `version = "0.2.0-rc.1"`).
- Step 9's `gh release view` shows `isPrerelease: true` (dynamic-flag
  step in release.yml catches non-bare-SemVer tags).
- Signing is still mandatory (Q2 clarification — all releases signed).
- Nightlies continue from the bridge's base version: after a `0.10.0-alpha.2`
  bump they are tagged `v0.10.0-nightly.YYYYMMDD`, which SemVer orders after the
  bridge and before the `v0.10.0` stable.

---

## 5. Retirement of the `alpha.N` sequence

`v0.1.0-alpha.70` (released 2026-08-05) was the LAST alpha release
under the retiring `alpha.N` sequential model.

`v0.2.0` (post-229 first stable) is the FIRST release under the new
two-channel model.

Consumer impact:

- **Pinned to `v0.1.0-alpha.70` explicitly** → unaffected; that tag
  persists forever.
- **Pinned to `latest`** → auto-transitions to `v0.2.0` on next fetch.
- **Pinned to a range like `>=v0.1.0-alpha, <v0.1.0`** → still resolves
  to `alpha.70` (no new alpha.N cut post-`v0.2.0` unless bridge invoked).

Bridge alphas via §4's mechanism are still permitted (Q3 always-acceptable)
but discouraged for non-emergency work post-`v0.2.0`.

---

## 6. Retirement of `auto-tag-release.yml`

Prior to feature 229, `.github/workflows/auto-tag-release.yml`
intended to auto-create tags on release-bump PR merge. It consistently
failed on missing `RELEASE_TAG_TOKEN` secret (see memory
`reference_release_process`).

Feature 229 deletes the workflow. Manual `git push origin <tag>` (§2 step 8)
is now the canonical trigger for stables + bridges. Nightlies are
automated via `.github/workflows/nightly.yml` using `GITHUB_TOKEN` (no
`RELEASE_TAG_TOKEN` dependency).
