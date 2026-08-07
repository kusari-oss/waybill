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

### Step 3 — bump `Cargo.toml`

```bash
git checkout -b release/v<X>.<Y>.<Z>
# edit Cargo.toml: [workspace.package] version = "<X>.<Y>.<Z>"
cargo update    # regenerate Cargo.lock
```

### Step 4 — regenerate all 6 golden test files

Per memory `feedback_release_bump_regen_all_golden_tests` — every
release-bump PR MUST regenerate:

```bash
WAYBILL_UPDATE_CDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX_GOLDENS=1 \
WAYBILL_UPDATE_SPDX3_GOLDENS=1 \
  cargo +stable test --no-fail-fast \
  --test cdx_regression \
  --test spdx_regression \
  --test spdx3_regression \
  --test oci_pull_backward_compat \
  --test optional_dep_classification \
  --test pkg_alias_binding_us1
```

### Step 5 — verify normalized diff (only version-string swap)

Per memory `feedback_verify_golden_churn_normalized`. Mask
content-addressed IDs, sort, compare:

```bash
for f in waybill-cli/tests/fixtures/golden/spdx-3/cargo.spdx3.json \
         waybill-cli/tests/fixtures/golden/cyclonedx/cargo.cdx.json; do
  echo "=== $f ==="
  git show HEAD:"$f" | sed -E 's/doc-[A-Z0-9]{20,32}/doc-XXX/g; s/[A-Z0-9]{16}/HEX16/g' | LC_ALL=C sort > /tmp/before.txt
  cat "$f" | sed -E 's/doc-[A-Z0-9]{20,32}/doc-XXX/g; s/[A-Z0-9]{16}/HEX16/g' | LC_ALL=C sort > /tmp/after.txt
  diff /tmp/before.txt /tmp/after.txt | head -6
done
```

Expected: only `waybill-<old>` → `waybill-<new>` version-string swaps.

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

# Download the SBOM + Sigstore bundle from the release, then verify:
cosign verify-blob \
  --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --bundle waybill-source.cdx.json.bundle \
  waybill-source.cdx.json
# expect: Verified OK
```

---

## 3. Nightly channel — operational notes

### How it works

`.github/workflows/nightly.yml` runs at **06:00 UTC daily** via cron.
Behavior:

1. Checks `main`'s HEAD SHA against the last nightly tag's SHA.
2. If identical → no-op with log line "no new commits since last
   nightly at `<tag>`".
3. If different → tags `v<X>.<Y>.<Z>-nightly.YYYYMMDD` (baseline
   `<X>.<Y>.<Z>` read from `Cargo.toml`) + `gh workflow run release.yml`
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
