# Quickstart: Verify milestone-132 SC closure against the pinned audit image

**Date**: 2026-06-19
**Branch**: `132-sc-closeout`
**Audience**: any implementer claiming a milestone-132 SC closed; CI re-verification; code reviewers

This document is the end-to-end protocol for re-measuring sbom-comparison scorecard
movement on the pinned audit image. It exists to make SC-001 / SC-002 / SC-003 / SC-004
reproducible across operators, machines, and future re-checks. Every shell command is
verbatim runnable.

## Step 0: Pin the audit image (one-time per milestone)

If `<DIGEST>` in `research.md §Audit Baseline` is still a placeholder, resolve it before
continuing:

```sh
aws sso login
DIGEST=$(aws ecr describe-images \
  --region us-east-1 \
  --repository-name remediation-planner \
  --image-ids imageTag=latest \
  --query 'imageDetails[0].imageDigest' \
  --output text)
echo "$DIGEST"   # expect: sha256:<64 hex chars>
```

Back-substitute the resulting digest into:

- `specs/132-sc-closeout/research.md §Audit Baseline` (replace `<DIGEST>` with the
  actual hex)
- `specs/132-sc-closeout/spec.md §Assumptions` (same)
- `specs/132-sc-closeout/spec.md §Dependencies` (same)

Commit those edits as the FIRST commit of any PR claiming an SC.

## Step 1: Generate baseline SBOMs against the pinned digest

```sh
IMAGE="767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@${DIGEST}"
docker pull "$IMAGE"

# mikebom (offline mode — current Path A only)
./target/release/mikebom sbom scan \
  --image "$IMAGE" \
  --output /tmp/mb-rp-132-offline.cdx.json \
  --root-name 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner \
  --offline

# mikebom (online mode — Path C deps.dev enrichment ON; this is the DEFAULT,
# omitting --offline is sufficient. --no-deps-dev / --enrich-sources / --offline
# are the opt-out controls.)
./target/release/mikebom sbom scan \
  --image "$IMAGE" \
  --output /tmp/mb-rp-132-online.cdx.json \
  --root-name 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner

# syft (against the SAME pinned digest — do NOT reuse the cached
# ~/Downloads/remediation-planner-syft-image-sbom.json which is stale `:latest`)
syft scan "registry:${IMAGE}" -o cyclonedx-json=/tmp/syft-rp-132-baseline.cdx.json
```

## Step 2: Run the sbom-comparison scorecard

```sh
SBOMCMP="/Users/mlieberman/Projects/sbom-comparison/sbom-comparison"

# Online-mode mikebom vs syft (the canonical SC-001/003/004 measurement)
"$SBOMCMP" \
  --a /tmp/mb-rp-132-online.cdx.json --aLabel mikebom \
  --b /tmp/syft-rp-132-baseline.cdx.json --bLabel syft \
  --format json > /tmp/mb-rp-132-online.scorecard.json

# Offline-mode mikebom vs syft (sanity check — confirms Path A complement works)
"$SBOMCMP" \
  --a /tmp/mb-rp-132-offline.cdx.json --aLabel mikebom \
  --b /tmp/syft-rp-132-baseline.cdx.json --bLabel syft \
  --format json > /tmp/mb-rp-132-offline.scorecard.json
```

## Step 3: Per-SC verification

### SC-001 — weighted score exceeds syft by ≥ 0.4

```sh
jq '.overall.scoreA - .overall.scoreB' /tmp/mb-rp-132-online.scorecard.json
# Expect: >= 0.4
```

### SC-002 — VERSION_MISMATCH count < 50

```sh
jq '.versions.mismatch' /tmp/mb-rp-132-online.scorecard.json
# Expect: < 50
```

### SC-003 — License Coverage ≥ 3 stars

```sh
jq '.licenses.starsA' /tmp/mb-rp-132-online.scorecard.json
# Expect: >= 3

jq '.licenses.effectiveRateA' /tmp/mb-rp-132-online.scorecard.json
# Expect: >= 60.0 (per the coverageStarsPct formula in research.md §SC-003 Threshold)
```

### SC-004 — Supplier Attribution ≥ 3 stars

```sh
jq '.suppliers.starsA' /tmp/mb-rp-132-online.scorecard.json
# Expect: >= 3
```

### SC-005 — Byte-identity goldens preserved (except expected churn)

```sh
cargo +stable test --workspace --test golden_byte_identity
# Expect: every test passes EXCEPT those marked with the "milestone-132 expected churn"
# annotation in the test source (the FR-003 enumerated fixtures gaining supplier.name).
```

### SC-006 — Scan-time growth < 30 %

```sh
# Compare against the milestone-131 baseline timing recorded in
# specs/131-quality-metadata-backfill/quickstart.md (if absent, re-measure milestone 131
# main first):
hyperfine --warmup 1 \
  "./target/release/mikebom sbom scan --image '$IMAGE' --output /tmp/discard.cdx.json --root-name foo" \
  --export-json /tmp/mb-rp-132.timing.json

# Compare median scan time to the 131 baseline. The growth percentage MUST be < 30 %.
```

### SC-007 — Milestone-131 spec retrospectively edited

```sh
grep -c '\*\*Status (2026-06-19)\*\*' specs/131-quality-metadata-backfill/spec.md
# Expect: 4   (one per SC-001..SC-004)

grep -c '## Post-Milestone Outcomes (2026-06-19)' specs/131-quality-metadata-backfill/spec.md
# Expect: 1
```

## Step 4: Pre-PR gate (mandatory per CLAUDE.md)

Before opening any PR that claims a milestone-132 SC:

```sh
./scripts/pre-pr.sh
# Equivalent to:
#   cargo +stable clippy --workspace --all-targets   (zero errors)
#   cargo +stable test --workspace                    (every suite N passed; 0 failed)
```

Per the standing user feedback ("Pre-PR gate: full output, don't grep") — the PR
description MUST quote the per-target `N passed; 0 failed` lines, NOT a failure-grep
result.

## Step 5: Honest PR description template

```markdown
## Milestone 132 closeout — what shipped

| SC | Target | Measured (pinned digest sha256:<DIGEST>) | Status |
|----|--------|------------------------------------------|--------|
| SC-001 | syft + 0.4 | <fill from jq> | <MET / NOT MET> |
| SC-002 | VERSION_MISMATCH < 50 | <fill from jq> | <MET / NOT MET> |
| SC-003 | License Coverage ≥ 3★ | <fill from jq> | <MET / NOT MET> |
| SC-004 | Supplier Attribution ≥ 3★ | <fill from jq> | <MET / NOT MET> |
| SC-005 | Byte-identity goldens | <test count> | <MET / NOT MET> |
| SC-006 | Scan-time growth < 30 % | <fill from hyperfine> | <MET / NOT MET> |
| SC-007 | Milestone-131 spec amended | <grep counts> | <MET / NOT MET> |

Pinned digest: sha256:<DIGEST>
Pre-PR gate: <paste the two `N passed; 0 failed` lines verbatim>
```

A PR description that does NOT include the measured-vs-target table MUST NOT cite SC
closure. The maintainer's standing feedback is explicit: "you have a habit of making
random assumptions here" — the table grounds the closure claim in numbers reviewers can
re-derive.
