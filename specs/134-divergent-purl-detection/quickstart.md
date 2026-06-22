# Quickstart — milestone 134 divergent-PURL detection

Operator-facing walkthrough of the four scenarios this milestone surfaces.

## Scenario 1 — Detect an accidental shadow copy (US1 / SC-001)

Setup:

```bash
mkdir -p /tmp/repro/crates/foo
mkdir -p /tmp/repro/vendor/foo

# Original crate
cat > /tmp/repro/crates/foo/Cargo.toml <<'EOF'
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
tokio = "1"
EOF

# Vendored copy with an EXTRA dep that the original doesn't have
cat > /tmp/repro/vendor/foo/Cargo.toml <<'EOF'
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
tokio = "1"
anyhow = "1"
EOF
```

Run:

```bash
mikebom --offline sbom scan --path /tmp/repro --output /tmp/repro.cdx.json
```

Inspect (per-component property):

```bash
jq '
  .components[]
  | select(.purl == "pkg:cargo/foo@1.2.3")
  | .properties[]
  | select(.name == "mikebom:duplicate-purl-divergent")
  | .value | fromjson
' /tmp/repro.cdx.json
```

Expected output (formatting added):

```json
{
  "v": 1,
  "purl": "pkg:cargo/foo@1.2.3",
  "reason": "deps-differ",
  "paths": [
    "crates/foo/Cargo.toml",
    "vendor/foo/Cargo.toml"
  ],
  "dep_sets_by_path": {
    "crates/foo/Cargo.toml": ["serde", "tokio"],
    "vendor/foo/Cargo.toml": ["anyhow", "serde", "tokio"]
  }
}
```

Inspect (document-scope summary):

```bash
jq '
  .metadata.properties[]
  | select(.name == "mikebom:purl-collisions-detected")
  | .value | fromjson
' /tmp/repro.cdx.json
```

Expected output: `CollisionsSummary` envelope with one entry (the same `DivergenceRecord` as above).

The pre-existing `tracing::warn!` from milestone 064 ALSO fires alongside the annotations.

## Scenario 2 — No divergence: identical vendored copy (SC-002 regression invariant)

Setup: same as Scenario 1, but make the two `Cargo.toml` files byte-identical:

```bash
cp /tmp/repro/crates/foo/Cargo.toml /tmp/repro/vendor/foo/Cargo.toml
```

Run + inspect:

```bash
mikebom --offline sbom scan --path /tmp/repro --output /tmp/repro.cdx.json
jq '.metadata.properties[] | select(.name == "mikebom:purl-collisions-detected")' /tmp/repro.cdx.json
# (empty output — annotation MUST NOT appear)
jq '.components[] | select(.purl == "pkg:cargo/foo@1.2.3").properties[]?.name' /tmp/repro.cdx.json | grep duplicate-purl-divergent
# (empty output — property MUST NOT appear)
```

The emitted SBOM is byte-identical to the pre-milestone-134 baseline for this fixture.

## Scenario 3 — Detect an adversarial shadow via deep-hash (US2 / SC-003)

Setup: same `Cargo.toml` files (byte-identical dep sets), but DIFFERENT `src/lib.rs` contents:

```bash
mkdir -p /tmp/repro/crates/foo/src /tmp/repro/vendor/foo/src
echo "pub fn safe() {}" > /tmp/repro/crates/foo/src/lib.rs
echo "pub fn malicious() {}" > /tmp/repro/vendor/foo/src/lib.rs
# Cargo.toml byte-identical between the two
cp /tmp/repro/crates/foo/Cargo.toml /tmp/repro/vendor/foo/Cargo.toml
```

Run with `--deep-hash`:

```bash
mikebom --offline sbom scan --path /tmp/repro --deep-hash --output /tmp/repro.cdx.json
```

Inspect:

```bash
jq '
  .components[]
  | select(.purl == "pkg:cargo/foo@1.2.3")
  | .properties[]
  | select(.name == "mikebom:duplicate-purl-divergent")
  | .value | fromjson
' /tmp/repro.cdx.json
```

Expected: `reason: "hashes-differ"` with `hashes_by_path` populated.

Without `--deep-hash`: the annotation does NOT appear (FR-005 + Scenario 2 logic).

## Scenario 4 — Scan-wide summary (US3)

Setup: three independent divergent collisions in the same workspace. Each follows the Scenario 1 pattern but with different crate names (`foo`, `bar`, `baz`).

Run + inspect the document-scope summary:

```bash
mikebom --offline sbom scan --path /tmp/repro-multi --output /tmp/repro-multi.cdx.json
jq '
  .metadata.properties[]
  | select(.name == "mikebom:purl-collisions-detected")
  | .value | fromjson
  | .collisions
  | length
' /tmp/repro-multi.cdx.json
# Expected: 3
```

The `collisions[]` array is sorted lex by `record.purl.as_str()` — order is deterministic across runs.

## Verification commands

End-to-end SC validations:

```bash
# SC-001 — declared-dep divergence detected
cargo test -p mikebom --test divergent_purl_deps_differ

# SC-002 — no false positives on identical vendored copies
cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression

# SC-003 — deep-hash divergence detected only under --deep-hash
cargo test -p mikebom --test divergent_purl_hashes_differ

# SC-004 — perf budget
# (Not enforced as a per-PR gate per milestone-094 architecture; tracked
# via the `Performance benchmarks` workflow's daily lane.)

# SC-005 — single jq query enumerates every collision
jq '.metadata.properties[] | select(.name == "mikebom:purl-collisions-detected") | .value | fromjson | .collisions[].purl' /tmp/repro.cdx.json
```

## Cross-format byte-equivalence check

```bash
# Generate the same fixture's SBOM in all three formats
mikebom --offline sbom scan --path /tmp/repro \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=/tmp/cdx.json \
  --output spdx-2.3-json=/tmp/spdx.json \
  --output spdx-3-json=/tmp/spdx3.json

# Run the parity extractors (already wired through milestone-071 infrastructure)
cargo test -p mikebom --test parity_extractor_divergent_purl_per_component
cargo test -p mikebom --test parity_extractor_divergent_purl_document_scope
```

Both extractors run the canonicalize-and-compare check from milestone-071's `canonicalize_for_compare` helper. Pass = the three formats carry byte-identical (after canonicalization) divergence payloads.
