# Quickstart: Cargo Workspace-Root [package] Runtime Classification

**Date**: 2026-07-16
**Audience**: mikebom maintainer implementing or reviewing m200.

## Prerequisites

- Working mikebom checkout on branch `200-cargo-workspace-root-scope`.
- `cargo +stable` toolchain (existing workspace toolchain).

## Reproducer 1 — Verify US1 fix against the vaultwarden reproducer

```bash
git clone --depth 1 https://github.com/kusari-sandbox/test-vaultwarden /tmp/test-vaultwarden
mikebom --offline sbom scan \
  --path /tmp/test-vaultwarden \
  --format cyclonedx-json \
  --output /tmp/vaultwarden.cdx.json \
  --no-deep-hash
jq '.metadata.component | {name, purl, scope, type}' /tmp/vaultwarden.cdx.json
jq '[.components[] | select(.name == "vaultwarden") | {name, purl, scope}]' /tmp/vaultwarden.cdx.json
```

**Expected (post-m200)**:
- First jq: `{"name": "vaultwarden", "purl": "pkg:cargo/vaultwarden@1.0.0", "scope": null, "type": "application"}`
- Second jq: `[]` (empty — vaultwarden is metadata.component, NOT a components[] entry).

**Pre-m200 baseline** (for comparison):
- First jq: `{"name": "macros", "purl": "pkg:cargo/macros@0.1.0", "scope": null, "type": "application"}`
- Second jq: `[{"name": "vaultwarden", "purl": "pkg:cargo/vaultwarden@1.0.0", "scope": "excluded"}]`

## Reproducer 2 — Verify US1 via the new in-tree fixture

```bash
cargo test -p mikebom --test cargo_workspace_root_lifecycle_m200 2>&1 | tail
```

**Expected**: `test result: ok. 2 passed; 0 failed; ...`

## Reproducer 3 — Verify US2 regression guard on existing cargo tests

```bash
cargo test -p mikebom --test transitive_parity_cargo 2>&1 | tail
cargo test -p mikebom --test scan_cargo 2>&1 | tail 2>/dev/null || true
cargo test -p mikebom --test optional_dep_classification 2>&1 | tail
cargo test -p mikebom --test produces_binaries_cargo 2>&1 | tail 2>/dev/null || true
```

**Expected**: every test result: `ok. N passed; 0 failed`. FR-003 says non-root cargo entries retain their pre-fix classification; existing tests are the regression guard.

## Reproducer 4 — Verify SC-002 explicitly (no components[] entry for the new root)

```bash
# Against the vaultwarden reproducer:
jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:cargo/vaultwarden")))] | length' /tmp/vaultwarden.cdx.json
```

**Expected**: `0` (post-m200). Pre-m200 baseline: `1`.

## Reproducer 5 — Verify SC-003 (fewer excluded components overall)

```bash
# Pre-m200 baseline (checkout main):
git checkout main
mikebom --offline sbom scan --path /tmp/test-vaultwarden --format cyclonedx-json --output /tmp/vaultwarden-pre.cdx.json --no-deep-hash
pre=$(jq '[.components[] | select(.scope == "excluded")] | length' /tmp/vaultwarden-pre.cdx.json)

# Post-m200:
git checkout 200-cargo-workspace-root-scope
mikebom --offline sbom scan --path /tmp/test-vaultwarden --format cyclonedx-json --output /tmp/vaultwarden-post.cdx.json --no-deep-hash
post=$(jq '[.components[] | select(.scope == "excluded")] | length' /tmp/vaultwarden-post.cdx.json)

echo "pre=$pre post=$post delta=$((pre - post))"
```

**Expected**: `delta >= 1` (at minimum, `vaultwarden` no longer contributes to excluded count).

## Reproducer 6 — Verify SC-006 pre-PR wall-clock delta

```bash
git checkout main
time ./scripts/pre-pr.sh 2>&1 | tail -3   # baseline

git checkout 200-cargo-workspace-root-scope
time ./scripts/pre-pr.sh 2>&1 | tail -3   # post-m200
```

Delta MUST be ≤ 5s per SC-006. Expected delta ≈0s (5 LOC production change + 2 new tests).

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
# Post-implementation golden drift check:
git diff --stat mikebom-cli/tests/fixtures/ 2>&1 | tail

# If rust-ripgrep public-corpus golden drifts:
gh workflow run public-corpus.yml --field branch=200-cargo-workspace-root-scope --field regen_goldens=true
```

Any golden drift outside the expected `pkg:cargo/<root-name>` scope change signals unexpected classifier interaction — investigate before committing.
