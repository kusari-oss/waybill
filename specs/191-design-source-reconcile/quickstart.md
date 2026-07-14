# Quickstart: m191 Design-Tier / Source-Tier Reconciliation Verification

**Date**: 2026-07-14
**Audience**: Developer implementing or reviewing m191; operator verifying the fix against a real monorepo.

## Purpose

Reproduces the reconciliation behavior (US1) and the versionless PURL fix (US2), both against synthetic fixtures and against a real npm project.

## Prerequisites

- mikebom binary at or after m191 (`cargo build --release -p mikebom-cli`, tagged post-merge)
- `jq` for JSON inspection
- Python 3.10+ with `.venv/spdx3-validate/bin/spdx3-validate` installed (per memory `reference_spdx3_validator`)

## Reproducer 1 — Synthetic reconciliation fixture

Build an npm project with a declared-and-resolved dep:

```bash
mkdir -p /tmp/m191-fixture-a && cd /tmp/m191-fixture-a

cat > package.json <<'EOF'
{
  "name": "m191-fixture-a",
  "version": "0.1.0",
  "dependencies": {
    "commander": "^11.1.0"
  }
}
EOF

cat > package-lock.json <<'EOF'
{
  "name": "m191-fixture-a",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "m191-fixture-a", "version": "0.1.0",
          "dependencies": { "commander": "^11.1.0" } },
    "node_modules/commander": { "version": "11.1.0" }
  }
}
EOF
```

Scan in all three formats:

```bash
mkdir -p /tmp/m191-out
mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m191-fixture-a --output /tmp/m191-out/a.cdx.json
mikebom sbom scan --offline --format spdx-2.3-json \
  --path /tmp/m191-fixture-a --output /tmp/m191-out/a.spdx.json
mikebom sbom scan --offline --format spdx-3-json \
  --path /tmp/m191-fixture-a --output /tmp/m191-out/a.spdx3.json
```

### Assertion 1 — Reconciled component count (US1)

```bash
jq '[.components[] | select(.name == "commander")] | length' /tmp/m191-out/a.cdx.json
```

**Expected**: `1`
**Pre-m191 (broken)**: `2` (one design-tier + one source-tier)

### Assertion 2 — Design-tier annotations transferred to source-tier survivor

```bash
jq '.components[] | select(.name == "commander") | .properties' /tmp/m191-out/a.cdx.json
```

**Expected**: includes `mikebom:sbom-tier=source`, `mikebom:requirement-range=^11.1.0`, `mikebom:source-manifest=package.json`.

### Assertion 3 — PURL and version are concrete

```bash
jq '.components[] | select(.name == "commander") | {purl, version, "bom-ref": .["bom-ref"]}' /tmp/m191-out/a.cdx.json
```

**Expected**:
```json
{
  "purl": "pkg:npm/commander@11.1.0",
  "version": "11.1.0",
  "bom-ref": "pkg:npm/commander@11.1.0"
}
```

No trailing `@`; no empty version.

### Assertion 4 — Cross-format PURL parity (FR-015)

```bash
CDX=$(jq -r '.components[] | select(.name=="commander") | .purl' /tmp/m191-out/a.cdx.json)
SPDX=$(jq -r '.packages[] | select(.name=="commander") | .externalRefs[] | select(.referenceType=="purl") | .referenceLocator' /tmp/m191-out/a.spdx.json)
SPDX3=$(jq -r '.["@graph"][] | select(.type=="software_Package" and .name=="commander") | .software_packageUrl' /tmp/m191-out/a.spdx3.json)

[ "$CDX" = "$SPDX" ] && [ "$SPDX" = "$SPDX3" ] && echo "PARITY OK: $CDX" || echo "PARITY FAIL"
```

**Expected**: `PARITY OK: pkg:npm/commander@11.1.0`

## Reproducer 2 — Standalone versionless design-tier (US2)

Build an npm project with an `optionalDependencies` entry that has NO lockfile resolution:

```bash
mkdir -p /tmp/m191-fixture-c && cd /tmp/m191-fixture-c

cat > package.json <<'EOF'
{
  "name": "m191-fixture-c",
  "version": "0.1.0",
  "optionalDependencies": {
    "not-installed-dep": "^1.0.0"
  }
}
EOF

cat > package-lock.json <<'EOF'
{
  "name": "m191-fixture-c",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "m191-fixture-c", "version": "0.1.0",
          "optionalDependencies": { "not-installed-dep": "^1.0.0" } }
  }
}
EOF

mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m191-fixture-c --output /tmp/m191-out/c.cdx.json
```

### Assertion 5 — Versionless PURL, no trailing @

```bash
jq -r '.components[] | select(.name == "not-installed-dep") | .purl' /tmp/m191-out/c.cdx.json
```

**Expected**: `pkg:npm/not-installed-dep`
**Pre-m191 (broken)**: `pkg:npm/not-installed-dep@`

Grep to confirm ZERO trailing-`@` PURLs anywhere:

```bash
grep -oE '"pkg:[^"]*@"' /tmp/m191-out/c.cdx.json
```

**Expected**: no matches.

### Assertion 6 — CDX `.version` field OMITTED

```bash
jq '.components[] | select(.name == "not-installed-dep") | has("version")' /tmp/m191-out/c.cdx.json
```

**Expected**: `false` (field entirely absent from JSON).
**Pre-m191 (broken)**: `true`, with value `""`.

### Assertion 7 — SPDX 2.3 uses NOASSERTION (FR-011)

```bash
mikebom sbom scan --offline --format spdx-2.3-json \
  --path /tmp/m191-fixture-c --output /tmp/m191-out/c.spdx.json

jq -r '.packages[] | select(.name == "not-installed-dep") | .versionInfo' /tmp/m191-out/c.spdx.json
```

**Expected**: `NOASSERTION`

### Assertion 8 — SPDX 3 omits `software_packageVersion` (FR-012)

```bash
mikebom sbom scan --offline --format spdx-3-json \
  --path /tmp/m191-fixture-c --output /tmp/m191-out/c.spdx3.json

jq '.["@graph"][] | select(.type=="software_Package" and .name=="not-installed-dep") | has("software_packageVersion")' /tmp/m191-out/c.spdx3.json
```

**Expected**: `false`

### Assertion 9 — spdx3-validate conformance (SC-007)

```bash
.venv/spdx3-validate/bin/spdx3-validate /tmp/m191-out/c.spdx3.json
```

**Expected**: exit 0, zero violations.

## Reproducer 3 — Multi-declaration reconciliation (Q1 / FR-004)

Build an npm workspace where two child manifests declare different ranges for the same dep:

```bash
mkdir -p /tmp/m191-fixture-b/packages/foo /tmp/m191-fixture-b/packages/bar

cat > /tmp/m191-fixture-b/package.json <<'EOF'
{
  "name": "m191-fixture-b-root",
  "version": "0.1.0",
  "workspaces": ["packages/*"]
}
EOF

cat > /tmp/m191-fixture-b/packages/foo/package.json <<'EOF'
{ "name": "foo", "version": "0.1.0", "dependencies": { "commander": "^11.0" } }
EOF

cat > /tmp/m191-fixture-b/packages/bar/package.json <<'EOF'
{ "name": "bar", "version": "0.1.0", "dependencies": { "commander": "^11.1.0" } }
EOF

# Root lockfile resolving both to 11.1.0.
cat > /tmp/m191-fixture-b/package-lock.json <<'EOF'
{
  "name": "m191-fixture-b-root",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "m191-fixture-b-root", "workspaces": ["packages/*"] },
    "packages/foo": { "name": "foo", "dependencies": { "commander": "^11.0" } },
    "packages/bar": { "name": "bar", "dependencies": { "commander": "^11.1.0" } },
    "node_modules/commander": { "version": "11.1.0" }
  }
}
EOF

mikebom sbom scan --offline --format cyclonedx-json \
  --path /tmp/m191-fixture-b --output /tmp/m191-out/b.cdx.json
```

### Assertion 10 — Multiple range entries preserved

```bash
jq '[.components[] | select(.name == "commander") | .properties[]? |
     select(.name == "mikebom:requirement-range") | .value]' /tmp/m191-out/b.cdx.json
```

**Expected**: `["^11.0", "^11.1.0"]` (both ranges preserved; NOT collapsed).

```bash
jq '[.components[] | select(.name == "commander") | .properties[]? |
     select(.name == "mikebom:source-manifest") | .value]' /tmp/m191-out/b.cdx.json
```

**Expected**: `["packages/foo/package.json", "packages/bar/package.json"]` (both manifests preserved; pairing intact by insertion order).

## Real-world validation (SC-005 — approximate)

Against a real large monorepo scan (e.g., a checkout of a real React Native project):

```bash
mikebom sbom scan --offline --format cyclonedx-json \
  --path /path/to/big-monorepo --output /tmp/monorepo.cdx.json

jq '.components | length' /tmp/monorepo.cdx.json
```

Compare against a pre-m191 baseline for the same repo. Expected: ≥5% reduction in component count (per the customer report of 101/1998 ≈ 5% duplicates).

## CI verification recap

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets -- -D warnings` AND `cargo +stable test --workspace` MUST pass clean.

New test files:
- `mikebom-cli/tests/design_source_reconcile.rs` (US1 acceptance + FR-004 multi-decl + Q2 workspace-scope + FR-005 graph-edge rewriting)
- `mikebom-cli/tests/design_tier_versionless_purl.rs` (US2 acceptance + FR-009/010/011/012 + FR-014 round-trip)
- New unit tests co-located with `reconcile_design_source_tiers` in `mikebom-cli/src/resolve/reconciler.rs`
- New unit tests for each `build_*_purl` helper's empty-version branch (11 ecosystems × 1-2 tests each)
