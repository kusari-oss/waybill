# Feature Specification: C/C++ Ecosystem Expansion (Phase 2)

**Feature Branch**: `105-cpp-ecosystem-expansion`
**Created**: 2026-05-28
**Status**: Draft
**Input**: User description: "let's build or update C/C++ specification to hit these things."

## Context

mikebom alpha.41 ships four C/C++ readers — bare CMake (`FetchContent_Declare` /
`ExternalProject_Add` / vendored `third_party/` detection), `vcpkg.json` manifest
mode, `conanfile.txt`, and Bazel `WORKSPACE`/`MODULE.bazel`. When the canonical
manifest pattern appears, these readers produce real PURLs with real versions
(e.g. `pkg:github/google/googletest@release-1.14.0`,
`pkg:bazel/build_bazel_rules_swift@1.7.1`).

End-to-end testing on three real-world corpora exposed coverage gaps that block
auditing the majority of modern C/C++ codebases:

| Corpus | C/C++ components found today | Real-world deps actually present | Gap class |
|---|---|---|---|
| gRPC v1.69.0 (with submodules populated) | 3 (from `WORKSPACE`) | ~16 git submodules + `find_package`-driven libs | submodule correlation; `conanfile.py` |
| Zephyr v4.4.0 (main repo) | 0 | 79 `west.yml`-managed modules (HALs, mbedTLS, picolibc, …) | `west.yml` meta-tool reader |
| esp-idf | 0 | ~50–200 `idf_component.yml`-managed components | `idf_component.yml` reader |
| OpenSTLinux app (typical) | App-level only | SDK sysroot (libc, libstdc++, openssl, gstreamer, ST HAL, …) | Yocto/OE recipe reader (**deferred to follow-on milestone — see Clarifications**) |
| CPM.cmake-using projects (modern mainstream) | 0 from CPM | All deps via `cpmaddpackage(...)` | CPM.cmake support |

This milestone closes the gaps in priority order so that a developer scanning
any of these project shapes sees their dependencies with real PURLs, real
versions, and the existing `mikebom:source-mechanism` annotation indicating
how each component entered the build.

## Clarifications

### Session 2026-05-28

- Q: Should Yocto / OpenSTLinux (originally US7) be delivered in this milestone or split? → A: **Split into its own follow-on milestone.** Milestone 105 delivers US1–US6 only (CPM.cmake, conanfile.py, west.yml, idf_component.yml, vcpkg classic mode, git-submodule correlation). Yocto recipe parsing becomes its own milestone after 105 ships, where the `pkg:bitbake/...` PURL question and OpenSTLinux-specific layer conventions can be researched properly.
- Q: What PURL form should the new `idf-component` reader emit for registry-resolved components? → A: **`pkg:idf/<namespace>/<name>@<version>` (speculative ecosystem name).** Downstream consumers that don't yet recognize `pkg:idf/` fall back to the registry source URL recorded in the existing `mikebom:source-files` / `mikebom:download-url` annotations. A package-url spec registration request for the `idf` ecosystem is filed as part of this milestone's documentation deliverables. Other new readers use already-established PURL types: `pkg:github/` or `pkg:git+https://` for `zephyr-west` and `git-submodule`; `pkg:conan/` for `conanfile.py`; `pkg:vcpkg/` for `vcpkg-classic` (matches existing `vcpkg-manifest`); `pkg:github/` or `pkg:generic/` for `cpm-cmake`.
- Q: How should mikebom dedup the same library when multiple readers identify it (e.g., `abseil-cpp` as both a `git-submodule` and a `conan-recipe`)? → A: **Deterministic manifest-mode-over-filesystem precedence with an "also-detected-via" trail.** Manifest-mode declarations (vcpkg.json, vcpkg classic install records, west.yml, conanfile.py/.txt, idf_component.yml) outrank filesystem-derived signals (git-submodule, cmake-vendored). Within each tier, prefer the reader whose source-mechanism produces the most specific PURL (Conan > GitHub > git+https > generic). Components are deduplicated by **canonical PURL string after normalization**. When the same canonical PURL is produced by multiple readers, the higher-precedence reader's source-mechanism wins, and the lower-precedence reader's signal is recorded in a new `mikebom:also-detected-via` annotation (array of strings, one per losing reader's source-mechanism value). Filesystem-walk order MUST NOT influence the chosen winner — the precedence table is the sole arbiter.
- Q: Should the new `git-submodule` and `zephyr-west` readers strip credentials from URLs (e.g., `https://user:token@github.com/...`) before emitting them in PURLs / annotations? → A: **Reuse milestone-075's URL-sanitization helper unconditionally** on every URL-derived emission from the new readers — PURL host portion, `mikebom:download-url`, and any annotation that contains a URL string. Sanitization strips `user:password` from `https://` URLs, normalizes `ssh://`-with-credential URLs, and emits a `tracing::warn!` event when redaction occurred. Behavior is unconditional (no opt-out flag) and unchanged for credential-free manifests, which are the overwhelming majority.
- Q: For the `git-submodule` reader, should every entry in `.gitmodules` be emitted (even submodules the current build doesn't link), or only those correlated with a `find_package(...)` call? → A: **Emit every submodule unconditionally, with a build-reference annotation reflecting build-time correlation.** Each submodule component carries `mikebom:build-reference: "declared-and-used"` when a matching `find_package(<name> ...)` call (case-insensitive name match against the submodule's last path segment, with target-alias resolution where available) appears in any scanned `CMakeLists.txt`; otherwise `mikebom:build-reference: "declared-only"`. Rationale: the source-tier SBOM should accurately reflect what's in the source tree (Constitution tier-honesty); the build-reference annotation lets downstream consumers filter out un-used deps with a single query.  *Correction recorded in research.md R3 (2026-05-28): the original answer named `mikebom:linkage-kind`, but that annotation is already used by the binary readers with a closed enum of `dynamic`/`static`/`mixed` and a CDX builder debug-assert that would have been violated. A new annotation `mikebom:build-reference` is introduced instead, with parity catalog row C57.*

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Modern CMake project using CPM.cmake (Priority: P1)

A developer maintains a modern C++ application whose `CMakeLists.txt` (or
`Dependencies.cmake`) uses [CPM.cmake](https://github.com/cpm-cmake/CPM.cmake)
via `cpmaddpackage(...)` to declare every dependency: fmt, spdlog, Catch2,
CLI11, FTXUI, etc. The developer runs `mikebom sbom scan --path .` expecting
to see every CPM-declared dependency as an SBOM component with the version
that CPM was configured to pin.

**Why this priority**: CPM.cmake is the de-facto modern alternative to bare
`FetchContent_Declare` in the C++ community and is referenced by the
`cpp-best-practices` starter templates that drive new C++ project bootstrapping.
Closing this gap takes the cmake reader from "covers a small minority of
modern cmake projects" to "covers the modern mainstream", and the parsing
shape (key/value arg list inside a CMake function call) is similar enough to
the existing `FetchContent_Declare` path that incremental cost is low.

**Independent Test**: Add a fixture project containing a `Dependencies.cmake`
with multiple `cpmaddpackage(NAME ... GITHUB_REPOSITORY ... GIT_TAG ...)` /
`cpmaddpackage(NAME ... VERSION ... GITHUB_REPOSITORY ...)` calls. Scan it
and assert: every declared dep emerges as an SBOM component with a real
PURL (`pkg:github/<org>/<repo>@<tag>` or `pkg:generic/<name>@<version>`),
matching the GIT_TAG or VERSION declared by the project. Annotated with
`mikebom:source-mechanism: "cpm-cmake"`.

**Acceptance Scenarios**:

1. **Given** a project with `cpmaddpackage(NAME fmt GITHUB_REPOSITORY fmtlib/fmt GIT_TAG 12.1.0 ...)`, **When** scanned with mikebom, **Then** an SBOM component `pkg:github/fmtlib/fmt@12.1.0` exists with `mikebom:source-mechanism: "cpm-cmake"`.
2. **Given** a project with `cpmaddpackage(NAME spdlog VERSION 1.17.0 GITHUB_REPOSITORY gabime/spdlog ...)` (no explicit GIT_TAG), **When** scanned, **Then** the component emerges as `pkg:github/gabime/spdlog@1.17.0`.
3. **Given** a project with `cpmaddpackage(NAME tools GITHUB_REPOSITORY lefticus/tools GIT_TAG main)` (rolling tag), **When** scanned, **Then** the component emerges as `pkg:github/lefticus/tools@main` and the `mikebom:resolver-step` annotation records that the version is unstable/rolling.
4. **Given** a project mixing `cpmaddpackage` and bare `FetchContent_Declare`, **When** scanned, **Then** both readers fire and components carry the appropriate `cpm-cmake` or `cmake-fetchcontent-*` source-mechanism value.

---

### User Story 2 — Modern Conan project using `conanfile.py` (Priority: P1)

A developer's project declares its C/C++ dependencies in a Python-style
`conanfile.py` (the modern Conan 2.x norm), not the older INI-style
`conanfile.txt`. The developer runs `mikebom sbom scan` and expects every
Conan recipe declared in the `requires`/`build_requires`/`tool_requires`
attributes to appear as a component, the same way `conanfile.txt`
dependencies already do today.

**Why this priority**: `conanfile.py` is the canonical Conan 2.x recipe
format; `conanfile.txt` is legacy. Conan 2.0 went GA in 2023, and new Conan
projects default to `.py`. The current `conan-recipe` reader only handles
`.txt`, meaning the majority of modern Conan-using C++ projects are
invisible to mikebom. This gap was directly exposed when scanning gRPC,
which has `conanfile.py` files in three third-party subdirectories
(abseil-cpp, protobuf, bloaty) all of which were missed.

**Independent Test**: Add a fixture project containing a `conanfile.py`
that declares `requires = ("zlib/1.3.1", "openssl/3.0.0")` and
`tool_requires = ("cmake/3.27.7",)`. Scan it; assert all three components
appear with `pkg:conan/<name>@<version>` PURLs and
`mikebom:source-mechanism: "conan-recipe"`. Differentiate runtime requires
vs build/tool requires via the existing `mikebom:lifecycle-scope` annotation.

**Acceptance Scenarios**:

1. **Given** a `conanfile.py` with `requires = ("zlib/1.3.1", "openssl/3.0.0")`, **When** scanned, **Then** components `pkg:conan/zlib@1.3.1` and `pkg:conan/openssl@3.0.0` exist with `mikebom:source-mechanism: "conan-recipe"`.
2. **Given** a `conanfile.py` declaring requirements inside the `requirements(self)` method via `self.requires("foo/1.2.3")`, **When** scanned, **Then** `pkg:conan/foo@1.2.3` is emitted (matches the method-based declaration style).
3. **Given** a `conanfile.py` with both `requires` and `tool_requires`, **When** scanned, **Then** tool requirements are tagged `mikebom:lifecycle-scope: "build"` and runtime requirements are tagged `mikebom:lifecycle-scope: "runtime"`.
4. **Given** a directory containing both `conanfile.txt` (legacy) and `conanfile.py` (modern), **When** scanned, **Then** mikebom emits each dependency exactly once (deduplicates by PURL) and does not double-count.

---

### User Story 3 — Zephyr RTOS application with `west.yml` modules (Priority: P2)

A developer working on a Zephyr-based embedded firmware project runs
`mikebom sbom scan` against their Zephyr workspace. The workspace contains
the main Zephyr tree plus a `west.yml` manifest listing 79 modules
(hardware abstraction layers for Nordic / NXP / ST / Espressif chips,
mbedTLS, picolibc, OpenThread, Trusted Firmware-M, …). The developer expects
each module declared in `west.yml` to surface as an SBOM component with the
exact Git revision the manifest pins.

**Why this priority**: Zephyr is the largest open-source RTOS project by
contributor count and the dominant build target for ST, Nordic, NXP, and
Espressif embedded developers. The published Zephyr SBOM (file-level
license-only via `reuse-6.2.0`) does not answer "what components is this
firmware made of"; mikebom should be the authoritative answer. Adding a
`west.yml` reader (well-defined YAML schema, no subprocess, no network)
unlocks the entire Zephyr ecosystem for SCA / vuln scanning.

**Independent Test**: Add a fixture containing a Zephyr-style `west.yml`
manifest with `manifest.projects:` entries (mix of `revision: <sha>`,
`revision: <tag>`, and entries with explicit `remote`). Scan; assert every
project appears as a component with `pkg:git+https://<remote>/<repo>@<rev>`
or equivalent PURL and `mikebom:source-mechanism: "zephyr-west"`. Component
count must equal manifest `projects[]` length (modulo `groups` filtering, see
edge cases).

**Acceptance Scenarios**:

1. **Given** a `west.yml` with `- name: hal_stm32, revision: a1b2c3d4..., remote: upstream`, **When** scanned, **Then** a component `pkg:github/zephyrproject-rtos/hal_stm32@a1b2c3d4...` (or equivalent VCS-locator PURL) emerges with `mikebom:source-mechanism: "zephyr-west"`.
2. **Given** a `west.yml` with multiple `remotes:` and `defaults: remote: upstream`, **When** scanned, **Then** each project's PURL correctly resolves to the right remote based on `remote:` or `defaults.remote:`.
3. **Given** a `west.yml` with `groups:` (e.g., `babblesim`, `optional`), **When** scanned, **Then** by default mikebom emits all groups; a `--exclude-group <name>` flag (or equivalent) lets the operator filter.
4. **Given** a scan of the Zephyr v4.4.0 main repository (real-world corpus), **When** scanned with mikebom, **Then** at minimum 79 C/C++ components from the west manifest appear in the output (up from 0 today).

---

### User Story 4 — Espressif esp-idf project using `idf_component.yml` (Priority: P2)

A developer building firmware for an ESP32 chip using esp-idf has multiple
`idf_component.yml` files (one per Espressif Component Manager subsystem)
declaring dependencies on registry-published components (`espressif/mdns`,
`espressif/esp_websocket_client`, etc.) with pinned versions. mikebom must
discover and parse all `idf_component.yml` files in the scan tree and emit
each declared dependency as an SBOM component.

**Why this priority**: esp-idf is the official Espressif SDK and the
dominant choice for ESP32/ESP32-S3/ESP32-C6 firmware development. The
Espressif Component Manager registry hosts thousands of components.
Without this reader, mikebom finds zero C/C++ components in any non-trivial
esp-idf project (confirmed during testing). Same gap-shape as Zephyr's
`west.yml`: a project-specific YAML meta-tool manifest.

**Independent Test**: Add a fixture with multiple `idf_component.yml` files,
each declaring `dependencies:` with version specifiers (e.g.,
`espressif/mdns: "^1.2.0"`, `local-comp: { path: ../local }`). Scan; assert
every registry-named dependency appears as `pkg:idf/<namespace>/<name>@<version>`
(or equivalent IDF-component PURL) annotated with
`mikebom:source-mechanism: "idf-component"`. Path-local components emit a
separate annotation distinguishing them from registry-resolved ones.

**Acceptance Scenarios**:

1. **Given** an `idf_component.yml` with `dependencies: espressif/mdns: "1.4.2"`, **When** scanned, **Then** a component `pkg:idf/espressif/mdns@1.4.2` emerges with `mikebom:source-mechanism: "idf-component"`.
2. **Given** an `idf_component.yml` with version range `"^1.2.0"` and a present `dependencies.lock` (the IDF lockfile), **When** scanned, **Then** the exact pinned version from the lockfile is used; absent a lockfile, the version range string is preserved with a `mikebom:requirement-range` annotation.
3. **Given** a project with multiple component-level `idf_component.yml` files (typical esp-idf layout: `main/idf_component.yml`, `components/<name>/idf_component.yml`), **When** scanned, **Then** mikebom unions all dependencies and emits each unique PURL once.
4. **Given** a path-based component (`my_lib: { path: ../my_lib }`), **When** scanned, **Then** mikebom emits a component with `pkg:generic/my_lib` and `mikebom:source-mechanism: "idf-component-local"`.

---

### User Story 5 — Microsoft-style C++ project using vcpkg classic mode (Priority: P3)

A developer's project uses vcpkg in classic (non-manifest) mode — there is
no `vcpkg.json` at the project root, only port definitions installed via
`vcpkg install <portname>`. The dependency declaration lives in build
metadata (`CMakeLists.txt` `find_package(...)` calls combined with a
vcpkg toolchain file). For projects that vendor a `vcpkg/ports/<name>/`
directory or include a `vcpkg/installed/<triplet>/vcpkg/info/<name>_<ver>_<triplet>.list`
manifest, mikebom should extract per-port name+version.

**Why this priority**: vcpkg classic mode predates manifest mode and is
still widely used in Microsoft-aligned C++ projects. Less critical than
CPM.cmake or `conanfile.py` because newer projects increasingly migrate to
manifest mode, but still a meaningful coverage hole. Effort is medium
because the data shape is heterogeneous (CONTROL files, `.list` files, or
inferred from `find_package` paths).

**Independent Test**: Add a fixture containing a vcpkg classic-mode tree
(`vcpkg/installed/x64-linux/vcpkg/info/<name>_<ver>_<triplet>.list` files).
Scan; assert each installed port appears as `pkg:vcpkg/<name>@<version>`
with `mikebom:source-mechanism: "vcpkg-classic"`.

**Acceptance Scenarios**:

1. **Given** a `vcpkg/installed/x64-linux/vcpkg/info/zlib_1.3.1_x64-linux.list`, **When** scanned, **Then** a component `pkg:vcpkg/zlib@1.3.1` emerges with `mikebom:source-mechanism: "vcpkg-classic"`.
2. **Given** a project mixing classic and manifest mode (vcpkg.json AND installed/ tree), **When** scanned, **Then** the manifest-mode declaration wins (deduplication by PURL) and the source-mechanism annotation reflects `vcpkg-manifest` (richer signal).

---

### User Story 6 — Large C++ project using git submodules + `find_package` (Priority: P3)

A developer's `CMakeLists.txt` does not use FetchContent or ExternalProject;
instead, all third-party libraries are pulled in as git submodules under
`third_party/` and consumed via `find_package(<name> REQUIRED CONFIG)`
calls. This is the pattern used by gRPC, LLVM, and many large C++ projects.
The developer expects mikebom to correlate `.gitmodules` entries with
`find_package` calls and surface each submodule as a component pinned to
its checked-out revision.

**Why this priority**: Closes the gap that surfaced during gRPC testing
(16 declared submodules → 0 components found). The implementation is
medium-effort: parse `.gitmodules` for `path` + `url`, walk the submodule
directory to read its checked-out HEAD revision, and emit a component per
submodule. Correlation with `find_package` calls is a nice-to-have for
distinguishing actual build-time deps from un-used submodules but is not
required for the MVP.

**Independent Test**: Add a fixture with a `.gitmodules` declaring 3
submodules (e.g., `third_party/abseil-cpp`, `third_party/protobuf`,
`third_party/googletest`), each with a checked-out HEAD revision. Scan;
assert 3 components emerge as
`pkg:git+https://<url>@<sha>` (or equivalent VCS-locator PURL) with
`mikebom:source-mechanism: "git-submodule"`.

**Acceptance Scenarios**:

1. **Given** a `.gitmodules` with 3 entries and populated submodule directories, **When** scanned, **Then** 3 components emerge each pinned to the submodule's checked-out commit SHA.
2. **Given** a `.gitmodules` entry whose submodule directory is empty (not initialized), **When** scanned, **Then** mikebom emits the component using the URL but flags it with `mikebom:resolver-step: "uninitialized-submodule"` and uses `version: "unknown"`.
3. **Given** a scan of the gRPC v1.69.0 corpus with submodules populated (real-world test), **When** scanned, **Then** at minimum 16 submodule components appear in the output (up from 3 today, accounting for the 3 already discovered via the bazel reader).

---

### Edge Cases

- **CPM.cmake**: `cpmaddpackage(...)` called with no `VERSION` and no
  `GIT_TAG` (CPM defaults the tag based on package name lookup) — mikebom
  emits `version: "unknown"` with a `mikebom:resolver-step` annotation
  explaining the absence.
- **CPM.cmake**: `cpmfindpackage(...)` and `cpmdeclarepackage(...)` variants
  in addition to `cpmaddpackage(...)` — mikebom recognizes all three.
- **CPM.cmake**: One-line and multi-line forms (whitespace and comments
  between key/value pairs) parse identically.
- **conanfile.py**: requirements declared inside conditional blocks
  (`if self.settings.os == "Linux": self.requires(...)`) — mikebom emits
  the component with a `mikebom:lifecycle-scope` annotation reflecting the
  guard condition as best-effort metadata, but does not attempt to evaluate
  the condition.
- **conanfile.py**: Project-specific Python helpers (`from conan import
  ConanFile` subclasses with non-trivial methods) — mikebom parses by
  regex / AST-light heuristics, gracefully skipping recipes that don't
  match recognized patterns and emitting a structured-warning log.
- **west.yml**: `import:` directives that pull additional manifest files
  from another west project — mikebom does not transitively chase imports
  in this milestone (deferred).
- **west.yml**: `path:` overrides that relocate a module elsewhere in the
  filesystem — mikebom uses `path:` to find the module's local checkout if
  present, but emits the component regardless of presence (using
  `revision:` as the version source).
- **idf_component.yml**: Local path components without a registry namespace
  (e.g. `my_lib: { path: ../my_lib }`) emit as `pkg:generic/my_lib` with a
  distinct source-mechanism value (`idf-component-local`).
- **idf_component.yml**: Override URLs (e.g. `service_url:
  https://my-private-registry/...`) are reflected in the PURL host
  qualifier or annotation.
- **vcpkg classic**: triplet variants (`x64-linux`, `x64-osx`, `arm64-linux`)
  — mikebom emits one component per unique `<name>@<version>` regardless
  of triplet, with the triplet captured in `mikebom:target-arch`.
- **Submodules**: nested submodules (submodule of a submodule) — mikebom
  walks recursively up to the project root only; does not enumerate
  transitively.
- **Submodules**: detached HEAD with no tag — version is the short commit
  SHA.
- **Submodules — `find_package` name mismatch**: when the submodule's
  last path segment doesn't case-insensitively match any `find_package`
  target name (e.g., `third_party/boringssl-with-bazel` for the
  `OpenSSL::SSL` target), the `mikebom:build-reference` annotation reads
  `"declared-only"`. This is acceptable — operators can override by
  adding a target-alias hint in a future milestone if false-negatives
  become common.
- **Submodules — target aliases**: when a `CMakeLists.txt` uses
  `add_library(SomeLib::SomeLib ALIAS foo)` and downstream code calls
  `find_package(SomeLib)`, mikebom recognizes the alias only when both
  sides are statically visible in scanned files. Dynamic aliases set
  inside macros / functions are not chased; the result is a conservative
  `"declared-only"` classification.
- **Credentials in URLs** (cross-cutting `git-submodule` / `zephyr-west`):
  a manifest containing `https://user:token@github.com/...` or
  `ssh://deploy-key@host:...` URLs MUST have credentials stripped before
  the URL appears anywhere in the SBOM. The redaction is logged via
  `tracing::warn!` so operators can fix the offending manifest. Behavior
  reuses milestone-075's `strip-id-credentials` helper unconditionally.
- **Polyglot regression** (cross-cutting): mikebom should never abort the
  scan because a non-C/C++ reader hit a parse error on an unsupported
  legacy lockfile (e.g., npm package-lock v1). The C/C++ readers in this
  milestone must be robust to this even when other readers fail.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When mikebom encounters a `cpmaddpackage(...)`, `cpmfindpackage(...)`, or `cpmdeclarepackage(...)` call in a CMake file, it MUST extract the package `NAME`, `VERSION` (if present), `GIT_TAG` (if present), and `GITHUB_REPOSITORY` (or `GIT_REPOSITORY`) arguments and emit an SBOM component carrying the appropriate PURL (`pkg:github/<org>/<repo>@<tag>` or `pkg:generic/<name>@<version>`).
- **FR-002**: Every component emitted via a CPM.cmake declaration MUST carry the annotation `mikebom:source-mechanism: "cpm-cmake"`.
- **FR-003**: When mikebom encounters a `conanfile.py`, it MUST extract `requires`, `build_requires`, and `tool_requires` declarations (whether declared as class attributes or inside the `requirements(self)` method) and emit one SBOM component per dependency with PURL `pkg:conan/<name>@<version>` and annotation `mikebom:source-mechanism: "conan-recipe"`.
- **FR-004**: Tool/build requirements declared in `conanfile.py` MUST be tagged with `mikebom:lifecycle-scope: "build"`; runtime `requires` MUST be tagged `mikebom:lifecycle-scope: "runtime"`.
- **FR-005**: When mikebom encounters a `west.yml` manifest at the scan root or in `<scan-root>/.west/`, it MUST parse `manifest.projects[]` and emit one SBOM component per project declaring `pkg:github/<remote>/<repo>@<revision>` (or `pkg:git+https://...@<revision>` if the remote URL is not a GitHub URL) with annotation `mikebom:source-mechanism: "zephyr-west"`.
- **FR-006**: When mikebom encounters one or more `idf_component.yml` files in the scan tree, it MUST extract the `dependencies:` map from each and emit one SBOM component per registry-named dependency with PURL `pkg:idf/<namespace>/<name>@<version>` (the speculative `idf` ecosystem name; see Clarifications) and annotation `mikebom:source-mechanism: "idf-component"`. The registry's source URL (typically the upstream GitHub repository for that component, retrieved from the manifest's `repository:`, `url:`, or registry-resolved metadata if locally available) MUST be recorded in the `mikebom:download-url` annotation so consumers that do not recognize `pkg:idf/` can fall back to source-URL identity. Local path-based dependencies MUST be annotated `mikebom:source-mechanism: "idf-component-local"` and emit as `pkg:generic/<name>`.
- **FR-007**: When mikebom encounters a vcpkg classic-mode installation tree (`vcpkg/installed/<triplet>/vcpkg/info/*.list`), it MUST emit one component per installed port with PURL `pkg:vcpkg/<name>@<version>` and annotation `mikebom:source-mechanism: "vcpkg-classic"`.
- **FR-008**: When mikebom encounters a `.gitmodules` file at the scan root, it MUST parse each submodule entry, look up the checked-out HEAD revision in the corresponding submodule directory, and emit one SBOM component per submodule with PURL `pkg:git+https://<url>@<commit-sha>` (or a derived ecosystem PURL when the URL maps cleanly to GitHub/GitLab/etc.) and annotation `mikebom:source-mechanism: "git-submodule"`. Every submodule MUST be emitted regardless of whether it is build-time-referenced; build-time correlation is reported via the `mikebom:linkage-kind` annotation per FR-008a.

- **FR-008a**: Each component emitted by the `git-submodule` reader MUST carry a `mikebom:build-reference` annotation whose value is one of `"declared-and-used"` (a `find_package(<name> ...)` call exists in any `CMakeLists.txt` under the scan root whose target name case-insensitively matches the submodule's last path segment, after target-alias resolution where the alias is statically visible) or `"declared-only"` (no matching `find_package` call). The cmake walker populates a global set of matched find_package target names first; submodules are then classified against the union. Walk order MUST NOT affect classification. `find_package` parsing is for correlation only — it MUST NOT cause component emission (the existing `find_package_does_not_emit_components` regression test stays passing).
- **FR-009**: For uninitialized submodules (entry in `.gitmodules` but empty directory), mikebom MUST still emit the component with `version: "unknown"` and a `mikebom:resolver-step: "uninitialized-submodule"` annotation; the scan MUST NOT fail.
- **FR-010**: Every new closed-enum `mikebom:source-mechanism` value introduced by this milestone (`cpm-cmake`, `zephyr-west`, `idf-component`, `idf-component-local`, `vcpkg-classic`, `git-submodule`) MUST be added to the C55 parity-catalog row's documented enum in `docs/reference/sbom-format-mapping.md`, and the parity-extractor for C55 MUST emit byte-identical values across CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output formats.
- **FR-011**: All new readers MUST be additive — emitting them MUST NOT break any existing golden fixture's byte-identity. New goldens are added for each new reader.
- **FR-012**: All new readers MUST work in `--offline` mode (no network access required during a scan). Version resolution from lockfiles or manifest revisions is local-file only.
- **FR-013**: A scan that encounters a parse error in any one of the new readers MUST emit a structured warning (per the existing `tracing` warn convention) and continue scanning; it MUST NOT abort the scan or fail the process exit code.
- **FR-014**: Each new reader MUST be polyglot-safe: when a directory tree contains manifests for multiple ecosystems, the new C/C++ readers MUST NOT prevent or interfere with other readers (npm, pip, cargo, etc.). Specifically: a `conanfile.py` deep inside `third_party/protobuf/` MUST NOT cause the protobuf submodule's own scans to be skipped.
- **FR-015**: When two or more readers produce components with the same canonical PURL, mikebom MUST emit exactly one component and select its `mikebom:source-mechanism` value deterministically by the following precedence (highest wins): (1) manifest-mode declarations — `vcpkg-manifest`, `vcpkg-classic`, `conan-recipe`, `cpm-cmake`, `zephyr-west`, `idf-component`, `idf-component-local`, `bazel-http-archive` — outrank (2) filesystem-derived signals — `git-submodule`, `cmake-vendored`. Within each tier, ties are broken by PURL specificity (Conan > GitHub > git+https > generic). All other readers' source-mechanism values that produced the same canonical PURL MUST be recorded on the winning component as a `mikebom:also-detected-via` annotation whose value is a JSON array of source-mechanism strings sorted lexicographically for determinism. Filesystem-walk order MUST NOT influence which reader wins.
- **FR-016**: Every URL emitted by the new readers in this milestone — whether as the host portion of a PURL, as a `mikebom:download-url` annotation, or embedded in any other URL-bearing annotation — MUST first be passed through the URL-sanitization helper shipped in milestone 075 (`strip-id-credentials`). The helper strips `user:password` segments from `https://` URLs, normalizes `ssh://`-with-credential URLs, and leaves credential-free URLs unchanged. When sanitization modifies a URL, a `tracing::warn!` event MUST be emitted naming the manifest file and a redacted form of the offending URL. Sanitization is unconditional — there is no opt-out flag in this milestone.

### Key Entities

- **Source mechanism**: a closed-enum string value attached as a `mikebom:source-mechanism` annotation to every component emitted by a C/C++ reader. Identifies *how* the component entered the build: `cmake-fetchcontent-git`, `cmake-fetchcontent-url`, `cmake-externalproject`, `cmake-vendored`, `bazel-http-archive`, `vcpkg-manifest`, `conan-recipe` (existing alpha.41 values) plus the new values from FR-010.
- **West manifest**: a YAML document at `<workspace>/west.yml` declaring a list of `projects[]`, each with `name`, `revision`, optional `remote`, optional `path`, and optional `groups`. Models a Zephyr workspace.
- **IDF component manifest**: a YAML document at `<component>/idf_component.yml` declaring a `dependencies` map of Espressif Component Manager registry names to version specifiers.
- **CPM call site**: a CMake function call (`cpmaddpackage`, `cpmfindpackage`, or `cpmdeclarepackage`) with keyword arguments describing a single dependency.
- **Conan recipe (Python)**: a `conanfile.py` Python file declaring a `ConanFile` subclass whose `requires` / `build_requires` / `tool_requires` class attributes or `requirements(self)` method enumerate dependencies.
- **Submodule entry**: an entry in `.gitmodules` declaring a `path` and `url`, paired with the checked-out revision of that path in the surrounding git working tree.
- **Also-detected-via trail**: a `mikebom:also-detected-via` annotation attached to a component whose canonical PURL was independently produced by two or more readers. Its value is a JSON array of the losing readers' source-mechanism values, sorted lexicographically. Used to preserve multi-reader-corroboration signal without producing duplicate components. New annotation introduced by this milestone; parity coverage is added to the catalog (new row C56, byte-identity symmetric across CDX / SPDX 2.3 / SPDX 3.0.1). CDX additionally emits the same signal natively in `evidence.identity[].methods[]` per the Phase 0 research R1 audit.
- **Build-reference**: a `mikebom:build-reference` annotation attached to a component emitted by the `git-submodule` reader (FR-008a). Closed enum: `"declared-and-used"` when the submodule's last-path-segment name case-insensitively matches a `find_package(...)` target in any scanned `CMakeLists.txt`, otherwise `"declared-only"`. Lets downstream vuln scanners filter un-referenced submodules with a single query. New annotation introduced by this milestone; parity coverage at catalog row C57, byte-identity symmetric across CDX / SPDX 2.3 / SPDX 3.0.1.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001** (CPM.cmake coverage): On the open-source `cpp-best-practices/cmake_template` project (or equivalent canonical CPM.cmake corpus), mikebom emits 100% of the dependencies declared via `cpmaddpackage(...)` as SBOM components with non-`unknown` versions when `VERSION` or `GIT_TAG` is declared.
- **SC-002** (Conan modernization): The number of conan-using fixture projects that mikebom successfully scans grows from ~50% (only `conanfile.txt`) to ~100% (`.txt` plus `.py`). Measured against a benchmark suite of 10 open-source conan-using projects (a mix of conanfile.txt and conanfile.py recipes).
- **SC-003** (Zephyr coverage): Scanning Zephyr v4.4.0's main repository emits at least 79 C/C++ components from the `west.yml` manifest (up from 0 today), each carrying a real PURL with the Git revision pinned in the manifest.
- **SC-004** (esp-idf coverage): Scanning a representative esp-idf project (any open-source ESP32 firmware sample with `idf_component.yml` manifests) emits at least 20 components from those manifests (up from 0 today).
- **SC-005** (gRPC submodule coverage): Scanning gRPC v1.69.0 with submodules populated emits at least 16 components from `.gitmodules` correlation (up from 3 today, which only catches the bazel-WORKSPACE entries).
- **SC-006** (Parity): All new readers' emitted components achieve byte-identity parity across CycloneDX 1.6, SPDX 2.3, and SPDX 3.0.1 output formats — verified by passing the existing `every_catalog_row_has_an_extractor` and parity round-trip test suite.
- **SC-007** (No regressions): All 33 existing golden fixtures continue to pass byte-identity after this milestone. Existing alpha.41 source-mechanism values (`cmake-fetchcontent-git`, `cmake-fetchcontent-url`, `cmake-externalproject`, `cmake-vendored`, `bazel-http-archive`, `vcpkg-manifest`, `conan-recipe`) emit identically to today.
- **SC-008** (Robustness): Scanning a polyglot project tree that contains manifests for multiple ecosystems (e.g., gRPC, which has Python + npm + maven + golang + cargo + gem tooling alongside C/C++) MUST complete successfully even when an unsupported manifest variant is present (e.g., npm package-lock v1) — no scan-abort. (This is a robustness criterion broader than the C/C++ readers themselves but exposed by the testing that motivated this milestone.)
- **SC-009** (Performance): Adding the new readers does not increase the scan wall-clock time for the existing golden-fixture corpus by more than 5%. Measured by comparing the post-milestone scan time against the alpha.41 baseline on the existing test corpus.
- **SC-010** (Dedup determinism): Given the same input tree, two scans run on different filesystems / in different orderings produce byte-identical SBOMs. Verified by a new dedicated test fixture in which two readers (e.g., `git-submodule` + `conan-recipe`) both match the same library; the chosen winner, the `mikebom:also-detected-via` array contents, and the SBOM byte output MUST be invariant across walk-order randomization.

## Assumptions

- **Scope boundary — CPM.cmake**: This milestone covers the most common CPM.cmake call shapes (`cpmaddpackage` with `NAME` + `GITHUB_REPOSITORY` + `GIT_TAG` or `VERSION`). Less common shapes — `cpm_default_*` configuration, `CPM_DOWNLOAD_LOCATION` overrides, custom CMake variables interpolated into URLs — are best-effort. Custom-function wrappers around `cpmaddpackage` are out of scope.
- **Scope boundary — `conanfile.py`**: Parsing is regex / AST-light, not full Python execution. Dynamic `requires` (e.g., `self.requires(f"{name}/{version}")` with computed strings) cannot be resolved and will be skipped with a structured warning.
- **Scope boundary — `west.yml`**: `manifest.import:` directives that pull in manifests from other west projects are NOT chased transitively in this milestone (deferred to a future "west-transitive" milestone if demand emerges).
- **Yocto is out of scope** for milestone 105: per the Clarifications session, the Yocto / OpenSTLinux recipe reader (and the `pkg:bitbake/...` PURL question) is split into a follow-on milestone. Milestone 105 delivers exactly US1–US6.
- **PURL types**: Settled in Clarifications. Existing PURL ecosystems are used where they map cleanly (`pkg:github/`, `pkg:conan/`, `pkg:vcpkg/`, `pkg:git+https://`). For Zephyr's west modules, GitHub-hosted projects use `pkg:github/<org>/<repo>@<rev>`; non-GitHub remotes fall back to `pkg:git+https://<url>@<rev>`. For esp-idf registry components, the speculative `pkg:idf/<namespace>/<name>@<version>` form is used with the upstream source URL recorded in `mikebom:download-url` as fallback identity. A package-url spec ecosystem-registration request for `idf` is filed as a milestone documentation deliverable.
- **No new Cargo dependencies**: Per the existing milestone-100+ pattern, this work uses only crates already in the workspace dependency closure (`serde`, `serde_json`, `serde_yaml` if needed for `west.yml`/`idf_component.yml`, `regex`, `quick-xml`, `tracing`, `anyhow`). `serde_yaml` is the only candidate addition; if it's not already transitively present, planning evaluates whether to add it as a direct dep or hand-roll a minimal YAML shape parser.
- **No new subprocesses**: All readers operate on local files. No `git` shell-outs needed for submodules — the checked-out revision can be read from `.git/modules/<path>/HEAD` (already a local-file read).
- **Network independence**: All version resolution is local. We do NOT fetch from the Conan Center, Espressif Component Registry, or vcpkg registries during a scan.
- **Constitution alignment**: All new readers respect Constitution Principle V (native fields take precedence over `mikebom:*` annotations) — annotation usage is justified per the existing alpha.41 audit pattern (source-mechanism has no native field across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1, validated in catalog row C55).
- **Constitution alignment**: Annotation byte-identity across all three formats is mandatory (Constitution Principle X). The C55 row's enum is expanded; the extractor is unchanged.
- **Reuse of existing infrastructure**: All new readers slot into the existing `mikebom-cli/src/scan_fs/package_db/` reader-dispatch architecture. No new top-level architecture is introduced; this is pure additive coverage expansion.
