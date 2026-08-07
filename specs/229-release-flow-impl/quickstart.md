# Quickstart: verifying release-flow implementation

**Feature**: 229-release-flow-impl
**Phase**: 1

Executes each SC-001 through SC-012 predicate against the finished implementation. Two-PR sequence per FR-010.

## Prerequisites

- Both PRs merged (infrastructure PR first, release-bump PR second).
- One cron cycle has passed after release-bump PR merge (so first nightly can be observed).
- `gh` CLI installed locally, authenticated against `kusari-oss/waybill`.
- `cosign` CLI installed locally for signature verification.

## SC-001 — first `v0.2.0` stable shipped

```bash
gh release view v0.2.0 --json isPrerelease,assets --repo kusari-oss/waybill
```

Expected: `isPrerelease: false`; `assets` includes ≥ 4 platform archives (`.tar.gz` × 3 + `.zip` × 1) + `SHA256SUMS` + at least one `.sbom.json.sig` signed SBOM artifact.

## SC-002 — `alpha.N` sequence retired

```bash
grep '^version' Cargo.toml    # expect: version = "0.2.0"
gh release list --repo kusari-oss/waybill --limit 5 --json name  # v0.1.0-alpha.70 present as pre-release; no new alpha.N after
```

## SC-003 — first nightly appears on release page

Wait ~24 hours after release-bump PR merge (or trigger workflow_dispatch on nightly.yml immediately for a same-day test).

```bash
gh release list --repo kusari-oss/waybill --limit 10 --json name,tagName,isPrerelease,createdAt \
  | jq '.[] | select(.tagName | test("nightly")) | {tagName, isPrerelease, createdAt}'
```

Expected: at least one entry matching `v0.2.0-nightly.YYYYMMDD` with `isPrerelease: true`.

## SC-004 — skip-if-unchanged works

Test path: on a day with NO merges to `main`, trigger `workflow_dispatch` on nightly.yml manually. Then inspect the workflow-run log:

```bash
gh run list --workflow nightly.yml --repo kusari-oss/waybill --limit 1 --json databaseId
RUN_ID=<from above>
gh run view $RUN_ID --repo kusari-oss/waybill --log | grep "no new commits since last nightly at "
```

Expected: log line contains the FR-001(d) stable substring.

## SC-005 — ALL release artifacts signed

For each of `v0.2.0` (stable), `v0.2.0-nightly.YYYYMMDD` (nightly), and a bridge tag if one exists:

```bash
TAG=<one of the above>
gh release download $TAG --repo kusari-oss/waybill --pattern '*.sbom.json' --pattern '*.sbom.json.sig' --dir /tmp/verify
cosign verify-blob \
  --certificate-identity-regexp "https://github.com/kusari-oss/waybill/.*" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --signature /tmp/verify/*.sbom.json.sig \
  /tmp/verify/*.sbom.json
```

Expected: `Verified OK` for each of the three tag classes.

## SC-006 — `WAYBILL_VERSION` env-override works

```bash
cd waybill-cli
WAYBILL_VERSION=1.2.3-test cargo build --release
./target/release/waybill --version    # expect: waybill 1.2.3-test
cargo build --release                  # no env-var
./target/release/waybill --version    # expect: waybill 0.2.0 (Cargo.toml-declared)
```

## SC-007 — cache-invalidation avoidance

```bash
cd waybill-cli
cargo build --release                          # first full build; time it (target: ≤ 8 min on fresh runner)
WAYBILL_VERSION=0.2.0-nightly.20260806 cargo build --release  # second build; measure crate-compile count
```

The second build should NOT recompile every crate; only the version-touching call sites should re-emit. Verify via `cargo build --release --timings` output — compilation-graph section should show < N crates re-compiled (where N is the full crate count).

## SC-008 — `auto-tag-release.yml` removed

```bash
test ! -f .github/workflows/auto-tag-release.yml && echo "removed" || echo "STILL PRESENT"
```

Expected: "removed".

## SC-009 — `RELEASING.md` documents the new flow

```bash
test -f RELEASING.md
grep -c "^## " RELEASING.md    # expect: 6 (exactly 6 top-level sections)
grep -l "nightly\|stable\|bridge\|manual tag push\|auto-tag-release.yml (removed)" RELEASING.md
```

## SC-010 — pre-PR gate green on both PRs

For the infrastructure PR:
```bash
./scripts/pre-pr.sh   # expect: >>> all pre-PR checks passed.
```

For the release-bump PR (expected 30+ min per memory `feedback_release_bump_prepr_slow` — may skip locally per RELEASING.md §2 step 6 and let CI verify).

## SC-011 — Sigstore OIDC verified for cron-triggered workflows

Successfully-signed first-nightly artifact per SC-005 IS the verification. If SC-005 fails on the nightly-class tag specifically, #666 gets a fresh comment describing the failure mode.

## SC-012 — nightly retention honored (after ≥ 31 days)

After a month of operation:

```bash
gh release list --repo kusari-oss/waybill --exclude-drafts --limit 100 --json name,createdAt,isPrerelease,tagName \
  | jq '[.[] | select(
      .isPrerelease
      and (.tagName | test("nightly"))
      and (.createdAt | fromdate < (now - 30*86400))
    )] | length'
```

Expected: `0`. Any nightly older than 30 days should have been auto-deleted.

## Sequencing summary

1. Cut infrastructure PR from branch `229-release-flow-impl`.
2. CI green; merge infrastructure PR.
3. Cut release-bump PR from new branch `release/v0.2.0`: bump `Cargo.toml`, regenerate goldens, verify normalized diff.
4. CI green (expect 30+ min); merge release-bump PR.
5. Manually push `v0.2.0` tag per RELEASING.md §2 step 8.
6. Watch release.yml fire; verify SC-001, SC-005 (stable).
7. Wait for first cron fire (or trigger workflow_dispatch on nightly.yml).
8. Verify SC-003, SC-005 (nightly).
9. After a month, verify SC-012.
