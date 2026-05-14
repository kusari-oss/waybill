# Quickstart — milestone 101 maintainer recipes

Five recipes for landing the Windows smoke test + experimental docs callout. Total estimated implementation time: ~45 minutes single-developer (most of the time is in the CI lap to verify the smoke step blocks correctly).

## Recipe 1 — Write the smoke test (FR-001..004, FR-011, FR-012)

Create `mikebom-cli/tests/scan_windows_smoke.rs` from the full code in `data-model.md §scan_windows_smoke.rs` (~150 lines). Key shape:

```rust
#![cfg(windows)]
#![allow(clippy::unwrap_used)]
// ... two #[test] fns: smoke_cargo_fixture, smoke_polyglot_monorepo
// ... helpers: run_scan_with_timeout, walk_for_backslash_in_path_fields, diagnose_and_panic
```

Verify on macOS dev host that the file compiles to nothing (the `#[cfg(windows)]` at the file top means the whole crate is empty on Unix):

```bash
cargo +stable test --test scan_windows_smoke 2>&1 | grep "test result:"
# Expected on macOS/Linux: ok. 0 passed; 0 failed; 0 ignored;
```

Then verify the broader pre-PR gate still passes:

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`
```

## Recipe 2 — Split the Windows CI lane test step (FR-008)

Open `.github/workflows/ci.yml`. Find the `lint-and-test-windows` job (around line 258); locate the existing `Tests (non-blocking, see issue #210)` step (around line 289). Replace it with two steps per `data-model.md §ci.yml`:

```yaml
      - name: Smoke test (blocking — milestone 101)
        run: cargo +stable test --test scan_windows_smoke

      - name: Tests (non-blocking, see issue #210)
        continue-on-error: true
        run: cargo +stable test --workspace
```

Smoke step runs FIRST. Validate locally with `actionlint` or visual diff against the macOS lane (the smoke step is the only one new).

## Recipe 3 — Update README.md (FR-005, FR-007)

Open `README.md`. Two changes:

**1. Platform-support table cell** (line ~272 post-milestone-100):

```markdown
| Windows x86_64    | 🧪 experimental (milestone 100, [#210](https://github.com/kusari-sandbox/mikebom/issues/210)) | ❌ |
```

(replaces `✅ supported (milestone 100)`).

**2. Insert experimental callout** at the top of the "Windows install" subsection (line ~280-ish):

```markdown
### Windows install

> 🧪 **Experimental.** Windows builds are available as of milestone
> 100, but are not feature-equivalent to Linux/macOS yet. Known gaps:
> Linux-only OS package readers (dpkg/rpm/apk), HOME-env-var-derived
> cache paths, OCI image cache atomic-rename, path-resolver pattern
> matcher, and Python stdlib collapse. Full Windows runtime test
> parity + production-code fixes are tracked in
> [#210](https://github.com/kusari-sandbox/mikebom/issues/210).
> Do not rely on the Windows binary for production SBOM workflows
> until #210 closes.

Download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the
[latest release]( ... )
```

(the existing download instructions stay; the callout goes ABOVE them.)

## Recipe 4 — Update docs/user-guide/installation.md (FR-006)

Open `docs/user-guide/installation.md`. Insert the SAME canonical callout in the platform-support / Windows section (verify the section exists; if not, add a "Windows install" heading + the callout + the same download/usage instructions as README).

The callout's content body MUST be byte-identical to README's (modulo blockquote-prefix whitespace if the docs site renders blockquotes differently).

Cross-file verification:

```bash
diff <(sed -n '/🧪 \*\*Experimental/,/until #210 closes/p' README.md) \
     <(sed -n '/🧪 \*\*Experimental/,/until #210 closes/p' docs/user-guide/installation.md)
# Expected: empty diff.
```

## Recipe 5 — Verify diff scope + open PR

Run the diff-scope guardrails per `contracts/smoke-test-contracts.md §Contract 9`:

```bash
git diff --name-only main | sort
# Expected:
#   .github/workflows/ci.yml
#   README.md
#   docs/user-guide/installation.md
#   mikebom-cli/tests/scan_windows_smoke.rs
#   (+ CLAUDE.md if /plan auto-updated it)
#   (+ specs/101-windows-smoke-experimental/...)

git diff --name-only main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$'
# Expected: empty
```

Run the pre-PR gate one final time, then open the PR. The PR description should mention:
- The smoke test exercises cargo + pypi + npm.
- 60-second per-scan timeout (FR-011).
- Inline diagnostics + actual.cdx.json on failure (FR-012).
- Issue #210 still tracks the remaining backlog.
- The Windows lane's smoke step is the new blocking gate.

## When in doubt

- **Test doesn't run on Windows CI** — check the `#[cfg(windows)]` at the file top. If you scoped it per-fn instead, the `#![cfg(windows)]` at file top is the cleaner form (whole file vanishes on non-Windows).
- **`CARGO_BIN_EXE_mikebom` is unset** — confirm `mikebom-cli/Cargo.toml` has `[[bin] name = "mikebom" ...]`. Cargo only sets this env for integration tests of bin-target crates.
- **Smoke test on macOS shows 0 tests** — that's correct; the `#[cfg(windows)]` gate is intentional. Use the existing `cdx_regression_cargo` for Unix forward-slash coverage.
- **Backslash hit on a CPE string** — bug in `walk_for_backslash_in_path_fields`: it must check `name in PATH_FIELD_NAMES` BEFORE inspecting `value`. CPE strings are values of `name = "cpe"` not in the list; they shouldn't be reached.
- **60-second timeout fires falsely on a slow runner** — Windows runners CAN be slow on first-run (cold cargo cache). If this becomes a regression-class problem, raise to 120s; but milestone 100's full `cargo test --workspace` took ~9 min and individual tests are sub-second, so 60s should be plenty.
- **The smoke step's cargo target tree is empty** — happens if you ran clippy with no test compile; cargo's `test --test` should rebuild as needed. If problematic, add `cargo +stable build --tests --target x86_64-pc-windows-msvc` as a no-op-but-warm step before the smoke step.
- **CI #210 work catches up to milestone 101** — when #210 closes and the test step's `continue-on-error: true` is dropped, the smoke step is still valuable as a fast-fail gate. Don't delete it; just stop relying on it as the only Windows gate.
