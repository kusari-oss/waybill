# Contract — Binary-identification enrichment deliverables

Behavioral contracts for every new or modified file. Each contract specifies: (a) the invariant the file/code path holds, (b) a verification recipe — typically a `grep` on emitted SBOM JSON or a `cargo test` invocation.

## Contract 1 — Embedded version-string extraction (FR-001, FR-002, SC-001)

**Path**: `mikebom-cli/src/scan_fs/binary/version_strings.rs`

**Invariant**: when a scanned binary's `.rodata` (ELF), `__cstring` (Mach-O), or `.rdata` (PE) section contains a substring matching any of the 5 v1 patterns at research.md §1, mikebom emits a `PackageDbEntry` with:
- `purl = pkg:generic/<lowercase-library-name>@<captured-version-triplet>` (or `pkg:generic/sqlite` for the SQLite source-id-only path)
- `evidence.identity[].technique = binary-analysis`
- `evidence.identity[].confidence = 0.6`
- `properties[]` includes `mikebom:identification-method = embedded-version-string`
- `evidence.occurrences[]` lists every binary file where this exact PURL was extracted (strict-PURL-equality dedup per Q3)

**Verification (synthetic fixture)**:
```bash
# Build a small C program that statically links OpenSSL 3.x, strip it:
make -C mikebom-cli/tests/fixtures/binary-id/openssl-static
file mikebom-cli/tests/fixtures/binary-id/openssl-static/main
# Expected: ELF, stripped, no DT_NEEDED for libssl.so.

target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/binary-id/openssl-static/ \
    --format cyclonedx-json --output /tmp/openssl-static.cdx.json

jq '.components[] | select(.purl | startswith("pkg:generic/openssl@"))' /tmp/openssl-static.cdx.json
# Expected: one entry with purl = pkg:generic/openssl@<version>, evidence.identity[]
#           containing { technique: "binary-analysis", confidence: 0.6 }, and
#           a property { name: "mikebom:identification-method", value: "embedded-version-string" }.
```

## Contract 2 — Packer detection always-emit property (FR-003, Q2, SC-002)

**Path**: `mikebom-cli/src/scan_fs/binary/packer.rs`

**Invariant**: EVERY file-level binary component in the emitted SBOM carries property `mikebom:binary-packer`. Value is `none` when no v1 packer signature matches; lowercase packer name (currently only `upx`) when a signature matches. Stretch: `mikebom:binary-packer-version` property also emitted when the UPX version banner is found.

**Verification**:
```bash
# Verify always-emit on an unpacked binary:
target/release/mikebom --offline sbom scan --path /usr/bin/  ...
jq '.components[] | select(.name=="ls") | .properties[] | select(.name=="mikebom:binary-packer")' /tmp/output.json
# Expected: { "name": "mikebom:binary-packer", "value": "none" }

# Verify positive detection on a UPX-packed binary:
upx --best mikebom-cli/tests/fixtures/binary-id/upx-fixture/sample
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/binary-id/upx-fixture/
jq '.components[] | select(.name=="sample") | .properties[] | select(.name=="mikebom:binary-packer")' /tmp/output.json
# Expected: { "name": "mikebom:binary-packer", "value": "upx" }
```

## Contract 3 — Symbol-fingerprint extraction (FR-004, SC-003)

**Path**: `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs`

**Invariant**: when an ELF binary's `.dynsym` exports ≥8 of the 10 symbols listed for any v1 library at research.md §3, mikebom emits a `PackageDbEntry` with:
- `purl = pkg:generic/<lowercase-library-name>` (no `@<version>`)
- `version = ""` (empty)
- `evidence.identity[].technique = binary-analysis`
- `evidence.identity[].confidence = 0.4`
- `properties[]` includes `mikebom:identification-method = symbol-fingerprint`
- `properties[]` includes `mikebom:fingerprint-symbols-matched = <N>/<10>` for transparency
- `evidence.occurrences[]` lists every binary that hit the same fingerprint

**Verification**:
```bash
# Build a fixture that exports OpenSSL's symbol set but has the version string stripped:
make -C mikebom-cli/tests/fixtures/binary-id/openssl-symbols-only
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/binary-id/openssl-symbols-only/ \
    --format cyclonedx-json --output /tmp/sym-only.cdx.json

jq '.components[] | select(.purl == "pkg:generic/openssl") | { confidence: .evidence.identity[0].confidence, props: .properties }' /tmp/sym-only.cdx.json
# Expected: confidence 0.4, properties include mikebom:identification-method = symbol-fingerprint and mikebom:fingerprint-symbols-matched = "<count>/10".
```

## Contract 4 — Composite evidence when both techniques match the same library (Clarification Q1, FR-005)

**Path**: `mikebom-cli/src/scan_fs/binary/mod.rs` (post-per-pass aggregation step)

**Invariant**: when a single binary triggers BOTH the embedded-version-string match AND the symbol-fingerprint match for the same library, the aggregator emits ONE `PackageDbEntry` (not two) with:
- `purl = pkg:generic/<lib>@<version>` (the higher-confidence version-string PURL wins for the component identity)
- `evidence.identity[]` array contains BOTH entries — one with `confidence = 0.6` + `identification-method = embedded-version-string`, one with `confidence = 0.4` + `identification-method = symbol-fingerprint`
- `evidence.occurrences[]` contains the single binary (the one where both signals fired)

**Verification**:
```bash
# A fixture that statically links OpenSSL AND retains both version string + symbol exports:
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/binary-id/openssl-full/ \
    --format cyclonedx-json --output /tmp/composite.cdx.json

jq '.components[] | select(.purl | startswith("pkg:generic/openssl@")) | .evidence.identity | length' /tmp/composite.cdx.json
# Expected: 2 (two entries in evidence.identity[]).

jq '[.components[] | select(.name == "openssl")] | length' /tmp/composite.cdx.json
# Expected: 1 (composite-evidence — one component, not two).
```

## Contract 5 — Strict-PURL-equality global dedup (Clarification Q3, FR-011)

**Path**: `mikebom-cli/src/scan_fs/binary/linkage.rs` (extended dedup pass)

**Invariant**: at the end of binary scanning, the global aggregator keys components by their full PURL string. Three binaries that each emit `pkg:generic/openssl@3.0.13` collapse to ONE component with three `evidence.occurrences[]` entries. Three binaries emitting `pkg:generic/openssl@3.0.13` + `pkg:generic/openssl@3.0.12` + `pkg:generic/openssl` (no version) stay as THREE separate components, each with its own `evidence.occurrences[]`.

**Verification**:
```bash
# Scan a directory containing 3 binaries: 2 that statically link OpenSSL 3.0.13 + 1 that statically links OpenSSL 3.0.12.
# (Synthetic fixture — same trick as openssl-static, with two binaries pointing at the same lib build and one pointing at an older lib.)
target/release/mikebom --offline sbom scan \
    --path mikebom-cli/tests/fixtures/binary-id/dedup-test/ \
    --format cyclonedx-json --output /tmp/dedup.cdx.json

# Two distinct PURLs:
jq -r '.components[] | select(.purl | startswith("pkg:generic/openssl")) | .purl' /tmp/dedup.cdx.json
# Expected:
#   pkg:generic/openssl@3.0.13
#   pkg:generic/openssl@3.0.12

# The 3.0.13 component has 2 occurrences, the 3.0.12 has 1:
jq '.components[] | select(.purl == "pkg:generic/openssl@3.0.13") | .evidence.occurrences | length' /tmp/dedup.cdx.json
# Expected: 2

jq '.components[] | select(.purl == "pkg:generic/openssl@3.0.12") | .evidence.occurrences | length' /tmp/dedup.cdx.json
# Expected: 1
```

## Contract 6 — Parity-catalog C12 row (research.md §5)

**Path**: `mikebom-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs` + `docs/reference/sbom-format-mapping.md`

**Invariant**: the `EXTRACTORS` table contains a row with `row_id: "C12", label: "mikebom:binary-packer", directional: Directionality::SymmetricEqual, order_sensitive: false`. The per-format extractors emit `mikebom:binary-packer` as a properties/annotation entry at the component (CDX) / package (SPDX 2.3 + SPDX 3) scope. `docs/reference/sbom-format-mapping.md` includes a C12 row in the same table where C10/C11 are listed.

**Verification**:
```bash
grep -n 'C12.*mikebom:binary-packer' mikebom-cli/src/parity/extractors/mod.rs
# Expected: one match at the existing EXTRACTORS table.

grep -n 'c12_cdx\|c12_spdx23\|c12_spdx3' mikebom-cli/src/parity/extractors/
# Expected: 3 matches (one per format file).

grep -nE '^\| C12 \|.*mikebom:binary-packer' docs/reference/sbom-format-mapping.md
# Expected: one match.

# Run the sbom_format_mapping_coverage parity test:
cargo +stable test -p mikebom --test sbom_format_mapping_coverage
# Expected: passes (the catalog and emitted-fields stay in sync).
```

## Contract 7 — Zero-Cargo-deps + production-code-scope guardrails (FR-007, FR-008, FR-009, FR-010, SC-007)

**Verification**:
```bash
# No new Cargo deps:
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
# Expected: 0

# Production code outside binary/ and parity/extractors/ + the generators: 0:
git diff --name-only main | grep -E '^mikebom-cli/src/' \
  | grep -vE '^mikebom-cli/src/scan_fs/binary/' \
  | grep -vE '^mikebom-cli/src/parity/extractors/' \
  | grep -vE '^mikebom-cli/src/generate/(cyclonedx|spdx)/' \
  | wc -l
# Expected: 0

# No golden regen on existing 9 ecosystems (SC-007 ≤1-component spurious bound):
git diff --name-only main | grep -E 'mikebom-cli/tests/fixtures/golden/.*\.(cdx|spdx)' \
  | grep -v 'mikebom-cli/tests/fixtures/golden/.*conan'  # any new golden for the 3 new fixtures is OK
# Expected: ≤3 (one per format for the new binary-id fixture if integrated into golden-regen; OR 0 if the new fixtures don't go into the golden harness)
```

## Contract 8 — Pre-PR gate clean (SC-005)

**Verification**:
```bash
./scripts/pre-pr.sh
# Expected: prints `>>> all pre-PR checks passed.`; exit 0.
# All test targets report `0 failed`.
# Three new test files (binary_embedded_version_strings, binary_packer_detection, binary_symbol_fingerprint) are listed in test results.
```

Should run with `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` for SBOM-spec-touching changes per CLAUDE.md convention.
