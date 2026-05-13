# Contract — milestone 100 Windows-host build + run support

Eight behavioral contracts. Each specifies the invariant and a verification recipe.

## Contract 1 — Cargo build succeeds on Windows (FR-001 / SC-004)

**Path**: workspace root.

**Invariant**: `cargo +stable build --target x86_64-pc-windows-msvc -p mikebom` exits 0 on a `windows-latest` runner with the workspace's stable toolchain. Zero new Cargo deps added.

**Verification**:
```yaml
# In ci.yml lint-and-test-windows job:
- name: Build (implicit via clippy)
  run: cargo +stable clippy --workspace --all-targets -- -D warnings
# clippy includes a build step under the hood; if build fails, clippy fails.
```

## Contract 2 — Cargo test passes on Windows (FR-002 / SC-005)

**Path**: workspace root.

**Invariant**: `cargo +stable test --workspace` on `windows-latest` reports every target as `0 failed`. POSIX-only tests gracefully skip (existing skip pattern) or are `#[cfg(unix)]`-gated.

**Verification**:
```yaml
- name: Tests
  run: cargo +stable test --workspace
```

## Contract 3 — Path normalization on Windows (Clarifications Q1 / FR-004 / SC-003)

**Path**: `mikebom-cli/src/scan_fs/sbom_path.rs::normalize_sbom_path` + the 4 chokepoint/emission-site call sites.

**Invariant**: every path string emitted into SBOM JSON (`evidence.occurrences[].location` in CDX, the equivalent SPDX 2.3 annotation comment, the equivalent SPDX 3 statement) uses forward-slash separators, regardless of host OS. On Windows, backslashes in source paths are replaced with forward-slashes before emission. Drive-letter prefixes (`C:`) are preserved verbatim.

**Verification** (unit + integration):

```bash
# Unit tests:
cargo +stable test -p mikebom --bin mikebom \
    --no-fail-fast scan_fs::sbom_path::tests 2>&1 | grep "test result:"
# Expected: ok. 3 passed.

# Integration smoke (run on Windows CI lane):
# Scan a directory containing Cargo.toml. Inspect emitted CDX:
mikebom.exe sbom scan --path . --output out.cdx.json
# Assert no backslashes in path-string fields:
findstr /v "\\\\" out.cdx.json   # PowerShell equivalent: -notmatch '\\'
# Expected: no JSON lines should contain backslash-escaped backslashes
# inside path-shaped fields (location, source_path, etc.).
```

## Contract 4 — Cross-host SBOM byte-identity (SC-003)

**Path**: same as Contract 3 + the existing cross-host normalize helper in the goldens test harness.

**Invariant**: scanning the same input files on Linux + macOS + Windows hosts produces SBOMs whose component sets are identical and whose path strings are forward-slash everywhere. The only legitimate cross-host difference is the workspace-root prefix (e.g., `/runner/_work/...` vs `C:/runners/_work/...`), which the existing goldens normalize helper strips.

**Verification**: the byte-identity goldens regression tests (`cdx_regression.rs`, `spdx_regression.rs`, `spdx3_regression.rs`) MUST pass on the Windows CI lane without regenerating the committed goldens (the goldens are forward-slash today on Linux/macOS; Windows-host output after milestone-100 normalization matches that format).

```yaml
# Implicit in `cargo test` on the Windows lane:
- name: Tests
  run: cargo +stable test --workspace
# If the goldens harness's workspace-path-strip helper fails to handle
# the Windows-style ROOT prefix, the regression tests will fail. The
# implementer extends the helper as needed during T-bringup.
```

## Contract 5 — Linux-only readers compile + no-op on Windows (FR-007)

**Path**: `scan_fs/package_db/{dpkg,rpm,apk}.rs`, `scan_fs/docker_image.rs`, `scan_fs/oci_pull/`.

**Invariant**: these readers compile on Windows hosts (their POSIX-specific code paths are `#[cfg(unix)]`-gated correctly) and silently return empty results when invoked on a Windows scan (their target files like `/var/lib/dpkg/status` don't exist there).

**Verification**:
```bash
# Compile check: clippy passes on Windows (Contract 1).
# Runtime check: a Windows-host scan of a Rust project doesn't emit
# any pkg:deb / pkg:rpm / pkg:apk components.
mikebom.exe sbom scan --path C:\path\to\rust-project --output out.cdx.json
findstr "pkg:deb" out.cdx.json && exit 1 || echo "clean"
findstr "pkg:rpm" out.cdx.json && exit 1 || echo "clean"
findstr "pkg:apk" out.cdx.json && exit 1 || echo "clean"
# Expected: all 3 grep negate-clean.
```

## Contract 6 — Windows CI lane exists and runs (FR-008 / SC-004 / SC-005)

**Path**: `.github/workflows/ci.yml::lint-and-test-windows`.

**Invariant**: the `ci.yml` workflow defines a `lint-and-test-windows` job that runs on `windows-latest` runners, executes `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace`, runs in parallel with the existing Linux + macOS lanes.

**Verification**:
```bash
grep -nE 'lint-and-test-windows|runs-on: windows-latest' \
    .github/workflows/ci.yml | head -3
# Expected: 2-3 matches naming the job + the runner.

# CI run verification (after PR opens):
gh run list --workflow=ci.yml --limit 1 --json conclusion,status,jobs \
    --jq '.[0].jobs[] | select(.name | contains("windows"))'
# Expected: at least one matching job entry; conclusion = success on
# the PR's CI run.
```

## Contract 7 — Windows release artifact (FR-009 / SC-006)

**Path**: `.github/workflows/release.yml::build-windows-x86_64` + `release` aggregation job.

**Invariant**: the `release.yml` workflow's `build-windows-x86_64` job produces a `mikebom-v<version>-x86_64-pc-windows-msvc.zip` artifact containing `mikebom.exe`, uploads it to the GitHub pre-release, and its SHA-256 appears in the published `SHA256SUMS` file. The `release` aggregation job's `needs:` includes `build-windows-x86_64`.

**Verification** (post-merge, on first alpha tag push):
```bash
gh release view v0.1.0-alpha.<N> --json assets --jq '.assets[].name'
# Expected: mikebom-v0.1.0-alpha.<N>-x86_64-pc-windows-msvc.zip
#           alongside the existing 3 tarballs + SHA256SUMS.

# SHA256SUMS includes the new artifact:
gh release download v0.1.0-alpha.<N> --pattern SHA256SUMS
grep 'x86_64-pc-windows-msvc.zip' SHA256SUMS
# Expected: one match with hash.
```

## Contract 8 — Diff scope guardrails (FR-005 / SC-007 / SC-008)

**Verification**:
```bash
# No new Cargo deps (FR-005 / SC-007):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
# Expected: 0

# Production code outside scan_fs/ + generate/ + workflows + docs:
git diff --name-only main | grep -E '^mikebom-cli/src/' \
  | grep -vE '^mikebom-cli/src/scan_fs/' \
  | grep -vE '^mikebom-cli/src/generate/' \
  | wc -l
# Expected: 0 (unless symlink-test gating in `tests/` is needed, which
# is a test-tree edit not src-tree)

# Golden regen scope (SC-008 = no schema changes):
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty (forward-slash was already the format on Linux/macOS;
# no goldens regenerate). If the existing workspace-path-strip helper
# needs an update, the diff is in the helper only, not the goldens
# themselves.
```

## Contract 9 — Pre-PR gate clean on Linux/macOS unchanged (SC-005 baseline)

**Path**: `./scripts/pre-pr.sh` on Linux/macOS dev hosts.

**Invariant**: the existing pre-PR gate continues to pass on Linux/macOS post-milestone-100. The Windows-specific changes (new CI lane, new release job, path-normalization helper) don't break Linux/macOS behavior. Forward-slash normalization on Unix is a no-op `String::to_string()` so output is bytewise unchanged.

**Verification** (run on macOS dev host):
```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`
```

## Contract 10 — README documents Windows install + usage (FR-011)

**Verification**:
```bash
grep -n 'Windows\|windows-msvc\|\.exe' README.md | head -5
# Expected: ≥3 matches across "Windows install" and "Windows usage" sections.
```
