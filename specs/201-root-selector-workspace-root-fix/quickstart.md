# Quickstart: Root-Selector Workspace-Root Disambiguation

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m201.

## Prerequisites

- Working mikebom checkout on branch `201-root-selector-workspace-root-fix`.
- `cargo +stable` toolchain (existing workspace toolchain).
- m200 landed on `main` (checked via `git log --oneline main -- mikebom-cli/src/scan_fs/package_db/cargo.rs | grep 200` — should show the m200 commit).

## Reproducer 1 — Verify SC-001/SC-002/SC-003 against test-vaultwarden

```bash
git clone --depth 1 https://github.com/kusari-sandbox/test-vaultwarden /tmp/test-vaultwarden 2>/dev/null || true
mikebom --offline sbom scan \
  --path /tmp/test-vaultwarden \
  --format cyclonedx-json \
  --output /tmp/vaultwarden-post-m201.cdx.json \
  --no-deep-hash 2>&1 | grep -E 'root-component|scan complete'
```

**Expected post-m201 scan-log line**:
```
root-component selected via heuristic ... selected=pkg:cargo/vaultwarden@1.0.0
  losers=["pkg:cargo/macros@0.1.0", "pkg:npm/scenarios@1.0.0"]
  heuristic="repo-root" confidence=0.90
```

**Pre-m201 baseline (post-m200)** — for comparison:
```
selected=pkg:cargo/macros@0.1.0
  losers=["pkg:cargo/vaultwarden@1.0.0", "pkg:npm/scenarios@1.0.0"]
  heuristic="ecosystem-priority" confidence=0.7
```

## Reproducer 2 — Verify against the extended m200 fixture

```bash
cargo test -p mikebom --test cargo_workspace_root_lifecycle_m200 -- --nocapture 2>&1 | tail
```

**Expected**: `test result: ok. 3 passed; 0 failed; ...` (was 2 tests pre-m201; m201 adds `scan_cargo_workspace_root_wins_multi_ecosystem_m201`).

## Reproducer 3 — Verify FR-004 regression guard on existing tests

```bash
# Every cargo integration test should pass byte-identically.
for t in transitive_parity_cargo optional_dep_classification produces_binaries_cargo scan_cargo; do
  cargo test --manifest-path mikebom-cli/Cargo.toml --test $t 2>&1 | tail -3 || echo "FAILED: $t"
done
```

**Expected**: every test `ok. N passed; 0 failed`. FR-003 preserves non-cargo behavior; FR-004 preserves cargo behavior for non-vaultwarden-shape scans.

## Reproducer 4 — Verify SC-002/SC-003 explicitly via jq

```bash
jq '.metadata.component | {name, purl}' /tmp/vaultwarden-post-m201.cdx.json
```

**Expected**: `{"name": "vaultwarden", "purl": "pkg:cargo/vaultwarden@1.0.0"}`.

```bash
jq '[.components[] | select(.name == "vaultwarden")] | length' /tmp/vaultwarden-post-m201.cdx.json
```

**Expected**: `0` — vaultwarden is now `metadata.component`, no longer in `components[]`.

## Reproducer 5 — Verify SC-006 pre-PR wall-clock delta

```bash
git checkout main
time ./scripts/pre-pr.sh 2>&1 | tail -3   # baseline

git checkout 201-root-selector-workspace-root-fix
time ./scripts/pre-pr.sh 2>&1 | tail -3   # post-m201
```

Delta MUST be ≤ 5s per SC-006. Expected delta ≈0s (3 small source edits + 1 fixture extension + 1 test addition).

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199/m200 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
# Post-implementation golden drift check:
git diff --stat mikebom-cli/tests/fixtures/ 2>&1 | tail
```

**Expected**: only the new `sub/package.json` in the extended m200 fixture. If any existing golden JSON drifts, investigate — that would be a FR-004 regression requiring code investigation.
