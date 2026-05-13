# Contract — milestone 097 CPE candidate emission

Five behavioral contracts. Each specifies (a) the invariant and (b) a verification recipe — a unit test name, an integration test, or a `jq`-grep on emitted SBOM JSON.

## Contract 1 — `pkg:generic/<v1-library>@<version>` emits canonical CPE (FR-001, SC-001)

**Path**: `mikebom-cli/src/generate/cpe.rs::synthesize_cpes()` "generic" arm.

**Invariant**: for every PURL of shape `pkg:generic/<slug>@<version>` where `<slug>` matches a row in `GENERIC_LIBRARY_CPES`, mikebom emits:
- `component.cpes[0]` = `"cpe:2.3:a:<vendor>:<product>:<version>:*:*:*:*:*:*:*"` where `(vendor, product)` is the table's first mapping for that slug.
- If the mapping has ≥2 vendor:product pairs, all are emitted into `component.cpes[]` in declaration order.

**Verification**:
```bash
# Unit test (in cpe.rs::tests):
cargo +stable test -p mikebom --bin mikebom \
    --no-fail-fast cpe::tests::generic_openssl_emits_canonical_cpe \
    cpe::tests::generic_curl_emits_dual_candidates 2>&1 | grep "test result:"
# Expected: ok. 2 passed.

# End-to-end via Trivy (toolchain-graceful-skip if absent):
echo '{"purl": "pkg:generic/openssl@3.0.13"}' | ...synthesize SBOM with this component...
trivy sbom /tmp/openssl.cdx.json
# Expected: ≥1 CVE in OpenSSL 3.0.13 advisory list appears in output.
```

## Contract 2 — Missing table entry emits no CPE silently (FR-003)

**Path**: same arm — the `let Some(vendors) = mapping else { return Vec::new(); };` branch.

**Invariant**: when `<slug>` is not present in `GENERIC_LIBRARY_CPES`, `synthesize_cpes` returns an empty Vec. No log line, no error, no panic. The PURL still appears in the SBOM; only the CPE field/external-ref is absent.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    cpe::tests::generic_unknown_library_returns_empty 2>&1 | grep "test result:"
# Expected: ok. 1 passed.

# Plus the existing `unknown_ecosystem_returns_empty` test continues to pass —
# the test is renamed but the assertion is preserved.
```

## Contract 3 — Versionless components emit no CPE (FR-004, SC-003)

**Path**: existing `cpe.rs:25-28` empty-version fast-return (no new code path).

**Invariant**: when `ResolvedComponent.version` is the empty string — the symbol-fingerprint-only milestone-096 case + the SQLite source-id-only edge case — `synthesize_cpes` returns an empty Vec regardless of which ecosystem arm matches.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    cpe::tests::generic_symbol_fingerprint_only_emits_no_cpe \
    cpe::tests::empty_version_returns_empty 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 4 — OpenJDK build-suffix stripping (Edge Cases, FR-001 nuance)

**Path**: same "generic" arm — the `if name == "openjdk"` special-case.

**Invariant**: `pkg:generic/openjdk@21.0.1+12` produces `cpe:2.3:a:oracle:openjdk:21.0.1:*:*:*:*:*:*:*` (build-suffix stripped). The PURL on the component is unchanged (`@21.0.1+12` stays); only the CPE field's version segment is normalized.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    cpe::tests::generic_openjdk_strips_build_suffix 2>&1 | grep "test result:"
# Expected: ok. 1 passed.
```

## Contract 5 — Mapping table well-formed at build time (FR-002 + FR-006)

**Path**: `cpe.rs::tests::mappings_alphabetically_sorted()` + `cpe.rs::tests::mappings_cover_all_curated_libraries()`.

**Invariant**:
1. `GENERIC_LIBRARY_CPES` is sorted alphabetically by `library_slug` for diff-friendliness.
2. Every `version_strings::CuratedLibrary::slug()` value either has a row in `GENERIC_LIBRARY_CPES` or is on the documented-omission list (currently `boringssl` only).
3. Every emitted CPE 2.3 string round-trips through `cpe_escape()` and contains exactly 13 colon-separated segments per the spec.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    cpe::tests::mappings_alphabetically_sorted \
    cpe::tests::mappings_cover_all_curated_libraries 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 6 — Diff scope guardrails (FR-007, FR-008, FR-009, SC-006, SC-007)

**Verification**:
```bash
# No new Cargo deps (FR-007):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
# Expected: 0

# Production code outside generate/cpe.rs:
git diff --name-only main | grep -E '^mikebom-cli/src/' \
  | grep -vE '^mikebom-cli/src/generate/cpe\.rs$' \
  | wc -l
# Expected: 0

# Golden regen scope (FR-009 / SC-006):
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1 | grep -oE '[0-9]+ files? changed'
# Expected: zero (no existing fixture contains a binary-extracted
# pkg:generic/<v1-library>@<version> component per milestone-096 SC-007).

# Diff scope allowlist:
git diff --name-only main | sort
# Expected (allowlist):
#   mikebom-cli/src/generate/cpe.rs
#   mikebom-cli/tests/cpe_binary_id.rs           (NEW)
#   specs/097-cpe-candidates/...
#   CLAUDE.md                                    (auto-updated by /speckit-plan)
```

## Contract 7 — Pre-PR gate clean (SC-005)

**Verification**:
```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: prints `>>> all pre-PR checks passed.`; exit 0.
# Clippy: zero warnings.
# Test suite: every target `0 failed`.
```

The SPDX 3 validator (`spdx3-validate==0.0.5`) accepts standards-native CPE arrays; the new emissions don't break conformance.
