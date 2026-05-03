# Quickstart: Verify npm main-module emission

Three recipes covering single-package, scoped-name, and workspace cases.

## Prerequisites

```sh
cargo +stable build -p mikebom
```

## Recipe A — Single-package npm scan (express)

```sh
git clone --depth 1 https://github.com/expressjs/express /tmp/express-066
target/debug/mikebom sbom scan \
  --path /tmp/express-066 \
  --format cyclonedx-json \
  --output /tmp/express-066.cdx.json \
  --no-deep-hash

jq '.metadata.component | { bom_ref: ."bom-ref", type, name, version, purl }' \
  /tmp/express-066.cdx.json
```

**Expect** (`<x.y.z>` matches the manifest version):
```json
{
  "bom_ref": "pkg:npm/express@<x.y.z>",
  "type": "application",
  "name": "express",
  "version": "<x.y.z>",
  "purl": "pkg:npm/express@<x.y.z>"
}
```

Verify C40 supplementary tag:

```sh
jq '.metadata.component.properties[] | select(.name == "mikebom:component-role") | .value' \
  /tmp/express-066.cdx.json
# → "main-module"
```

## Recipe B — Scoped package (e.g. `@types/node` style)

Synthesize a minimal scoped fixture:

```sh
mkdir -p /tmp/scoped-test
cat > /tmp/scoped-test/package.json <<'EOF'
{
  "name": "@kusari/foo",
  "version": "1.0.0"
}
EOF

target/debug/mikebom sbom scan \
  --path /tmp/scoped-test \
  --format cyclonedx-json \
  --output /tmp/scoped.cdx.json \
  --no-deep-hash

jq '.metadata.component.purl' /tmp/scoped.cdx.json
```

**Expect**:
```
"pkg:npm/%40kusari/foo@1.0.0"
```

The `@` is URL-encoded to `%40` per PURL spec.

## Recipe C — npm 7+ workspace

Synthesize a workspace fixture:

```sh
mkdir -p /tmp/ws-test/packages/a /tmp/ws-test/packages/b
cat > /tmp/ws-test/package.json <<'EOF'
{
  "name": "monorepo-root",
  "private": true,
  "workspaces": ["packages/*"]
}
EOF
cat > /tmp/ws-test/packages/a/package.json <<'EOF'
{ "name": "a", "version": "0.5.0" }
EOF
cat > /tmp/ws-test/packages/b/package.json <<'EOF'
{
  "name": "b",
  "version": "0.5.0",
  "dependencies": { "a": "*" }
}
EOF

target/debug/mikebom sbom scan \
  --path /tmp/ws-test \
  --format spdx-2.3-json \
  --output /tmp/ws.spdx.json \
  --no-deep-hash

jq '[.packages[]
     | select(.primaryPackagePurpose == "APPLICATION")
     | { name, purl: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) }
    ] | sort_by(.name)' \
  /tmp/ws.spdx.json
```

**Expect**:
```json
[
  { "name": "a", "purl": "pkg:npm/a@0.5.0" },
  { "name": "b", "purl": "pkg:npm/b@0.5.0" }
]
```

The workspace root (`monorepo-root`) is correctly skipped per FR-002 (private + no version).

Verify multi-target `documentDescribes`:

```sh
jq '.documentDescribes | length' /tmp/ws.spdx.json
# → 2

jq '[.relationships[] | select(.spdxElementId == "SPDXRef-DOCUMENT" and .relationshipType == "DESCRIBES")] | length' /tmp/ws.spdx.json
# → 2
```

## When to run

- **Recipe A** during US1 implementation as the primary acceptance check
- **Recipe B** to verify scoped-name encoding (US1 AS#2)
- **Recipe C** for workspace + multi-DESCRIBES verification (US1 AS#3 + US3 AS#2)

All three recipes should also be exercised as integration tests in `tests/scan_npm.rs`.
