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

### Step 1 — decide `main` is stable-worthy, and decide the version

Maintainer judgment. No automated gate. Rule of thumb: recent CI green,
no in-flight destabilizing PRs, changelog contains a coherent set of
features/fixes since the last stable.

**First, check whether the public-corpus goldens are current**, because they
are the one set the bump cannot refresh. If any reader's or emitter's output
changed since the last refresh, refresh them and land that **before** the bump
PR, following `docs/development/refreshing-corpus-goldens.md` — budget an
afternoon, since the procedure needs several lane runs.

Two reasons it cannot wait until the bump. Refreshing them inside the bump PR
makes `check-version-bump.py` fail by design: corpus goldens live under the
fixture tree, so they reach its comparison, and a reader-output change is
exactly the "differs beyond the version" that comparison exists to catch. And a
green local gate says nothing either way — every corpus test is `#[ignore]`d
locally and the PR lane does not run them, so an unrefreshed reader change
surfaces as a red nightly after the release rather than a red check before it.

**Then choose the version. Do not inherit it.** `Cargo.toml` already holds
one: the next *patch*, which `post-release.yml` set as a placeholder after
the previous release (step 10). A placeholder is a default, not a decision.
Read `[Unreleased]` and ask, in this order:

1. **Any new surface?** A subcommand, a flag, an output format. New surface
   is a **minor**, whatever else is in the release.
2. **Anything a consumer can observe change on the wire?** An annotation's
   shape, a relationship type, a document's subject, an identifier that was
   stable and now is not, a schema version. Also a **minor** — *even when
   every one of those items is a bug fix*. SemVer grades observable change,
   not whether the change was justified.
3. **Only fixes no consumer can distinguish from the last release, except
   by the bug being gone?** Then the placeholder patch is correct.

Why it matters concretely: cargo treats `0.Y.Z` and `0.Y.Z+1` as one
compatibility range, so `^0.Y` resolves a patch release without a thought.
Shipping new surface or a wire change as a patch tells every consumer that
nothing changed for them.

Pre-1.0, the minor is the strongest signal available short of `1.0.0`; there
is no separate major to escalate to, which is why step 2 of the list is not
"wait for a major".

Question 1 is enforced, so you cannot skip it by accident:
`scripts/check-release-version.py` refuses a patch whose changelog section has
a non-empty `### Added`, and it runs in every CI lane via
`waybill-cli/tests/release_gates.rs`. Questions 2 and 3 are yours — no check
can read an entry under *Fixed* and tell whether a consumer can observe it. The
gate is a floor, not a verdict.

If the answer is minor or major, steps 3–5 move the version past the
placeholder. `release-bump.sh` exits 1 rather than no-op if handed the
version already set, so a release that *is* a patch needs no bump at all and
its PR is changelog-only.

> **Worked example — v0.11.0 (#1219).** It was first cut as `0.10.1`, the
> placeholder, carrying two new subcommands (`sbom edit`,
> `sbom verify-chain`) and five wire-visible changes, all filed in the
> changelog under *Added* and *Fixed*. The arithmetic was never wrong; this
> step did not exist, so nobody asked whether the placeholder still fit. It
> was caught before the tag only because someone questioned the number.
> Write the consumer-visible changes into a note at the top of the release
> section too — a reader scanning *Fixed* will not expect their pipeline to
> break.

### Step 2 — cut a release-bump PR

Create branch `release/v<X>.<Y>.<Z>`. Title it
`release: bump workspace to v<X>.<Y>.<Z>`, or `release: cut v<X>.<Y>.<Z>`
for a changelog-only patch.

Nothing parses the title. It was mandatory while `auto-tag-release.yml`
matched on it to create the tag; feature 229 deleted that workflow (§6) and
the tag is pushed by hand at step 8, so the format is now a convention for
readers alone.

### Steps 3–5 — bump the version, regenerate goldens, verify the diff

```bash
git checkout -b release/v<X>.<Y>.<Z>
./scripts/release-bump.sh <X>.<Y>.<Z>
```

`scripts/release-bump.sh` does the three things these steps used to describe by hand:

- **Version:** sets `[workspace.package] version` in `Cargo.toml`.
- **Lockfile:** runs `cargo update --workspace`, so only waybill's own crates change in `Cargo.lock`. A plain `cargo update` would upgrade every dependency as a side effect of a version bump.
- **Goldens and check:** regenerates every golden (`scripts/regen-goldens.sh`), then runs `scripts/check-version-bump.py`. That check fails if anything outside `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and test fixtures changed, or if a fixture differs from `HEAD` by anything other than the tool version (`waybill-<v>`, `"version": "<v>"`) and the content-addressed IDs derived from it. It then sweeps the fixture tree for the old version, to catch a golden the regen sweep never reached — a fixture that did not change at all cannot "differ beyond the version", so the comparison alone is silent about that case (#1221). The two forms are checked differently, and the difference is the point: `waybill-<v>` is the tool's own name and version joined, so a textual hit is conclusive, while `"version": "<v>"` is the shape *every* CycloneDX component uses, so a hit there counts only if its object also carries a `waybill` name. Grepping that second form plainly failed two releases running, on real dependencies — `tasty-quickcheck@0.11.1` and `pathspec@0.12.1` — that the sweep had correctly left alone (#1251, fixed in #1257). A fixture the structural check cannot parse is reported, not skipped: the pass exists to find a golden nothing reached, and an unreadable file is that case unexamined. Fixtures listed in its `INPUT_FIXTURES` are exempt from that grep: they are documents a test reads as *input*, nothing regenerates them, and the version in them names the build that emitted them. Do not hand-edit a fixture to satisfy this check — that is how v0.11.0 left three `sbom_edit` documents disagreeing about their own provenance and broke the next automated bump (#1220).

Commit any new scripts or docs **before** running it, since the check treats them as unexpected changes. Then move the CHANGELOG's `[Unreleased]` entries under `## [<X>.<Y>.<Z>] - <date>`.

**If the release is a patch and `Cargo.toml` already holds that version**
(the usual case, since step 10 left it there), there is nothing to bump:
`release-bump.sh` exits 1 rather than no-op, steps 3–5 do not apply, and the
PR is changelog-only. Move the `[Unreleased]` entries and open it. Step 6's
reasoning does not apply either — no version changed, so the compile cache is
intact and the local gate runs at its normal cost.

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
# SHA256SUMS + waybill-source.cdx.json + waybill-source.cdx.json.sig.bundle.json
# (also published as waybill-source.cdx.json.bundle, its pre-v0.10.1 name)

# Download the SBOM + Sigstore bundle from the release, then verify.
# If your cosign install has a stale TUF cache (last used against
# Rekor v1), refresh the trust root first — v0.2.0+ bundles are logged
# to Rekor v2 and need the newer log key:
cosign initialize

# waybill signs this SBOM itself (`sbom scan --sign`); release.yml runs
# the same check before publishing.
cosign verify-blob \
  --certificate-identity "https://github.com/kusari-oss/waybill/.github/workflows/release.yml@refs/tags/v<X>.<Y>.<Z>" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --bundle waybill-source.cdx.json.sig.bundle.json \
  waybill-source.cdx.json
# expect: Verified OK
```

### Step 10 — begin the next development version

This step is automated by `.github/workflows/post-release.yml`. When `release.yml` succeeds for a stable tag push, the workflow runs two jobs.

**`prepare`** (read-only token, never stored on disk):
1. checks that `main` is still at `<X>.<Y>.<Z>`;
2. runs `./scripts/release-bump.sh <X>.<Y>.<Z+1>`;
3. uploads the diff as a patch.

**`land`** (write token, runs no cargo):
1. applies the patch;
2. re-runs `scripts/check-version-bump.py` *from `main`*, not from the patch;
3. pushes `release/begin-v<X>.<Y>.<Z+1>` and dispatches `ci.yml` on it;
4. when CI passes, stops, and its job summary prints a ready `gh pr create` for that branch.

**Then you open, approve and merge that PR**, before the next nightly (06:00 UTC). Opening it runs the normal PR lanes as well.

If any step fails, it files a `[release] post-release version bump failed` issue.

**Why the split.** `release-bump.sh` builds and tests the whole workspace, so every dependency's build script runs in `prepare`. That job never holds a token that can push. The patch it produces reaches `main` only if two things pass:
- the version-only check, which rejects:
  - added or deleted files;
  - paths other than `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and test fixtures;
  - any change other than the workspace's own version;
- CI;
- your review of the PR.

**Why you open the PR.** The workflow cannot:
- Opening a PR with `GITHUB_TOKEN` needs the setting "Allow GitHub Actions to create and approve pull requests". It is locked off above this repository, and turning it on would let every workflow approve PRs.
- `main` requires a PR, so the workflow cannot push to it either. Until this changed, `land` fast-forwarded `main` itself.
- A `GITHUB_TOKEN` push triggers no workflows, which is why `land` dispatches `ci.yml` on the branch: it shows CI passing before you open the PR.
- #1136 tracks a scoped bot identity that could open the PR itself.

**Check the job without pushing.** Dispatch `post-release.yml` with `tag: v<X>.<Y>.<Z>` and `dry_run: true`. It runs `prepare` only, then stops. Dry-run is the default for manual dispatches.

**Manual fallback,** when the workflow is unavailable:

```bash
git checkout -b release/begin-v<X>.<Y>.<Z+1>
./scripts/release-bump.sh <X>.<Y>.<Z+1>
# commit, open PR "chore(release): begin <X>.<Y>.<Z+1> development", merge when green
```

**This step no longer gates the nightly channel.** It used to: the nightly
read its baseline from `Cargo.toml`, so skipping this step left `main` at
the version just released and the next nightly came out as
`v<X>.<Y>.<Z>-nightly.<date>`, which SemVer orders **before** the stable
`v<X>.<Y>.<Z>` — a newer build looking older (#1133). That is not a
hypothetical: it happened on 2026-10-23, when this step was skipped after
v0.12.1 and `v0.12.1-nightly.20261023` published ahead of its own release.
Nothing detected it.

Since #1263 the nightly derives its baseline from the latest stable **tag**
instead (§3), so skipping this step can no longer invert the channel.

What it still does, and why it is worth keeping: it keeps `main`'s version
ahead of every released version, so a build from `main` is distinguishable
from the release it descends from. Without it, a local build and the
shipped artifact both report the same number while having different
contents. That is hygiene rather than correctness — a missed step is now a
confusing `--version`, not a broken release channel.

If the next release turns out to be a minor or major release, its own bump
(Steps 3–5) moves past the patch version — that decision is step 1's, and
this sentence is not a substitute for making it there.

---

## 3. Nightly channel — operational notes

### How it works

`.github/workflows/nightly.yml` runs at **06:00 UTC daily** via cron.
Behavior:

1. Checks `main`'s HEAD SHA against the last nightly tag's SHA.
2. If identical → no-op with log line "no new commits since last
   nightly at `<tag>`".
3. If different → tags `v<X>.<Y>.<Z>-nightly.YYYYMMDD` + `gh workflow run
   release.yml` to build + sign artifacts. The baseline is the **latest
   stable tag's patch plus one**, computed by `scripts/nightly-baseline.sh`
   from `git tag -l 'v*'` — not from `Cargo.toml` (#1263). Only bare
   `vX.Y.Z` tags seed it, so neither a prior nightly nor an `-alpha.`
   bridge pre-release can, and the baseline is always at least one patch
   ahead of anything released. That makes `v<released>-nightly.<date>`
   unmintable rather than merely unlikely; the script also asserts its own
   result outranks the latest stable, and is covered by
   `waybill-cli/tests/release_gates.rs`.
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
