# Quickstart — milestone 100 maintainer recipes

Six recipes for landing the Windows-host build/run support. Total estimated implementation time: ~3-5 hours single-developer (mostly waiting on CI feedback during the audit + bring-up).

## Recipe 1 — Add the `normalize_sbom_path` helper (FR-004 / SC-003)

Create `mikebom-cli/src/scan_fs/sbom_path.rs` with the two pub fns + 3 unit tests per `data-model.md §sbom_path.rs`. Add `pub mod sbom_path;` to `mikebom-cli/src/scan_fs/mod.rs`. Confirm:

```bash
cargo +stable test -p mikebom --bin mikebom \
    --no-fail-fast scan_fs::sbom_path::tests 2>&1 | grep "test result:"
# Expected: ok. 3 passed.
```

## Recipe 2 — Wire the chokepoint (FR-004)

Find the two `ResolvedComponent` builder sites in `mikebom-cli/src/scan_fs/mod.rs` (around lines 167 + 542). Each currently does:

```rust
source_file_paths: vec![entry.source_path.clone()],
```

Change to:

```rust
source_file_paths: vec![
    crate::scan_fs::sbom_path::normalize_sbom_path_str(&entry.source_path),
],
```

Same site has a `source_path: entry.source_path.clone()` line on `ResolvedComponent` itself — apply the same normalization there for consistency.

Also wire the 3 defensive-normalization sites at:
- `mikebom-cli/src/generate/cyclonedx/evidence.rs:84`
- `mikebom-cli/src/generate/spdx/annotations.rs:~260`
- `mikebom-cli/src/generate/spdx/v3_annotations.rs:~272`

Each wraps the path-emitting expression with `normalize_sbom_path_str(...)`. Per `data-model.md §CDX / SPDX 2.3 / SPDX 3 emission sites`.

Compile-check: `cargo +stable check -p mikebom`.

Run existing goldens regression on Linux/macOS — expect zero-diff (forward-slash was already the native format; the no-op cfg branch on Unix means output bytes are identical):

```bash
cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression 2>&1 | grep "test result:"
# Expected: ok. 9 passed. (3 tests × 3 files = 9 ecosystems)
```

## Recipe 3 — Add the CI Windows lane (FR-008)

Open `.github/workflows/ci.yml`. After the `lint-and-test-macos` job's last step (line ~245), add the `lint-and-test-windows` job per `research.md §3` + `data-model.md §.github/workflows/ci.yml`.

Verify locally with `actionlint` or by visual diff against the macOS job — they should differ only in `runs-on:` and the job name.

After commit + push, the new lane will fire on the next CI run. Expect potential failures:
- POSIX-only tests that don't have `#[cfg(unix)]` gates → add gates per Recipe 4.
- Tests asserting backslash-free output that now see normalized forward-slash → expected (the normalization works).
- Tests asserting forward-slash output that incorrectly assumed always-Unix → fix the test.

## Recipe 4 — Audit + gate POSIX-only tests (FR-002 / FR-010)

The Windows CI lane bring-up will surface failures. For each:

```rust
// Before — Windows-incompatible:
#[test]
fn my_symlink_test() {
    std::os::unix::fs::symlink(target, link).unwrap();
    // ... assertions
}

// After:
#[cfg(unix)]
#[test]
fn my_symlink_test() {
    std::os::unix::fs::symlink(target, link).unwrap();
    // ... assertions
}
```

Pre-known candidates (per `research.md §6`): tests under `tests/filesystem_walker_*.rs` (milestone 054 symlink-loop tests). Audit at implementation time + gate as needed.

For tests that hardcode Unix paths (`/bin/ls`, etc.) but don't fail on Windows because the path-absence check returns empty: leave them as-is.

## Recipe 5 — Add the release Windows build job (FR-009)

Open `.github/workflows/release.yml`. After `build-macos-aarch64` (line ~250), add `build-windows-x86_64` per `research.md §4` + `data-model.md §.github/workflows/release.yml`.

Update the `release` aggregation job's `needs:` array (currently at line ~250):

```yaml
# Before:
needs: [build-linux-x86_64, build-linux-aarch64, build-macos-aarch64]

# After:
needs: [build-linux-x86_64, build-linux-aarch64, build-macos-aarch64, build-windows-x86_64]
```

Verify locally with `actionlint`. After commit, the next tag push will exercise the new build job.

## Recipe 6 — Update README + run pre-PR gate + verify diff scope

Add the Windows install + usage section to README.md per `data-model.md §README.md`.

Run the pre-PR gate on Linux/macOS:

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`
```

Verify diff scope (Contract 8):

```bash
git diff --name-only main | sort
# Expected:
#   .github/workflows/ci.yml
#   .github/workflows/release.yml
#   CLAUDE.md                                              (auto-updated)
#   README.md
#   mikebom-cli/src/generate/cyclonedx/evidence.rs
#   mikebom-cli/src/generate/spdx/annotations.rs
#   mikebom-cli/src/generate/spdx/v3_annotations.rs
#   mikebom-cli/src/scan_fs/mod.rs
#   mikebom-cli/src/scan_fs/sbom_path.rs                   (NEW)
#   specs/100-windows-host-build/...
#   (optional) tests/filesystem_walker_*.rs if gates added

git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' && echo "DEP CHURN" || echo "clean"
# Expected: clean

git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty (no goldens regenerated)
```

## When in doubt

- **A test asserts backslash output and now fails on Windows**: that test was asserting non-normalized output. After milestone 100, ALL SBOM-emitted paths are forward-slash. Fix the assertion.
- **A test hardcodes a Unix path that doesn't exist on Windows**: graceful-skip (existing pattern — `if !path.exists() { eprintln!("skipping"); return; }`). Avoid `#[cfg(unix)]` unless the test asserts POSIX-specific behavior.
- **`std::os::unix::fs::symlink` used unconditionally**: wrap test in `#[cfg(unix)]`. Symlink creation on Windows requires admin/developer-mode privileges; CI runners don't have those.
- **Drive-letter prefix preservation**: `C:\foo` → `C:/foo` is correct. The colon stays; only `\` → `/`. Verify by `assert_eq!(normalize_sbom_path_str(r"C:\foo"), "C:/foo")` on a Windows host.
- **Backslash inside a filename** (extremely rare on Windows, technically not allowed but possible on some filesystems): the `replace('\\', '/')` will normalize them too. Acceptable lossy behavior — operators with this edge case are already outside the spec.
- **PowerShell vs Git Bash on the Windows runner**: the release job uses `shell: pwsh` explicitly. The CI lint+test job uses default shell which on `windows-latest` is PowerShell — `cargo` runs the same regardless.
- **Cache key collisions across host OSes**: `actions/cache` uses `${{ runner.os }}` in the key, so Windows + Linux + macOS have isolated caches. No collision.
