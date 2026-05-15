# Quickstart — milestone 102 maintainer recipes

Six recipes for landing the C/C++ source-tree readers. Total estimated implementation time: ~4-6 hours single-developer.

## Recipe 1 — Implement the 4 readers (US1, US2, US3)

Each reader is independent — implement in any order. Recommended order:

1. **`vcpkg.rs` first** (simplest, ~150 lines). serde-deserialize `vcpkg.json`, emit `pkg:vcpkg/...` components. Verify with `cargo +stable test --test scan_vcpkg`.
2. **`conan.rs` second** (~200 lines, two sub-parsers). conanfile.txt INI-style line parser + conanfile.py regex extractor. Verify with `cargo +stable test --test scan_conan`.
3. **`bazel.rs` third** (~280 lines, two parsers). MODULE.bazel `bazel_dep` regex + WORKSPACE.bazel `http_archive`/`git_repository` regexes. Verify with `cargo +stable test --test scan_bazel`.
4. **`cmake.rs` last** (~350 lines, three sub-parsers + vendored gating). Most complex; benefits from the regex patterns settled in bazel.rs first. Verify with `cargo +stable test --test scan_cmake` AND `cargo +stable test --test scan_cmake_vendored`.

For each reader: follow the `cargo.rs` template — public `pub fn read(scan_root: &Path) -> (Vec<PackageDbEntry>, Vec<ParseErrorAnnotation>)`. Use `regex::Regex` with `(?ms)` flags for multi-line patterns. Use `encode_purl_segment()` + `Purl::new()` for PURL construction. Set `extra_annotations` for `mikebom:download-url` / `mikebom:vendored` / `mikebom:bazel-archive-name`.

After each reader: `cargo +stable clippy -p mikebom --all-targets -- -D warnings` to keep the lint gate green.

## Recipe 2 — Wire readers into scan_fs::scan_path() dispatch

Modify `mikebom-cli/src/scan_fs/mod.rs`'s `scan_path()` to call the 4 new readers alongside the existing 11. Pass `include_vendored` through from `scan_cmd::execute()`. Aggregate parse-errors into the scan-summary `mikebom:parse-error` annotation.

Verify with `cargo +stable test -p mikebom --bin mikebom --no-fail-fast` — all 1400+ existing tests still pass.

## Recipe 3 — Add `--include-vendored` CLI flag

Modify `mikebom-cli/src/cli/scan_cmd.rs`:
1. Add `include_vendored: bool` field to `ScanArgs` with `#[arg(long, env = "MIKEBOM_INCLUDE_VENDORED")]`.
2. Pass through to `execute()` as a new parameter.
3. Plumb to `scan_fs::scan_path()`.

Verify with: `cargo +stable build -p mikebom && ./target/debug/mikebom sbom scan --help | grep vendored` → flag appears in help output.

## Recipe 4 — Generate the 12 goldens (4 ecosystems × 3 formats)

Once the 4 readers + integration tests pass, regenerate the goldens:

```bash
MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
  cargo +stable test -p mikebom \
    --test cdx_regression --test spdx_regression --test spdx3_regression
```

This writes 12 NEW goldens under `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{bazel,cmake,vcpkg,conan}.*`. Existing 9 ecosystems' goldens MUST stay untouched (verify via `git diff --stat`).

Then re-run without the env vars to confirm byte-identity locks in:

```bash
cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
  2>&1 | grep "test result:"
# Expected: ok. 13 passed (9 old + 4 new) on each format.
```

## Recipe 5 — Update README + CLI reference docs

`README.md`: add 4 rows to the "Supported ecosystems" table covering Bazel, CMake, vcpkg, Conan with manifest paths.

`docs/user-guide/cli-reference.md`: add a `--include-vendored` section per FR-017. Must cover: default-OFF, what counts as vendored (third_party/ + vendor/), false-positive risks, `version.txt` backfill.

## Recipe 6 — Pre-PR gate + diff-scope audit + open PR

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`

# Diff scope (Contract 12):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$' | wc -l
# Expected: 0

git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.cdx.json | tail -1
# Expected: empty (existing 9 ecosystems unchanged).
```

Open PR with title `feat(102): C/C++ source-tree readers (Bazel + CMake + vcpkg + Conan)`.

## When in doubt

- **CMake regex matches but extracts wrong field** — the rule's keyword ordering in real CMakeLists.txt files varies (`GIT_REPOSITORY ... GIT_TAG ...` vs `GIT_TAG ... GIT_REPOSITORY ...`). Use named captures and check both orderings.
- **vcpkg.json has `"version>="` as a JSON key** — special characters in keys are valid JSON; use `#[serde(rename = "version>=")]` on the struct field.
- **conanfile.py has `requires = base_reqs + ["zlib/1.2.13"]`** — non-literal; the regex won't catch it. Documented in SC-005's 80% floor; skip silently.
- **A Bazel dep appears in both MODULE.bazel and WORKSPACE.bazel with different versions** — MODULE.bazel wins per Contract 3. Implement the dedup in `bazel.rs::dedup_module_wins()`.
- **`add_subdirectory(third_party/foo)` is the project's own monorepo module, not a vendored dep** — false-positive risk. The `--include-vendored` flag's default-OFF design protects against this; document the risk in cli-reference.md.
- **Cross-ecosystem dedup looks wrong** — re-read Q2's clarification + FR-010. vcpkg openssl + Conan openssl emit as TWO separate components (different ecosystems = different package-manager sources). This is intentional, not a bug.
- **Parse-error annotation isn't appearing in the SBOM** — check that `scan_fs::mod.rs::scan_path()` is collecting `Vec<ParseErrorAnnotation>` from each reader and surfacing as a `metadata.properties[]` entry named `mikebom:parse-error`.
