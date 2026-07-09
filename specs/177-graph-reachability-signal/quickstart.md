# Quickstart: m177 Manual Verification

**Feature**: 177-graph-reachability-signal
**Date**: 2026-07-09

Four verification paths — one per user story + one polyglot scenario + one composition-with-existing-codes check.

## Path A — US2: constraint-only scan flips to `partial`

**Setup**: `requirements.txt`-only pip fixture.

```bash
mkdir -p /tmp/m177-us2 && cat > /tmp/m177-us2/requirements.txt <<'EOF'
requests>=2.31.0
click>=8.1.7
pyyaml>=6.0
EOF

mikebom --offline sbom scan --path /tmp/m177-us2 \
    --format cyclonedx-json \
    --output /tmp/m177-us2.cdx.json \
    --no-deep-hash
```

**Path A.1 — completeness value flips**:

```bash
jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness") | .value' /tmp/m177-us2.cdx.json
```

**Expected**: `partial` (pre-177 was `complete` — the change is the milestone deliverable).

**Path A.2 — reason code appears**:

```bash
jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness-reason") | .value' /tmp/m177-us2.cdx.json
```

**Expected**: value contains `transitive-edges-unresolvable: pypi`.

## Path B — US1: reachability tool machine-check

**Setup**: same SBOM from Path A.

```bash
# Emulate a reachability tool's pre-flight gate:
IS_UNRELIABLE=$(jq -r '
    .metadata.properties[]?
    | select(.name == "mikebom:graph-completeness-reason")
    | .value
    | contains("transitive-edges-unresolvable")
' /tmp/m177-us2.cdx.json)

if [ "$IS_UNRELIABLE" = "true" ]; then
    echo "Graph unreliable for reachability — refusing to run."
    exit 1
fi
```

**Expected**: script exits 1 with "Graph unreliable" message.

**Path B.2 — extract affected ecosystems**:

```bash
jq -r '
    .metadata.properties[]?
    | select(.name == "mikebom:graph-completeness-reason")
    | .value
    | capture("transitive-edges-unresolvable: (?<eco>[^;]+)")
    | .eco
    | split(", ")
    | .[]
' /tmp/m177-us2.cdx.json
```

**Expected**: one line — `pypi`.

## Path C — US3: polyglot scan enumerates affected ecosystems

**Setup**: cargo-with-lockfile PLUS pip-without-lockfile fixture.

```bash
mkdir -p /tmp/m177-us3/rust && cat > /tmp/m177-us3/rust/Cargo.toml <<'EOF'
[package]
name = "poly"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
EOF
cat > /tmp/m177-us3/rust/Cargo.lock <<'EOF'
[[package]]
name = "poly"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.219"
EOF

cat > /tmp/m177-us3/requirements.txt <<'EOF'
requests>=2.31.0
EOF

mikebom --offline sbom scan --path /tmp/m177-us3 \
    --format cyclonedx-json \
    --output /tmp/m177-us3.cdx.json \
    --no-deep-hash
```

**Path C.1 — completeness is `partial`**:

```bash
jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness") | .value' /tmp/m177-us3.cdx.json
```

**Expected**: `partial`.

**Path C.2 — reason names pypi but NOT cargo**:

```bash
jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness-reason") | .value' /tmp/m177-us3.cdx.json
```

**Expected**: contains `transitive-edges-unresolvable: pypi`; does NOT contain `cargo` in that specific code's detail (cargo is source-tier via `Cargo.lock`, so it's reachability-safe).

**Path C.3 — reachability tool filters to safe ecosystems**:

```bash
# A reachability tool wants to know which ecosystems are safe to analyze.
UNRELIABLE_ECOS=$(jq -r '
    .metadata.properties[]?
    | select(.name == "mikebom:graph-completeness-reason")
    | .value
    | capture("transitive-edges-unresolvable: (?<eco>[^;]+)")
    | .eco
    | split(", ")
    | .[]
' /tmp/m177-us3.cdx.json)

# ALL_ECOS = every ecosystem present in components[]
ALL_ECOS=$(jq -r '.components[] | .purl | capture("^pkg:(?<eco>[^/]+)/") | .eco' /tmp/m177-us3.cdx.json | sort -u)

# SAFE = ALL - UNRELIABLE
comm -23 <(echo "$ALL_ECOS") <(echo "$UNRELIABLE_ECOS" | sort -u)
```

**Expected**: `cargo` (safe — can reachability-analyze), NOT `pypi` (unreliable — filter out).

## Path D — SC-002: fully-resolved scan stays `complete`

**Setup**: cargo-with-lockfile only.

```bash
mkdir -p /tmp/m177-safe && cat > /tmp/m177-safe/Cargo.toml <<'EOF'
[package]
name = "safe"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
EOF
cat > /tmp/m177-safe/Cargo.lock <<'EOF'
[[package]]
name = "safe"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.219"
EOF

mikebom --offline sbom scan --path /tmp/m177-safe \
    --format cyclonedx-json \
    --output /tmp/m177-safe.cdx.json \
    --no-deep-hash

jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness") | .value' /tmp/m177-safe.cdx.json
```

**Expected**: `complete`. NO `transitive-edges-unresolvable` in any reason code (annotation may or may not exist depending on m170 dedup + other classifier state).

**SC-006 verification** — the pre-existing `cargo.cdx.json` golden should stay byte-identical to its pre-177 form (modulo the alpha.56 → alpha.57 version bump, which is orthogonal).

## Path E — SC-004: composition with existing codes

**Setup**: a scan that triggers BOTH `TransitiveEdgesUnresolvable` AND `RootSelectionAmbiguous` (or another existing code). Constructing this fixture requires a project with (a) multiple candidate roots + (b) design-tier components. Simplest: two-module monorepo with `requirements.txt` files at both roots.

```bash
mkdir -p /tmp/m177-compose/a /tmp/m177-compose/b
echo "requests>=2.31.0" > /tmp/m177-compose/a/requirements.txt
echo "click>=8.1.7" > /tmp/m177-compose/b/requirements.txt

mikebom --offline sbom scan --path /tmp/m177-compose \
    --format cyclonedx-json \
    --output /tmp/m177-compose.cdx.json \
    --no-deep-hash

jq -r '.metadata.properties[]? | select(.name == "mikebom:graph-completeness-reason") | .value' /tmp/m177-compose.cdx.json
```

**Expected**: value contains BOTH `transitive-edges-unresolvable: pypi` AND at least one other reason code, semicolon-joined per `join_reason_codes`. (Exact other-code depends on m127 root-selector behavior on the fixture; the test-side assertion is the presence of the m177 code in a semicolon-joined value alongside ≥1 other code.)

## Full success criteria table

| SC | Verification | Expected |
|---|---|---|
| SC-001 | Path B.1 IS_UNRELIABLE result | `true` (script exits 1) |
| SC-002 | Path D `graph-completeness` value | `complete` on cargo-with-lockfile |
| SC-003 | Path C.2 `transitive-edges-unresolvable: pypi` (no cargo) | matches |
| SC-004 | Path E reason contains ≥2 codes semicolon-joined | matches |
| SC-005 | Path A.2 `grep -F "transitive-edges-unresolvable: "` | 1 hit |
| SC-006 | Cargo/gem/npm/maven/apk/deb/rpm goldens byte-identical modulo version bump | zero diff outside version-string |
| SC-007 | pip / composer goldens flip `graph-completeness` value + gain reason code | diff scope bounded to those two annotations |
| SC-008 | Manual audit — reading-guide subsection updated per Entity 5 | prose-level; verify at PR review |
