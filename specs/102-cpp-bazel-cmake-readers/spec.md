# Feature Specification: C/C++ source-tree readers (Bazel + CMake)

**Feature Branch**: `102-cpp-bazel-cmake-readers`
**Created**: 2026-05-14
**Status**: Draft
**Input**: User description: "I want to look at including better C/C++ support. In particular i want to better support Bazel and CMake."

## Clarifications

### Session 2026-05-14

- Q: Malformed manifest file (truncated, syntax error, invalid JSON, unbalanced parens) — skip silently / skip+warn / fail-closed / fail-only-on-all-failed? → A: Option B — skip the unparseable file with a `tracing::warn!` log listing the file path + parse error, emit zero components from that file, AND attach a scan-summary-level `mikebom:parse-error` annotation enumerating every file that failed to parse. Aligns with Principle X (Transparency — signal gaps to consumers) and matches the existing maven/golang reader precedent (per-file skip-with-warn).
- Q: Cross-ecosystem dedup — when `vcpkg.json` AND `conanfile.txt` both declare the same logical dep (e.g., `openssl`), merge into one component or emit two? → A: Option B — emit TWO separate components, one per ecosystem PURL (`pkg:vcpkg/openssl@X` AND `pkg:conan/openssl@Y`). The PURL ecosystem distinction is meaningful — different package-manager sources have different versions, content, and patch sets; merging loses provenance fidelity (Principle X). The existing deduplicator's `(ecosystem, name, version, parent_purl)` key naturally separates them; FR-010's "deduplicate" language applies only to the find_package-vs-versioned-source case (where find_package has no version).
- Q: CMake `add_subdirectory(third_party/foo)` vendored-dep emission — default-on / default-on-with-version-file / opt-in flag / don't emit at all? → A: Option A — default-OFF; emit only when operator opts in via `--include-vendored` CLI flag (also accepts `MIKEBOM_INCLUDE_VENDORED=1` env var per the existing milestone-052 + Cross-tier env-var convention). Rationale: vendored deps are hard to identify reliably (no version metadata unless a `version.txt` is co-located; high false-positive risk for `add_subdirectory` calls that aren't actually third-party — e.g., `add_subdirectory(src)`, `add_subdirectory(tests)`). Conservative default protects Principle IX (Accuracy: minimize false positives); opt-in preserves the use case for operators who need vendored coverage. Operator-facing docs MUST explicitly document the flag's behavior, false-positive risks, and how to use a co-located `version.txt` for version backfill.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Bazel project author gets SBOM coverage for declared deps (Priority: P1)

A team building a C++ service with Bazel maintains a `MODULE.bazel` (Bzlmod, the Bazel 6+ dependency model) declaring third-party deps via `bazel_dep(name = "abseil-cpp", version = "20240722.0")` plus a `WORKSPACE.bazel` carrying legacy `http_archive(...)` declarations for libraries that haven't migrated to Bzlmod yet. When they run `mikebom sbom scan --path .` on the source tree, today they get zero C/C++ components — Bazel manifests are invisible to mikebom. They want every declared Bazel dependency to surface in the emitted SBOM with the right ecosystem PURL, the declared version, and (when available) the upstream URL + SHA-256 from the `http_archive` rule.

**Why this priority**: Bazel is the dominant build system for large-scale C++ codebases at Google, Meta, LinkedIn, Tesla, and most of the C++ infrastructure ecosystem — and it has structured, machine-parseable build files (unlike CMake's Turing-complete `.cmake` scripting). MODULE.bazel + http_archive declarations are the lowest-risk, highest-coverage source of C/C++ dependency truth available without runtime tracing. The blocker that prevents mikebom from being useful in the C/C++ enterprise space today.

**Independent Test**: scan a Bazel fixture project containing both `MODULE.bazel` (with ≥2 `bazel_dep` entries) and `WORKSPACE.bazel` (with ≥1 `http_archive` + ≥1 `git_repository`); verify the emitted CycloneDX 1.6 contains the expected components with the right `pkg:bazel/<name>@<version>` PURLs (or canonical fallback PURL for git deps), upstream URLs, and SHA-256 hashes where declared.

**Acceptance Scenarios**:

1. **Given** a directory containing a `MODULE.bazel` with `bazel_dep(name = "abseil-cpp", version = "20240722.0")` and `bazel_dep(name = "googletest", version = "1.14.0")`, **When** the operator runs `mikebom sbom scan --path .`, **Then** the emitted SBOM contains 2 components with `pkg:bazel/abseil-cpp@20240722.0` and `pkg:bazel/googletest@1.14.0` PURLs, both annotated with `mikebom:source-files = ["MODULE.bazel"]`.
2. **Given** the same directory also has a `WORKSPACE.bazel` with `http_archive(name = "rules_python", urls = ["https://github.com/bazelbuild/rules_python/archive/0.30.0.tar.gz"], sha256 = "abc...")`, **When** the same scan runs, **Then** the SBOM additionally contains a `rules_python` component with the upstream URL recorded as `mikebom:download-url` and the declared SHA-256 recorded under the component's `hashes[]`.
3. **Given** a `WORKSPACE.bazel` with `git_repository(name = "foo", remote = "https://github.com/owner/foo.git", commit = "abc1234...")`, **When** the scan runs, **Then** the SBOM contains a `foo` component whose PURL encodes the git ref (e.g. `pkg:bazel/foo@abc1234`) and the upstream remote URL is recorded.
4. **Given** a `MODULE.bazel` with `bazel_dep(name = "rules_cc", version = "0.0.9", dev_dependency = True)`, **When** the scan runs with the default scope filter, **Then** that component is either omitted (default `--exclude-scope=dev`) or included with `scope = "test"` annotation per the standards-native CDX `scope` field.

---

### User Story 2 - CMake project author gets SBOM coverage for FetchContent + ExternalProject (Priority: P1)

A team building C/C++ libraries with CMake declares third-party deps using `FetchContent_Declare(googletest GIT_REPOSITORY ... GIT_TAG release-1.14.0)` and `ExternalProject_Add(zlib URL ... URL_HASH SHA256=...)` directives in `CMakeLists.txt` and sometimes in included `.cmake` modules under `cmake/` or `third_party/`. Today these are invisible to mikebom. The team wants every `FetchContent_Declare` + `ExternalProject_Add` entry to surface in the SBOM, with version (git tag, commit SHA, or URL-derived version), upstream URL, and SHA-256 where declared via `URL_HASH`.

**Why this priority**: CMake is the most widely deployed C/C++ build system across open-source (LLVM, OpenSSL, every Linux distro's `-cmake` package). FetchContent (added in CMake 3.11) is the modern in-build dependency-fetching mechanism; it's structured enough to parse heuristically without executing the CMake script. ExternalProject is the older, more powerful counterpart. Both leave declarative breadcrumbs that mikebom can extract.

**Independent Test**: scan a CMake fixture project containing `CMakeLists.txt` with ≥1 `FetchContent_Declare` and ≥1 `ExternalProject_Add`; verify the SBOM emits the expected components with the right ecosystem-appropriate PURLs (e.g. `pkg:github/google/googletest@release-1.14.0` for GitHub-hosted FetchContent, `pkg:generic/zlib@1.3.1` with a download-URL annotation for the ExternalProject case), declared upstream URLs, and any declared SHA-256 hashes.

**Acceptance Scenarios**:

1. **Given** a `CMakeLists.txt` containing `FetchContent_Declare(googletest GIT_REPOSITORY https://github.com/google/googletest.git GIT_TAG release-1.14.0)`, **When** mikebom scans the project, **Then** the emitted SBOM contains a `googletest` component with PURL `pkg:github/google/googletest@release-1.14.0` (or, if the GIT_TAG is a SHA, `pkg:github/google/googletest@<sha>`) and `mikebom:source-files = ["CMakeLists.txt"]`.
2. **Given** the same project also has `ExternalProject_Add(zlib URL https://zlib.net/zlib-1.3.1.tar.gz URL_HASH SHA256=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23)`, **When** the scan runs, **Then** the SBOM contains a `zlib` component with PURL `pkg:generic/zlib@1.3.1` (version parsed from the URL filename), `mikebom:download-url` set to the declared URL, and the declared SHA-256 in the component's `hashes[]` array.
3. **Given** a CMake project with a `vcpkg.json` manifest file (vcpkg manifest mode is the standard way CMake projects declare dependencies for the vcpkg ecosystem package manager), **When** the scan runs, **Then** the SBOM additionally contains `pkg:vcpkg/<name>@<version>` components for every dep declared in `vcpkg.json`. (See US3 for the full vcpkg/Conan coverage scope.)
4. **Given** a CMake project where the `FetchContent_Declare` is inside an included file like `cmake/third_party.cmake`, **When** the scan runs (and walks `cmake/` + `third_party/` subdirectories per project-roots discovery), **Then** the declared deps surface in the SBOM with `mikebom:source-files = ["cmake/third_party.cmake"]`.

---

### User Story 3 - C/C++ manifest-format coverage extends to vcpkg + Conan (Priority: P2)

Beyond Bazel and CMake's native fetch mechanisms, the C/C++ ecosystem has two dominant "real" package managers: vcpkg (Microsoft) and Conan (JFrog). Both are commonly used alongside CMake — a CMake project's deps are often declared in a `vcpkg.json` manifest or a `conanfile.txt`/`conanfile.py` rather than (or in addition to) inline `FetchContent_Declare`. When the operator scans a CMake project that uses vcpkg or Conan, they want those declared dependencies to surface in the SBOM too.

**Why this priority**: P2 because Stories 1+2 are independently shippable as the headline value (Bazel + CMake-native), and vcpkg/Conan coverage is a natural extension that doesn't change the reader-architecture decisions. P2 also because vcpkg.json manifest mode and Conan's `conanfile.txt` have stable, well-documented declarative shapes (vcpkg.json is JSON; conanfile.txt is INI-shaped), so the implementation risk is bounded.

**Independent Test**: scan a fixture project containing `vcpkg.json` with a `dependencies` array and a `conanfile.txt` with a `[requires]` section; verify the SBOM emits `pkg:vcpkg/<name>@<version>` and `pkg:conan/<name>@<version>` components respectively.

**Acceptance Scenarios**:

1. **Given** a `vcpkg.json` containing `{"dependencies": ["zlib", {"name": "openssl", "version>=": "3.0.0"}]}`, **When** mikebom scans, **Then** the SBOM contains `pkg:vcpkg/zlib` (no version because none declared) and `pkg:vcpkg/openssl@3.0.0` (version-floor as declared version) components.
2. **Given** a `conanfile.txt` with `[requires]\nzlib/1.2.13\nopenssl/3.0.0`, **When** mikebom scans, **Then** the SBOM contains `pkg:conan/zlib@1.2.13` and `pkg:conan/openssl@3.0.0` components.
3. **Given** a project with BOTH a `CMakeLists.txt` (declaring `find_package(zlib REQUIRED)`) AND a `vcpkg.json` (declaring `"zlib"`), **When** mikebom scans, **Then** the SBOM emits exactly ONE zlib component (deduplicated) attributed to vcpkg as the version-bearing source (vcpkg gives a concrete version; `find_package` does not).

---

### Edge Cases

- **MODULE.bazel + WORKSPACE.bazel both present**: most large Bazel projects have both during the Bzlmod migration period. Both are parsed; deps from both are merged. If a dep appears in both with different versions, MODULE.bazel wins (Bzlmod is authoritative for Bazel 7+).
- **WORKSPACE.bazel without sha256**: `http_archive` rules without `sha256` are still parsed; the component is emitted without a `hashes[]` entry but with the declared URL.
- **`bazel_dep` with `version = "0.0.0-...-..."` (development version pin)**: emitted verbatim; consumers handle these as floating refs.
- **CMakeLists.txt with `FetchContent_Declare` inside an `if(BUILD_TESTING)` block**: parsed without evaluating conditions; the dep is emitted with `scope = "test"` when the surrounding block is detected as a test-only conditional. Pragmatic, not semantically perfect.
- **CMakeLists.txt with macro definitions that emit `FetchContent_Declare` indirectly**: not detected (the parser is pattern-based, not a CMake interpreter); operator can supplement via a sidecar manifest if needed.
- **`add_subdirectory(third_party/foo)` vendored dependency**: NOT emitted by default. Operator can opt in via the new `--include-vendored` CLI flag (or `MIKEBOM_INCLUDE_VENDORED=1` env var). When opted-in, emit with `scope = "required"`, `mikebom:vendored = true` property, and the version backfilled from a co-located `version.txt` / `.version` file when present (PURL becomes `pkg:generic/foo@<version-from-file>`; otherwise `pkg:generic/foo` with no version). Default-off rationale: `add_subdirectory` is also used heavily for first-party project sub-modules (`add_subdirectory(src)`, `add_subdirectory(tests)`) — emitting all of them as components would produce noise; gating on the `third_party/`/`vendor/` path heuristic + opt-in flag protects Principle IX (Accuracy).
- **vcpkg.json with `overrides` block**: overrides change the version of an existing dep; emit with the overridden version, not the original declaration's version.
- **Conan `conanfile.py` (Python-script form, vs `.txt`)**: parse `requires = ["zlib/1.2.13", ...]` and `tool_requires = [...]` via regex; ignore arbitrary Python logic. Coverage is heuristic; document that.
- **PURL ecosystem choice for Bazel-declared deps**: `pkg:bazel/...` is the canonical PURL for Bazel Central Registry (BCR) modules. Non-BCR deps (legacy `http_archive` with no BCR entry) fall back to `pkg:generic/...` with `mikebom:download-url` + `mikebom:bazel-archive-name` annotations.
- **CMake `find_package(X CONFIG)` against system-installed packages**: NOT parsed as a dep — the package is provided by the host OS or `vcpkg`/`Conan`, both of which are separately scanned. Listing system `find_package` calls would double-count.
- **Bazel rules_jvm_external Maven deps**: out of scope for milestone 102 (Maven is already covered by the milestone-005+ Maven reader; rules_jvm_external is a forwarding mechanism, not a new ecosystem).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect Bazel projects by the presence of `MODULE.bazel`, `WORKSPACE`, `WORKSPACE.bazel`, or any `.bzl` files in standard locations, and walk into the project root to extract dependency declarations.
- **FR-002**: System MUST parse `MODULE.bazel` to extract every `bazel_dep(name = "...", version = "...", dev_dependency = ...)` declaration and emit each as an SBOM component with PURL `pkg:bazel/<name>@<version>`. `dev_dependency = True` MUST emit with CDX `scope = "test"` per standards-native field precedence.
- **FR-003**: System MUST parse `WORKSPACE` / `WORKSPACE.bazel` (legacy) to extract `http_archive(name, urls, sha256)`, `http_file(name, urls, sha256)`, and `git_repository(name, remote, commit, tag)` rules. Each becomes an SBOM component with a best-effort version (from URL filename for archives, from `commit`/`tag` for git refs).
- **FR-004**: System MUST record the declared upstream URL on every Bazel-derived component (as `mikebom:download-url` per existing milestone-052 convention) AND record the declared `sha256` value in the component's `hashes[]` array.
- **FR-005**: System MUST detect CMake projects by the presence of `CMakeLists.txt` in the scan root or its immediate subdirectories, and walk both the root and any standard CMake-module directories (`cmake/`, `Modules/`, `third_party/`).
- **FR-006**: System MUST parse `CMakeLists.txt` and any included `.cmake` file under standard CMake module paths to extract:
  - `FetchContent_Declare(<name> GIT_REPOSITORY ... GIT_TAG ...)` → `pkg:github/<owner>/<repo>@<tag>` if the GIT_REPOSITORY URL matches a github.com pattern; otherwise `pkg:generic/<name>@<tag>` with `mikebom:download-url`.
  - `FetchContent_Declare(<name> URL ... URL_HASH SHA256=...)` → `pkg:generic/<name>@<version>` with version parsed from URL filename if possible; the URL recorded as `mikebom:download-url`; the SHA-256 in `hashes[]`.
  - `ExternalProject_Add(<name> URL ... URL_HASH SHA256=...)` → same shape as `FetchContent_Declare` URL form.
  - `ExternalProject_Add(<name> GIT_REPOSITORY ... GIT_TAG ...)` → same shape as `FetchContent_Declare` GIT form.
- **FR-007**: System MUST detect and parse `vcpkg.json` manifest files in the scan root, extracting every `dependencies` array entry. Both the string form (`"zlib"`) and the object form (`{"name": "openssl", "version>=": "3.0.0"}`) MUST be supported. Each becomes a `pkg:vcpkg/<name>@<version>` component (version omitted if not declared).
- **FR-008**: System MUST detect and parse `conanfile.txt` (Conan recipe file in INI format) in the scan root, extracting the `[requires]` and `[tool_requires]` sections. Each `<name>/<version>` line becomes a `pkg:conan/<name>@<version>` component. `[tool_requires]` entries get `scope = "build"` per CDX standards-native field.
- **FR-009**: System MUST detect and best-effort-parse `conanfile.py` (Conan recipe file in Python form) via regex-extraction of `requires = [...]` and `tool_requires = [...]` literal lists. Documented as heuristic coverage; non-literal cases (deps assembled in Python control flow) are out of scope.
- **FR-010**: System MUST deduplicate components emitted by multiple readers ONLY when they share the same PURL ecosystem AND name (e.g., a zlib appearing in both `CMakeLists.txt`'s `find_package` AND `vcpkg.json`'s `dependencies` both resolve to `pkg:vcpkg/zlib` — collapse into one). Deduplication MUST prefer the version-bearing source as the canonical entry; the other entry's source-files MUST merge into the canonical component's `mikebom:source-files` array. **Cross-ecosystem same-name deps MUST emit as separate components** (e.g., `pkg:vcpkg/openssl@X` AND `pkg:conan/openssl@Y` — both surface, since each represents a distinct package-manager-declared source with potentially different versions, content, and patch sets). The existing deduplicator's `(ecosystem, name, version, parent_purl)` key naturally separates them.
- **FR-011**: System MUST NOT emit components for `find_package(X)` declarations alone — these refer to system-installed packages already covered by the OS-package readers (dpkg/rpm/apk) or by vcpkg/Conan if used. Double-counting would inflate the SBOM with phantom entries.
- **FR-012**: System MUST emit `mikebom:source-files = [...]` on every Bazel/CMake/vcpkg/Conan-derived component pointing back to the specific manifest file(s) it was declared in, per Constitution Principle X (Transparency).
- **FR-013**: System MUST treat all Bazel/CMake/vcpkg/Conan readers as cross-platform (no `#[cfg(unix)]` gates) — they read text/JSON manifest files only, no OS-specific filesystem APIs.
- **FR-014**: System MUST conform to CycloneDX 1.6, SPDX 2.3, and SPDX 3 emission for every component (Principle V: Specification Compliance). Standards-native `scope` field MUST be used in preference to any `mikebom:dev-dependency`-style annotation (per Principle V audit clause + the milestone-052 lifecycle-scope precedent).
- **FR-015**: When a manifest file (`MODULE.bazel`, `WORKSPACE.bazel`, `CMakeLists.txt`, `vcpkg.json`, `conanfile.txt`, `conanfile.py`, or any included `.cmake` module) fails to parse — truncated content, encoding issues, syntax errors, unbalanced parens/braces, invalid JSON — the reader MUST (a) emit a `tracing::warn!` log naming the file path + the parse error, (b) emit zero components from that file (skip, do not silently treat as empty), AND (c) attach a scan-summary-level `mikebom:parse-error` annotation listing every file that failed to parse during this scan. The scan as a whole MUST NOT fail; other readers + other files MUST continue normally (skip-with-warn precedent matches the existing maven/golang readers). Per Constitution Principle X (Transparency).
- **FR-016**: System MUST gate vendored-dep emission (CMake `add_subdirectory(third_party/...)` / `add_subdirectory(vendor/...)`) behind an opt-in `--include-vendored` CLI flag (also accepts `MIKEBOM_INCLUDE_VENDORED=1` env var per the existing milestone-052 + Cross-tier env-var convention). Default is OFF. When opted-in, emit `pkg:generic/<name>@<version-from-version.txt-if-present>` components with `mikebom:vendored = true` and `mikebom:source-files = [<the CMakeLists.txt that contained the add_subdirectory call>]`. Only `add_subdirectory(<path>)` calls where `<path>` starts with `third_party/` or `vendor/` are considered; first-party project-internal `add_subdirectory(src)`-style calls remain unaffected.
- **FR-017**: System MUST document the `--include-vendored` flag in both `mikebom --help` output AND in `docs/user-guide/cli-reference.md`. Documentation MUST explicitly state: (a) the default is OFF, (b) what counts as a "vendored" dep (`third_party/` or `vendor/` path prefix), (c) the false-positive risks (e.g., `add_subdirectory` calls that aren't actually third-party), and (d) how a co-located `version.txt` file is used for version backfill. This serves Principle X (Transparency) for the operator-facing surface.

### Key Entities *(include if feature involves data)*

- **Bazel dependency**: a triple of (name, version, source) where source ∈ {bzlmod, http_archive, http_file, git_repository}. Optional fields: declared upstream URL, declared SHA-256, dev_dependency flag.
- **CMake fetched dependency**: a triple of (name, version, source) where source ∈ {fetchcontent_git, fetchcontent_url, externalproject_git, externalproject_url}. Optional fields: declared GIT_REPOSITORY URL, declared GIT_TAG, declared URL, declared URL_HASH (sha256), test-scope flag.
- **vcpkg dependency**: a pair of (name, optional version) parsed from `vcpkg.json::dependencies[]`. Optional fields: version constraint (e.g., `version>=`), feature flags.
- **Conan dependency**: a pair of (name, version) parsed from `conanfile.txt::[requires]` or `conanfile.py::requires=[...]`. Optional fields: scope (tool vs runtime).
- **Manifest source-file path**: the absolute path of the declaring file (`MODULE.bazel`, `WORKSPACE.bazel`, `CMakeLists.txt`, `cmake/third_party.cmake`, `vcpkg.json`, `conanfile.txt`, `conanfile.py`), emitted as `mikebom:source-files = [...]` per FR-012.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator running `mikebom sbom scan --path .` against a Bazel C++ project (MODULE.bazel + WORKSPACE.bazel) sees ≥95% of declared dependencies surface in the emitted SBOM as components with correct ecosystem PURLs and declared versions, matching what `bazel mod graph` (or equivalent introspection) reports for direct deps.
- **SC-002**: An operator running the same command against a CMake project using FetchContent + ExternalProject sees ≥90% of declared deps surface in the SBOM with correct PURLs (the 10% headroom accounts for `FetchContent_Declare` calls inside macros or non-literal arguments — these are documented as heuristic-coverage gaps).
- **SC-003**: An operator running the same command against a CMake project using `vcpkg.json` manifest mode sees 100% of declared deps surface (vcpkg.json is well-structured JSON with no heuristic ambiguity).
- **SC-004**: An operator running the same command against a CMake project using `conanfile.txt` sees ≥98% of declared deps surface (conanfile.txt is INI-format with stable structure; the 2% headroom is for line-continuation edge cases).
- **SC-005**: An operator running the same command against a CMake project using `conanfile.py` sees ≥80% of declared deps surface where deps are declared as literal lists (the heuristic ceiling; non-literal cases are documented as out-of-scope).
- **SC-006**: An operator's existing SBOM workflows that don't involve C/C++ continue unchanged: scanning a Rust/Go/Python/JS/Ruby/Java project emits the same byte-identical SBOM pre/post-milestone-102 (verifiable via the existing goldens regression suite).
- **SC-007**: No `mikebom:*` property is introduced where an existing CDX/SPDX 2.3/SPDX 3 native field already carries the same semantic, per Constitution Principle V. The expected new property surfaces: `mikebom:download-url` reuses the existing milestone-052+ convention; `mikebom:bazel-archive-name` documents a Bazel-specific archive identifier with no native equivalent. Both pass Principle V's audit clause; the spec author records the audit in the spec's Functional Requirements.
- **SC-008**: Total diff scope ≤8 NEW source files (4 readers — bazel, cmake, vcpkg, conan — plus 4 ecosystem-test fixtures or tests) + path-resolver/dispatcher changes ≤2 modified files + CLI changes (1 new `--include-vendored` flag per FR-016) + docs (`docs/user-guide/cli-reference.md` per FR-017 + the README ecosystems table). Zero new Cargo runtime dependencies (regex parsing uses the already-present `regex` crate; JSON via `serde_json`; INI via `toml` or pure-Rust line splitting).
- **SC-009**: The `--include-vendored` flag is discoverable: `mikebom sbom scan --help` shows it with a one-line description; `docs/user-guide/cli-reference.md` carries the full operator-facing description per FR-017. An operator running `mikebom sbom scan --help | grep vendored` sees the flag without prior knowledge it exists.

## Assumptions

- **Bazel scope assumption**: MODULE.bazel parsing covers Bazel 6+ Bzlmod-style projects. WORKSPACE.bazel parsing covers the long tail of Bazel 5 / legacy projects + Bazel 7+ projects still mid-migration. Both are necessary; both are scoped here.
- **CMake script-parsing assumption**: CMake's scripting language is Turing-complete, but `FetchContent_Declare` and `ExternalProject_Add` calls in practice use literal arguments ≥90% of the time across the open-source corpus (verified via spot-check of LLVM, gRPC, Envoy, Bazel, RocksDB CMake trees). The parser is pattern-based — sufficient for the literal-argument majority; documented gaps for the heuristic-only minority.
- **PURL ecosystem assumption**: `pkg:bazel/...`, `pkg:vcpkg/...`, and `pkg:conan/...` are the canonical PURL types for these ecosystems (vcpkg + conan are in the PURL spec; bazel is widely used in practice for Bazel Central Registry modules even though the formal PURL spec entry is in progress). Where a Bazel `http_archive` has no clear BCR entry, fall back to `pkg:generic/...`.
- **Out of scope**: actual *build-time* tracing of C/C++ compiler invocations (eBPF / fanotify / preload-shim style). That's a milestone-of-its-own (would integrate with the existing eBPF tracing on Linux). This milestone is source-tree manifest analysis only.
- **Out of scope**: Meson `meson.build` + wrap deps. Meson is the third-most-common C/C++ build system but its declarative-dep surface is smaller; defer to a future milestone unless this milestone surfaces unexpectedly low coverage on the Meson corpus.
- **Out of scope**: xmake, build2, premake. Niche relative to Bazel/CMake/vcpkg/Conan.
- **Out of scope**: pkg-config `.pc` files. They describe linkage, not declared dependencies; the binary scanner + OS-package readers already cover linkage-time C/C++ dep visibility.
- **Out of scope**: parsing CMake's `find_package(X REQUIRED)` for component emission (per FR-011 rationale). Out-of-scope to avoid double-counting against vcpkg/Conan/OS-package coverage.
- **Out of scope**: Bazel `rules_jvm_external` Maven deps (already covered by milestone-005+ Maven reader; `rules_jvm_external` is a forwarding rule, not a new ecosystem).
- **Constitution alignment**: this milestone is user-space-only source-tree manifest parsing, fully aligned with Principle II (eBPF-only observation applies to *runtime* dependency discovery; source-tree readers like cargo/npm/pip/gem/maven/golang are already established as enrichment/manifest-analysis per the existing reader architecture). New readers are purely additive; no constitutional amendment needed.
- **Existing reader architecture extension**: the 4 new readers (bazel, cmake, vcpkg, conan) live under `mikebom-cli/src/scan_fs/package_db/` alongside the existing 11 readers (cargo, npm, pip, gem, maven, golang, dpkg, apk, rpm, rpm_file, file_hashes). The path-resolver dispatcher in `mikebom-cli/src/resolve/path_resolver.rs` gets 4 new arms.
