# Quickstart — milestone 110 (Pluggable fingerprint corpus v2)

How an operator + a corpus author exercise this milestone end-to-end. Three personas:

1. **Operator** running mikebom against a build tree.
2. **Corpus author** publishing a v2 corpus to their own source.
3. **OSS contributor** verifying no milestone-108 regression.

Each scenario maps directly to the spec's user stories so the implementation can use this doc as the operator-facing test plan.

## Scenario 1 — Operator consumes a v2 corpus to get versioned PURLs (US1)

**Setup**: an operator with valid credentials for a configured corpus source. The corpus contains a v2 record for OpenSSL 3.1.4.

**Step 1**: Configure the source via env vars (the lowest-friction onboarding):

```bash
export MIKEBOM_FINGERPRINTS_CORPUS=1
export MIKEBOM_FINGERPRINTS_SOURCES="https://corpus.example/release.json=KUSARI_CORPUS_TOKEN"
export KUSARI_CORPUS_TOKEN="$(cat ~/.secrets/mikebom-corpus-token)"
```

**Step 2**: Run the scan:

```bash
mikebom sbom scan --path ./build/ --output sbom.cdx.json
```

**Expected output** (info-level logs to stderr):

```text
INFO mikebom::fingerprints::fetch: source https://corpus.example/release.json fetched archive abc123, 30 records loaded
INFO mikebom::fingerprints::fetch: source https://kusari-sandbox.github.io/mikebom-fingerprints/release.json cache hit (TTL 22h remaining)
INFO mikebom::scan_cmd: scan complete: 14 components written to sbom.cdx.json
```

**Verify the versioned PURL emitted**:

```bash
jq '.components[] | select(.name == "openssl")' sbom.cdx.json
```

Expected (key fields):

```json
{
  "name": "openssl",
  "purl": "pkg:github/openssl/openssl@openssl-3.1.4",
  "evidence": {
    "identity": [{
      "field": "purl",
      "confidence": 0.95,
      "methods": [
        { "technique": "binary-analysis", "confidence": 0.85, "value": "indicator-kind:exported_symbols" },
        { "technique": "binary-analysis", "confidence": 0.95, "value": "indicator-kind:version_string" }
      ]
    }]
  },
  "properties": [
    { "name": "mikebom:confidence", "value": "high" },
    { "name": "mikebom:indicators-matched", "value": "[\"exported_symbols\",\"version_string\"]" },
    { "name": "mikebom:purl-aliases", "value": "[\"pkg:deb/debian/libssl3@3.1.4-1\"]" },
    { "name": "mikebom:fingerprint-corpus-sha", "value": "[{\"source_id\":\"<source-hash>\",\"sha\":\"abc123def456...\"}]" }
  ]
}
```

The component has BOTH the native CDX `evidence.identity` block AND the parity-bridging `mikebom:*` properties (per the R1 audit).

## Scenario 2 — Operator with no auth gets graceful fallback (US2)

**Setup**: same operator as scenario 1, but without `KUSARI_CORPUS_TOKEN` set.

**Step 1**: Same env vars EXCEPT the token is unset:

```bash
export MIKEBOM_FINGERPRINTS_CORPUS=1
export MIKEBOM_FINGERPRINTS_SOURCES="https://corpus.example/release.json=KUSARI_CORPUS_TOKEN"
unset KUSARI_CORPUS_TOKEN
```

**Step 2**: Same scan command.

**Expected output** (warning visible):

```text
WARN mikebom::fingerprints::fetch: source https://corpus.example/release.json declared credential_env=KUSARI_CORPUS_TOKEN but the env var is unset; skipping this source. Set the env var or remove the source from configuration.
INFO mikebom::fingerprints::fetch: source https://kusari-sandbox.github.io/mikebom-fingerprints/release.json cache hit
INFO mikebom::scan_cmd: scan complete: 14 components written to sbom.cdx.json
```

**Verify**: scan exit code is 0; sbom.cdx.json still contains the openssl component, but with the milestone-108 v1-compat fields (no `evidence.identity` block from the v2 record; the component is `pkg:generic/openssl` and `mikebom:confidence: "medium"` from the v1-compat baseline).

## Scenario 3 — OSS contributor verifies no regression (US3)

**Setup**: a contributor with no auth credentials, no `MIKEBOM_FINGERPRINTS_SOURCES` set, just the milestone-108 default.

**Step 1**: Minimal config:

```bash
export MIKEBOM_FINGERPRINTS_CORPUS=1
# Nothing else.
```

**Step 2**: Run against the milestone-108 reference fixture:

```bash
mikebom sbom scan --path ./mikebom-cli/tests/fixtures/fingerprints/m108-reference/ --output baseline.cdx.json
```

**Expected**: emitted SBOM identical to the pre-milestone-110 baseline modulo the addition of `mikebom:confidence: "medium"` on each fingerprint-derived component. CI checks this with:

```bash
# Canonicalize + SHA-256 + compare against the re-anchored golden
mikebom-cli/tests/golden/fingerprints_v1_regression.cdx.json.sha256
```

The OSS-regression CI lane (FR-019) runs this check on every PR.

## Scenario 4 — Multi-indicator collision emits both candidates (US4)

**Setup**: a binary statically linked against BoringSSL; the configured corpus contains records for BOTH BoringSSL and OpenSSL (the public-API-overlap collision case from the design doc).

```bash
mikebom sbom scan --path ./build-boringssl/ --output sbom.cdx.json
```

**Expected**: TWO components emitted, each cross-referencing the other:

```bash
jq '.components[] | select(.name | test("(openssl|boringssl)"))' sbom.cdx.json
```

```json
[
  {
    "name": "boringssl",
    "purl": "pkg:github/google/boringssl@<commit>",
    "properties": [
      { "name": "mikebom:confidence", "value": "high" },
      { "name": "mikebom:also-detected-via", "value": "[\"pkg:github/openssl/openssl@*\"]" }
    ]
  },
  {
    "name": "openssl",
    "purl": "pkg:github/openssl/openssl@*",
    "properties": [
      { "name": "mikebom:confidence", "value": "medium" },
      { "name": "mikebom:also-detected-via", "value": "[\"pkg:github/google/boringssl@<commit>\"]" }
    ]
  }
]
```

The boringssl record (with a version-string indicator pinning the specific build) lands at `high` confidence; the openssl record (matching only on shared symbols) lands at `medium`. The operator sees the strongest claim first plus the corroboration.

## Scenario 5 — Self-identity scan of openssl's own source tree

**Setup**: an OpenSSL maintainer running mikebom against the openssl source tree itself.

```bash
cd ~/projects/openssl
mikebom sbom scan --path . --output self-scan.cdx.json
```

**Expected**: the matcher resolves self-identity from `CMakeLists.txt::project(openssl)`, sees that the configured corpus's openssl record's PURL matches, and SKIPS the record's weak indicators (exported_symbols, version_string) per the self-suppression rule. Strong indicators (Build-Id of the locally-built binary, if present) MAY still emit a self-attribution component — the operator sees "this binary is openssl 3.1.4-dev" which IS useful information for them.

If the operator wants to FORCE the corpus to match openssl against their own source (e.g., testing the corpus record's symbol set against their development build):

```bash
mikebom sbom scan --path . --scan-as my-test-project --output self-scan.cdx.json
```

`--scan-as my-test-project` makes self-identity NOT match the openssl record → matcher applies the openssl record as a third-party dep. Used for corpus QA workflows.

## Scenario 6 — Corpus author tests their own source against mikebom

**Setup**: a corpus author has built their v2 corpus archive and wants to verify mikebom consumes it correctly before publishing.

**Step 1**: Start the test corpus server:

```bash
./scripts/fixture-corpus-server --port 8443 --root /path/to/local/corpus/ &
```

**Step 2**: Configure mikebom against the local server:

```bash
export MIKEBOM_FINGERPRINTS_CORPUS=1
export MIKEBOM_FINGERPRINTS_SOURCES="https://localhost:8443/release.json"
mikebom fingerprints fetch --source https://localhost:8443/release.json --force
```

**Expected**: archive fetched, signature verified (against the test allowed-issuer), 30 records loaded, no warnings. The fixture-corpus-server script is part of `mikebom-cli/tests/fingerprints_v2_pluggable.rs`; corpus authors can copy it as a starting reference for their own deployment.

## CI lane setup (for the OSS-regression contract)

A new GitHub Actions job in `.github/workflows/ci.yml`:

```yaml
fingerprints-v1-regression:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v5
      with:
        persist-credentials: false
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - run: |
        # No auth credentials in environment; no extra sources configured.
        # The milestone-108 default source loads from cache (Swatinem populates it from prior runs)
        # OR a fresh fetch if cache miss.
        unset MIKEBOM_FINGERPRINTS_SOURCES
        cargo +stable test --workspace --test fingerprints_v1_regression
```

The test asserts byte-equality against the re-anchored milestone-108 golden modulo the new `mikebom:confidence: "medium"` annotation.

## What this milestone does NOT cover

(Covered in spec § Assumptions; surfaced here for the operator audience.)

- The corpus contents themselves — only the mechanism. To get a corpus, an operator either consumes the milestone-108 public corpus (default), runs their own ingestion pipeline (out of scope), or configures a third-party source (out of scope for distribution).
- Low-confidence operator triage — matches below the medium floor are suppressed (no emission). A follow-on milestone adds `mikebom-overrides.yaml` + `mikebom corpus contribute`.
- Sharding — full-archive fetch only. A follow-on milestone adds lazy per-library shards.
- Source-tree copyright-header indicators — binary-only indicators in this milestone.
- Function-body hashing — remains a research-stage indicator type.
