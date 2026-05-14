# Contract — milestone 101 Windows smoke test + experimental docs callout

Eight behavioral contracts. Each specifies the invariant and a verification recipe.

## Contract 1 — Smoke test exits 0 on both fixtures (FR-001 / SC-002)

**Path**: `mikebom-cli/tests/scan_windows_smoke.rs`.

**Invariant**: `cargo +stable test --test scan_windows_smoke` on `windows-latest` reports `ok. 2 passed; 0 failed; 0 ignored`. Both `smoke_cargo_fixture` and `smoke_polyglot_monorepo` invoke `mikebom.exe sbom scan` against their respective fixtures and the subprocess exits 0.

**Verification**:
```bash
# On windows-latest CI runner:
cargo +stable test --test scan_windows_smoke 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 2 — Emitted SBOM is well-formed CycloneDX 1.6 (FR-002)

**Invariant**: for every smoke-test scan, the emitted file parses as JSON, has `bomFormat == "CycloneDX"`, has `specVersion == "1.6"`, and has a non-empty `components[]` array.

**Verification**: in-test, via `serde_json::from_str` + explicit field checks (see `scan_windows_smoke.rs::run_smoke_case`).

## Contract 3 — Per-ecosystem PURL coverage (FR-002)

**Invariant**:
- The cargo-fixture scan produces ≥1 component with `purl` starting with `pkg:cargo/`.
- The polyglot-fixture scan produces ≥1 component with `pkg:pypi/` AND ≥1 with `pkg:npm/`.

**Verification**: in-test, via the `expected_purl_prefixes` parameter passed to `run_smoke_case`.

## Contract 4 — Path-shaped fields have zero backslashes (FR-003 / SC-005 of milestone 100)

**Path**: every `mikebom:source-files` property value, `mikebom:source-path` property value, and `location` field value in the emitted SBOM.

**Invariant**: zero literal `\` characters anywhere in path-shaped field values, confirming milestone-100's `normalize_sbom_path_str` is in effect at runtime.

**Verification**: in-test, via `walk_for_backslash_in_path_fields(&sbom)` returning an empty `Vec`. On failure, diagnostic output names the offending field + value.

**Edge case (preserved from milestone 100)**: CPE 2.3 escape sequences (`cpe:2.3:a:github.com\/davecgh\/go-spew:...`) contain literal backslashes by spec. The walker is scoped to PATH-shaped fields only (via `PATH_FIELD_NAMES`), so CPE strings are NOT scanned and don't false-positive.

## Contract 5 — 60-second per-scan hang detection (FR-011)

**Path**: `scan_windows_smoke.rs::run_scan_with_timeout`.

**Invariant**: if `mikebom.exe sbom scan` does not exit within 60 seconds, the test kills the subprocess via `taskkill /F /PID` and panics with the message "mikebom.exe sbom scan timed out — likely hang regression". The kill thread is spawned at scan start and fires after exactly 60 seconds; the test detects the timeout via elapsed-time check (`elapsed > 58s`).

**Verification** (impossible to test directly without a deliberate hang regression; document the expected behavior):
- Normal scan completes in <5 seconds → no timeout.
- Hung scan (e.g., milestone-054 symlink-loop class) → timeout fires at 60s, test panics with the hang message instead of waiting for the GitHub Actions job-level 6-hour timeout.

## Contract 6 — Failure diagnostics include actual.cdx.json + inline component list (FR-012)

**Path**: `scan_windows_smoke.rs::diagnose_and_panic`.

**Invariant**: on any assertion failure (envelope mismatch, missing PURL prefix, backslash in path field), the test:
1. Writes the full emitted SBOM to `<tempdir>/mikebom-smoke-<label>-<pid>.cdx.json`.
2. Prints `--- SMOKE FAILURE [<label>] ---` to stderr.
3. Prints the failure message describing what was asserted.
4. Prints the first 10 component PURLs from the emitted SBOM.
5. Prints the absolute path to the `actual.cdx.json` file.
6. Panics with `smoke test [<label>] failed`.

This matches the existing `cdx_regression.rs` diagnostic pattern.

## Contract 7 — CI smoke step blocks merge; workspace step doesn't (FR-008 / SC-001 / SC-002)

**Path**: `.github/workflows/ci.yml::lint-and-test-windows`.

**Invariant**:
- A `Smoke test (blocking — milestone 101)` step exists, runs `cargo +stable test --test scan_windows_smoke`, has NO `continue-on-error:` attribute.
- A separate `Tests (non-blocking, see issue #210)` step runs `cargo +stable test --workspace` with `continue-on-error: true`.
- The smoke step runs BEFORE the workspace step (sequenced for cargo-cache warmth).

**Verification**:
```bash
grep -nE 'Smoke test \(blocking|Tests \(non-blocking' .github/workflows/ci.yml | head -4
# Expected: 2 matches in that order (smoke first).

grep -A2 'Smoke test (blocking' .github/workflows/ci.yml | grep -c 'continue-on-error'
# Expected: 0 (smoke step is blocking).

grep -A2 'Tests (non-blocking' .github/workflows/ci.yml | grep -c 'continue-on-error: true'
# Expected: 1.
```

**Post-merge CI verification**: open a deliberately-failing test PR (similar to the inspector test we ran on PR #211); the Windows lane reports FAILURE on the smoke step and the merge button is disabled.

## Contract 8 — Documentation says "experimental" with #210 link (FR-005 / FR-006 / FR-007 / SC-003 / SC-004)

**Path**: `README.md` + `docs/user-guide/installation.md`.

**Invariant**:
- `README.md` contains a `🧪 **Experimental.**` blockquote callout in the "Windows install" subsection's first paragraph.
- The callout lists the known-gap categories (Linux-only OS package readers, HOME env-var derivation, OCI cache, path-resolver matcher, Python stdlib collapse).
- The callout links to `#210` (GitHub auto-links by issue number).
- The callout says "Do not rely on the Windows binary for production SBOM workflows."
- The platform-support table's Windows row reads `🧪 experimental (milestone 100, ...#210)`.
- `docs/user-guide/installation.md` contains the SAME callout content (byte-for-byte for the callout block; surrounding prose may vary).

**Verification**:
```bash
grep -n '🧪 \*\*Experimental' README.md docs/user-guide/installation.md
# Expected: ≥1 match in each file.

grep -n '#210\|issues/210' README.md docs/user-guide/installation.md
# Expected: ≥1 match in each file.

# Cell-text verification:
grep '| Windows x86_64' README.md
# Expected: contains '🧪 experimental' (not '✅ supported').

# Cross-file callout consistency (modulo whitespace):
diff <(grep -A8 '🧪 \*\*Experimental' README.md) \
     <(grep -A8 '🧪 \*\*Experimental' docs/user-guide/installation.md)
# Expected: trivial / no diff in callout content.
```

## Contract 9 — Diff scope guardrails (FR-009 / FR-010 / SC-008)

**Verification**:
```bash
# Exactly 4 files: 1 NEW test + 3 MODIFIED.
git diff --name-only main | sort
# Expected:
#   .github/workflows/ci.yml
#   README.md
#   docs/user-guide/installation.md
#   mikebom-cli/tests/scan_windows_smoke.rs

# Optional CLAUDE.md if /speckit-plan's update-agent-context hook fires:
# specs/101-windows-smoke-experimental/...  (spec dir, expected)
# CLAUDE.md (auto-updated by /plan, expected)

# Zero Cargo.* churn:
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$' | wc -l
# Expected: 0

# Zero production-code changes:
git diff --name-only main | grep -E '^mikebom-cli/src/' | wc -l
# Expected: 0

# Zero goldens regenerated:
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty
```

## Contract 10 — Pre-PR gate clean on Linux/macOS (SC-006)

**Path**: `./scripts/pre-pr.sh` on Linux/macOS dev hosts.

**Invariant**: existing pre-PR gate continues to pass post-milestone-101. Smoke test compiles to empty on non-Windows (its `#[cfg(windows)]` gate at the file-top level makes the integration-test binary empty); CI YAML changes only the Windows job; docs changes don't affect any code path.

**Verification** (on macOS dev host):
```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`
```
