# Quickstart: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Date**: 2026-07-15
**Audience**: mikebom maintainer implementing or reviewing m199.

## Prerequisites

- Working mikebom checkout on branch `199-reconciler-array-alias`.
- `cargo +stable` toolchain (existing workspace toolchain).

## Reproducer 1 — Verify US1 always-array shape (multi-declaration)

```bash
mkdir -p /tmp/m199-multi/packages/{foo,bar}
cat > /tmp/m199-multi/package.json <<'EOF'
{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}
EOF
cat > /tmp/m199-multi/packages/foo/package.json <<'EOF'
{"name": "foo", "version": "1.0.0", "dependencies": {"commander": "^11.0"}}
EOF
cat > /tmp/m199-multi/packages/bar/package.json <<'EOF'
{"name": "bar", "version": "1.0.0", "dependencies": {"commander": "^11.1.0"}}
EOF
cat > /tmp/m199-multi/package-lock.json <<'EOF'
{
  "name": "root", "version": "1.0.0", "lockfileVersion": 3,
  "packages": {
    "": {"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]},
    "packages/foo": {"name": "foo", "version": "1.0.0", "dependencies": {"commander": "^11.0"}},
    "packages/bar": {"name": "bar", "version": "1.0.0", "dependencies": {"commander": "^11.1.0"}},
    "node_modules/commander": {"version": "11.1.0"}
  }
}
EOF

mikebom sbom scan --offline --path /tmp/m199-multi/ --format cyclonedx-json --output /tmp/multi-out.json
jq '.components[] | select(.purl == "pkg:npm/commander@11.1.0") | .properties[] | select(.name | test("mikebom:requirement-ranges|mikebom:source-manifests|mikebom:declared-as"))' /tmp/multi-out.json
```

**Expected**: two properties (no `mikebom:declared-as` for this fixture — no aliases):
```json
{"name": "mikebom:requirement-ranges", "value": "[\"^11.0\",\"^11.1.0\"]"}
{"name": "mikebom:source-manifests",   "value": "[\"packages/bar/package.json\",\"packages/foo/package.json\"]"}
```

**Verify no singular scalars**:
```bash
grep -c 'mikebom:requirement-range"' /tmp/multi-out.json  # (singular, no `s`)
grep -c 'mikebom:source-manifest"'   /tmp/multi-out.json  # (singular)
```
Both counts MUST be `0`.

## Reproducer 2 — Verify US2 npm-alias resolved-identity matching

```bash
mkdir -p /tmp/m199-alias
cat > /tmp/m199-alias/package.json <<'EOF'
{
  "name": "my-app", "version": "1.0.0",
  "dependencies": {
    "my-alias": "npm:actual-pkg@1.0.0"
  }
}
EOF
cat > /tmp/m199-alias/package-lock.json <<'EOF'
{
  "name": "my-app", "version": "1.0.0", "lockfileVersion": 3,
  "packages": {
    "": {"name": "my-app", "version": "1.0.0",
          "dependencies": {"my-alias": "npm:actual-pkg@1.0.0"}},
    "node_modules/my-alias": {"name": "actual-pkg", "version": "1.0.0"}
  }
}
EOF

mikebom sbom scan --offline --path /tmp/m199-alias/ --format cyclonedx-json --output /tmp/alias-out.json

# Verify: exactly one actual-pkg component with declared-as annotation.
jq '.components[] | select(.purl == "pkg:npm/actual-pkg@1.0.0") | {purl, declared_as: (.properties[]? | select(.name == "mikebom:declared-as") | .value)}' /tmp/alias-out.json
# Verify: no phantom `pkg:npm/my-alias` component.
jq '[.components[] | select(.purl | test("pkg:npm/my-alias"))] | length' /tmp/alias-out.json
```

**Expected**:
- First jq: `{"purl": "pkg:npm/actual-pkg@1.0.0", "declared_as": "[\"my-alias\"]"}`
- Second jq: `0` (no phantom).

## Reproducer 3 — Verify SC-004 determinism (two consecutive scans byte-identical)

```bash
mikebom sbom scan --offline --path /tmp/m199-alias/ --format cyclonedx-json --output /tmp/alias-out-1.json
mikebom sbom scan --offline --path /tmp/m199-alias/ --format cyclonedx-json --output /tmp/alias-out-2.json
diff /tmp/alias-out-1.json /tmp/alias-out-2.json && echo "byte-identical"
```

**Expected**: `byte-identical` (after mikebom's standard non-determinism masking). Determinism failure → FR-003 sort ordering broken.

## Reproducer 4 — Verify FR-008 zero-drift on existing goldens

```bash
cargo test --workspace 2>&1 | grep -E "(regression|golden)" | tail
```

**Expected**: all existing golden-regression tests pass byte-identically. Only the 2 new US1/US2 fixtures contribute new golden files (if goldens are generated for scan_npm.rs tests — which they may or may not depending on test structure).

## Reproducer 5 — Verify SC-007 pre-PR wall-clock delta

```bash
# 1. Time post-m199 (current HEAD):
time ./scripts/pre-pr.sh 2>&1 | tail -3

# 2. Stash m199 changes, time pre-m199 baseline:
git stash push -m 'm199-scratch: measure pre-PR baseline'
time ./scripts/pre-pr.sh 2>&1 | tail -3
git stash pop
```

Delta MUST be ≤ 5s per SC-007. Expected delta ≈0s (no runtime cost added; only 2 new tests).

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.
