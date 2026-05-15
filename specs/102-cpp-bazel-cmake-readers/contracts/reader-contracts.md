# Contract — milestone 102 C/C++ source-tree readers

12 behavioral contracts. Each specifies the invariant and a verification recipe.

## Contract 1 — Bazel MODULE.bazel produces `pkg:bazel/...` (FR-002 / SC-001)

**Path**: `mikebom-cli/src/scan_fs/package_db/bazel.rs::read`.

**Invariant**: every `bazel_dep(name = "X", version = "Y")` in `MODULE.bazel` produces exactly one `pkg:bazel/X@Y` component. `dev_dependency = True` sets `LifecycleScope::Development` (→ CDX `scope = "excluded"` per existing milestone-052 mapping; SPDX 2.3 `DEV_DEPENDENCY_OF`).

**Verification**:
```bash
cargo +stable test --test scan_bazel 2>&1 | grep "test result:"
# Expected: ok. N passed (where N covers the 2 fixture bazel_deps).
```

## Contract 2 — Bazel WORKSPACE.bazel produces components with URL + sha256 (FR-003 / FR-004)

**Invariant**: `http_archive(name = N, urls = [U], sha256 = S)` produces a component with:
- PURL = `pkg:bazel/N@<version-from-url-or-"unknown">`
- `mikebom:download-url = U` annotation
- `mikebom:bazel-archive-name = N` annotation
- `hashes[]` entry `{algorithm: "SHA-256", value: S}` (when declared)

**Verification**: `scan_bazel.rs` test asserts the emitted SBOM contains the http_archive component with the correct `mikebom:download-url` value + SHA-256 hash.

## Contract 3 — Bazel MODULE.bazel wins on version conflict with WORKSPACE.bazel (Edge Cases)

**Invariant**: when `bazel_dep(name = X, version = A)` in MODULE.bazel and `http_archive(name = X, urls = [...with-different-version-B...])` in WORKSPACE.bazel both declare the same name `X`, the emitted component carries version `A` (MODULE wins). Bzlmod is authoritative for Bazel 7+.

**Verification**: synthetic fixture extension covers this case; assertion in `scan_bazel.rs`.

## Contract 4 — CMake FetchContent GIT_REPOSITORY produces pkg:github when applicable (FR-006)

**Invariant**: `FetchContent_Declare(<name> GIT_REPOSITORY https://github.com/<owner>/<repo>.git GIT_TAG <tag>)` emits component with PURL `pkg:github/<owner>/<repo>@<tag>`. Non-github GIT_REPOSITORY URLs emit `pkg:generic/<name>@<tag>` with `mikebom:download-url`.

**Verification**: `scan_cmake.rs` asserts the googletest fixture component has PURL `pkg:github/google/googletest@release-1.14.0`.

## Contract 5 — CMake ExternalProject_Add URL+URL_HASH produces sha256 (FR-006)

**Invariant**: `ExternalProject_Add(<name> URL ... URL_HASH SHA256=<digest>)` emits component with `hashes[]` containing the declared digest and `mikebom:download-url` pointing at the URL.

**Verification**: `scan_cmake.rs` asserts the zlib fixture component carries the expected SHA-256.

## Contract 6 — CMake `cmake/*.cmake` subdirectory walks discover included declarations (FR-005)

**Invariant**: when a `CMakeLists.txt` does `include(cmake/third_party.cmake)` and `third_party.cmake` contains a `FetchContent_Declare`, the dep surfaces in the SBOM with `mikebom:source-files = ["cmake/third_party.cmake"]`.

**Verification**: `scan_cmake.rs` fixture exercises this; assertion on source-files property value.

## Contract 7 — vcpkg.json `dependencies[]` produces `pkg:vcpkg/` components (FR-007 / SC-003)

**Invariant**: every entry in `dependencies` (both string-form and object-form) emits exactly one `pkg:vcpkg/<name>[@<version>]` component. The object-form's `version>=` populates the version segment.

**Verification**: `scan_vcpkg.rs` asserts the 2 expected components from the fixture.

## Contract 8 — conanfile.txt `[requires]` + `[tool_requires]` produce `pkg:conan/` components with right scope (FR-008)

**Invariant**: lines under `[requires]` emit components with default scope (Runtime). Lines under `[tool_requires]` emit with `LifecycleScope::Build` (→ CDX `scope = "excluded"` + SPDX 2.3 `BUILD_DEPENDENCY_OF`).

**Verification**: `scan_conan.rs` asserts: 2 `pkg:conan/zlib`/`pkg:conan/openssl` components with no scope; 1 `pkg:conan/cmake@3.27.0` with `Build` scope.

## Contract 9 — `--include-vendored` flag gates `add_subdirectory(third_party/...)` (FR-016)

**Invariant**:
- Default (`--include-vendored` not set, env unset): NO components emit from `add_subdirectory(third_party/foo)` calls.
- With `--include-vendored` or `MIKEBOM_INCLUDE_VENDORED=1`: one `pkg:generic/foo@<version-from-version.txt>` component emits with `mikebom:vendored = true`.

**Verification**:
```bash
cargo +stable test --test scan_cmake 2>&1 | grep "test result:"
cargo +stable test --test scan_cmake_vendored 2>&1 | grep "test result:"
# Two separate test files — the latter sets the env var via `Command::env`.
```

## Contract 10 — Cross-ecosystem dedup: vcpkg + Conan declarations of same name emit two components (FR-010 + Q2)

**Invariant**: when `vcpkg.json` declares `"openssl"` AND `conanfile.txt` declares `openssl/3.0.0`, the emitted SBOM contains BOTH `pkg:vcpkg/openssl` AND `pkg:conan/openssl@3.0.0` as separate components. The existing deduplicator's `(ecosystem, name, version, parent_purl)` key keeps them distinct.

**Verification**: A combined fixture with both `vcpkg.json` + `conanfile.txt` in the same dir; assertion on component count = 2 for "openssl" (one per ecosystem).

## Contract 11 — Parse errors surface as scan-summary annotation (FR-015)

**Invariant**: a malformed manifest file (e.g., truncated `vcpkg.json` with unbalanced braces) produces:
- `tracing::warn!` log mentioning the file path and parse error
- Zero components from that file
- A scan-summary-level `mikebom:parse-error` annotation listing the file path

**Verification**: integration test injects a malformed fixture; asserts the SBOM's `metadata.properties[]` contains a `mikebom:parse-error` entry naming the offending file.

## Contract 12 — Diff scope + zero-regression on non-C/C++ projects (SC-006 / SC-008)

**Invariant**:
- `git diff --name-only main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$' | wc -l` → 0 (no new deps).
- `git diff --stat mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}.*` → empty (existing 9 ecosystems' goldens unchanged).
- Existing pre-PR gate passes: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` → `>>> all pre-PR checks passed.`

**Verification**: Final task in the implementation phase runs these checks; PR description includes the output.
