# Feature Specification: CMake walker depth extension — reach nested `Find*.cmake` files

**Feature Branch**: `156-cmake-walker-depth`
**Created**: 2026-07-02
**Status**: Draft
**Input**: User description: "extend CMake walker to depth-2 within cmake/, Modules/, third_party/ to reach nested Find*.cmake files — closes the Kamailio walker-scope gap identified in milestone-155 F1 remediation"

## Origin & context

Milestone 155 reversed milestone-102's FR-007 refusal and enabled `find_package` + `pkg_check_modules` extraction from CMake source trees. But its Kamailio testbed hit only **1** identified component (`OpenSSL 1.1.0` from `cmake/defs.cmake`) — well below the whole-tree `find_package` call count of ~10. During `/speckit-analyze` F1 remediation, an empirical grep revealed the root cause: 9+ of Kamailio's declared deps live in `cmake/modules/Find*.cmake` files at **depth-2** relative to the scan root, and mikebom's `discover_cmake_files` helper only reads depth-1 children of the well-known CMake directories (`cmake/`, `Modules/`, `third_party/`).

Concretely, `discover_cmake_files` at `mikebom-cli/src/scan_fs/package_db/cmake.rs:195-223` calls `std::fs::read_dir` (one-level iteration) on each of those subdirs — so `<root>/cmake/defs.cmake` is discovered but `<root>/cmake/modules/FindOpenSSL.cmake` is not.

The milestone-155 spec + PR body explicitly named walker-depth extension as a separate follow-up milestone opportunity. This is that follow-up.

**Verified 2026-07-02 against `/Users/mlieberman/Projects/kamailio`**:

```
depth-1 find_package calls: 1  (OpenSSL 1.1.0 in cmake/defs.cmake)
depth-2 find_package calls: 2+ (already grep-verified during milestone-155 F1);
                                actual reachable count once walker
                                extends: 9-10 declared deps
                                (Libev, NETSNMP, MariaDBClient,
                                LibfreeradiusClient, Radius, Ldap,
                                Unistring, Erlang, Oracle,
                                plus whatever else is in cmake/modules/)
```

The design keeps everything narrow: recursive descent **only under the two "project's own CMake" top-level directories** (`cmake/`, `Modules/`) by default; `third_party/` stays at depth-1 (matching milestone-102 behavior) but a new opt-in flag `--cmake-third-party-recursive` extends the recursion to `third_party/` too. No new top-level directories added, no `src/**/CMakeLists.txt` recursive walk (that's a separate future milestone with different failure modes — generated CMakeLists.txt in build/, deeply nested project layouts).

## Clarifications

### Session 2026-07-02

- Q: Should the recursive walker also descend into `third_party/` by default? Vendoring large trees (LLVM ~3000 .cmake files, Chromium, etc.) would emit that dep's whole transitive graph as parent-project declared deps. → A: **Depth-1 default for `third_party/`, opt-in to recursive via a new flag**. `cmake/` and `Modules/` are the "project's own CMake" dirs and get recursive descent (that's where the Kamailio Find*.cmake files live); `third_party/` is treated as "someone else's tree" and stays at depth-1 unless the operator explicitly opts in via `--cmake-third-party-recursive`. Auditors of narrow-scope projects (Kamailio) get their win; auditors of projects with vendored deps get sane default noise levels; auditors who WANT the vendored deps' transitive dep declarations opt in with one flag. Follow-up milestones can extend to per-dir depth caps if operator demand surfaces.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Compliance auditor scanning a Kamailio-shaped tree gets the full declared-dep roster (Priority: P1)

A compliance auditor holding a Git checkout (or `.tar.gz`) of a mature C/C++ project whose CMake tree uses the "Find scripts nested under cmake/modules/" pattern (Kamailio being the canonical example) runs mikebom to inventory its declared external dependencies. Post-milestone-155 they got 1 identified component (OpenSSL). Post-milestone-156 they get one component per distinct `find_package(<Name>)` call across the whole `cmake/**/*.cmake` (or `Modules/**/*.cmake`, `third_party/**/*.cmake`) tree — hitting the ≥10 whole-tree count the milestone-155 origin story anticipated.

**Why this priority**: The direct closure of the milestone-155 F1 remediation debt. Milestone 155 shipped as walker-scope-honest (≥1 floor) with an explicit "walker-depth extension is a separate future milestone" callout. This IS that milestone — no scope inflation, no design changes to the emission shape, just extending the file-discovery scope.

**Independent Test**: Run mikebom against a fresh Kamailio checkout at `/Users/mlieberman/Projects/kamailio`. Assert: (a) `≥10` identified components carry `mikebom:source-mechanism = "cmake-find-package"` OR `"cmake-pkg-check-modules"`; (b) the specific expected names (OpenSSL, Libev, NETSNMP, MariaDBClient, LibfreeradiusClient, Radius, Ldap, Unistring, Erlang, Oracle) appear at their expected PURLs (`pkg:generic/openssl@1.1.0`, `pkg:generic/libev`, etc.); (c) each emitted component's `mikebom:source-files` annotation names the deepest file where its declaration appeared (e.g., `cmake/modules/FindOpenSSL.cmake` for the OpenSSL find_package call in Kamailio's Find script).

**Acceptance Scenarios**:

1. **Given** a scan target with `find_package(Libev)` in `<root>/cmake/modules/FindLibev.cmake` (depth-2), **When** mikebom scans the tree, **Then** the emitted SBOM MUST contain a component with `purl = "pkg:generic/libev"`, `mikebom:source-mechanism = "cmake-find-package"`, and `mikebom:source-files` naming the depth-2 path.
2. **Given** a scan target with a nested `.cmake` file at arbitrary depth under `cmake/` (e.g., `<root>/cmake/modules/vendor/Extra.cmake` at depth-3), **When** mikebom scans, **Then** any `find_package` / `pkg_check_modules` calls in that file MUST be extracted with the same milestone-155 emission shape as depth-1 emissions.
3. **Given** two declarations of the same package at different depths (e.g., `find_package(OpenSSL 1.1.0)` at depth-1 in `cmake/defs.cmake` AND `find_package(OpenSSL 3.0)` at depth-2 in `cmake/modules/FindOpenSSL.cmake`), **When** mikebom scans, **Then** the Q1 highest-version-wins rule from milestone 155 applies unchanged — the emitted PURL is `pkg:generic/openssl@3.0` and both source-file paths are captured in the merged component's `mikebom:source-files` annotation.
4. **Given** a scan target that contains a symlink cycle inside `cmake/` (e.g., `cmake/loop` → `cmake/`), **When** mikebom scans, **Then** the walker MUST NOT infinite-loop; each `.cmake` file MUST be read at most once; the scan MUST complete in bounded time.
5. **Given** a scan target with `find_package(Foo)` declared in a file that the operator excluded via `--exclude-path cmake/modules/`, **When** mikebom scans, **Then** the excluded file MUST NOT be walked; no component from that file MUST be emitted.

---

### User Story 2 — Existing depth-1 emissions unchanged (Priority: P2)

An operator running mikebom against any pre-existing CMake fixture (the milestone-090 goldens; the milestone-102/103 fetchcontent + vendored fixtures; the milestone-155 Kamailio-shape fixture) expects byte-identical output vs post-milestone-155-pre-156 — as long as those fixtures don't contain new files at depth-2+ that would now be discovered.

**Why this priority**: SC-002 byte-identity guard. Milestone 156 MUST NOT regress any pre-existing fixture that only has depth-1 `.cmake` files. Regression here signals accidental scope creep.

**Independent Test**: Run `cargo test --workspace --no-fail-fast` and verify every pre-existing golden test (CDX cmake / SPDX 2.3 cmake / SPDX 3 cmake plus all 10 other ecosystem goldens) passes without regenerate. Additionally, milestone-155's SC-004 Kamailio-shape fixture at `mikebom-cli/tests/fixtures/cmake-find-package/kamailio-shape/` (which contains a depth-2 `cmake/modules/FindLibev.cmake` file) MUST continue to emit exactly the same 5 cmake-find-package + 1 cmake-pkg-check-modules components — its depth-2 file's `find_package_handle_standard_args(Libev ...)` call is intentionally the NON-emitting FR-009 pattern, so the count stays at 5+1.

**Acceptance Scenarios**:

1. **Given** the milestone-090 `cmake` fixture (top-level CMakeLists.txt with FetchContent + ExternalProject + `find_package(OpenSSL REQUIRED)` + include of `cmake/third_party.cmake` — all depth-1), **When** mikebom scans, **Then** the emitted CDX MUST be byte-identical to the post-milestone-155 golden (same component count, same emissions, same annotations).
2. **Given** the milestone-155 Kamailio-shape fixture, **When** mikebom scans, **Then** the emitted component counts stay 5 cmake-find-package + 1 cmake-pkg-check-modules; the depth-2 `FindLibev.cmake` file's `find_package_handle_standard_args(Libev ...)` call MUST NOT emit (FR-009 boundary unchanged); no new components appear in the SBOM.

---

### Edge Cases

- **Symlink cycles**: as covered in US1 A4. Reuse milestone-054's `safe_walk` visited-set pattern (canonicalize each candidate path and skip if already visited during THIS walker invocation).
- **Symlinks pointing outside the scan root**: skip. Milestone-054's precedent: staying inside the scan root prevents surprise fs access.
- **Build-tree contamination**: CMake generates a lot of `.cmake` files in build directories (`build/`, `cmake-build-debug/`, `out/`, etc.). Milestone 156 does NOT auto-exclude these — operators can add `--exclude-path build,out,cmake-build-*` via the existing milestone-113 flag if noise arises. Rationale: some projects genuinely have a `build/` directory in source (rare but exists); auto-exclusion by name would break them. Documented in the CHANGELOG.
- **Vendored deps with their own `find_package` calls**: `<root>/third_party/somedep/cmake/*.cmake` (depth-2+ within `third_party/`) may contain `find_package(SomeDepDep)` calls for the vendored project's own transitive deps. By default (Q1 clarification 2026-07-02) milestone 156 does NOT walk these — `third_party/` stays at depth-1. Operators wanting the full transitive vendored-dep tree opt in via `--cmake-third-party-recursive` (FR-019). Prevents surprise-huge SBOMs for projects that vendor LLVM/Chromium/WebRTC/etc. (thousands of `.cmake` files with hundreds of `find_package` calls in the vendored tree, most of which document the vendored dep's transitive deps rather than the parent project's).
- **Extremely deep hierarchies**: no hard depth cap. Milestone-054's visited-set (path canonicalization) prevents cycles; a hierarchy that's a legitimate tree of 500 nested subdirs is walked completely.
- **Case-sensitivity on macOS/Windows**: mikebom's existing `.cmake` extension check uses `eq_ignore_ascii_case`; the extended walker keeps that behavior.
- **Non-CMake files with `.cmake` extension inside build dirs** (e.g., `build/CMakeFiles/Progress.cmake` — CMake-emitted): if the operator hasn't excluded `build/`, these get walked. Parsing them is safe (the milestone-155 regex is comment-strip + boundary-aware); they may or may not emit — in most cases they contain no `find_package` / `pkg_check_modules` calls so are silent noise. If they DO happen to contain a call, that's the operator's call to filter via `--exclude-path`.
- **`.cmake` file that's a package DEFINITION** (i.e., the `Find<Name>.cmake` file for the named package itself, not a place where `find_package` gets CALLED): mikebom's regex extracts every `find_package` call site regardless of whether the containing file is a `Find<Name>.cmake` script or a top-level CMakeLists.txt. Under Kamailio's pattern (`FindNETSNMP.cmake` contains `find_package(NETSNMP ...)` as its internal call), the extraction correctly emits `pkg:generic/netsnmp`. This is exactly the desired behavior per US1 A2.

## Requirements *(mandatory)*

### Functional Requirements

#### Core walker extension (US1)

- **FR-001**: The `discover_cmake_files` helper at `mikebom-cli/src/scan_fs/package_db/cmake.rs:195` MUST perform recursive descent under `<scan_root>/cmake/` and `<scan_root>/Modules/`. Discovery captures every `.cmake` file AND every `CMakeLists.txt` file at any depth beneath those two top-level dirs. `<scan_root>/third_party/` MUST continue to be walked at depth-1 (matching milestone-102 behavior) UNLESS the operator opts in via `--cmake-third-party-recursive` (see FR-019 + FR-020).

- **FR-019 (NEW)**: mikebom MUST expose a new CLI flag `--cmake-third-party-recursive` (boolean, default `false`) on the `mikebom sbom scan` subcommand. When set, the extended walker MUST apply the same recursive-descent behavior to `<scan_root>/third_party/` as it does to `<scan_root>/cmake/` and `<scan_root>/Modules/` per FR-001. When unset, `<scan_root>/third_party/` stays at depth-1 (milestone-102 behavior preserved). Rationale: keeps the SC-001 Kamailio win noise-free by default while giving operators of projects with vendored deps (LLVM, Chromium, WebRTC, etc.) an explicit opt-in path to reach the vendored tree's transitive dep declarations.

- **FR-020 (NEW)**: When `--cmake-third-party-recursive` is set AND the extended walker discovers new `.cmake` files under `<scan_root>/third_party/<dep>/**/`, the emitted `PackageDbEntry` instances MUST carry the SAME milestone-155 shape as any other `find_package` emission (per FR-006). No additional annotation is emitted to mark "this came from a vendored tree" — consumers can inspect `mikebom:source-files` to see the path prefix and filter downstream if desired.

- **FR-002**: The top-level scan-root scan MUST continue to include `<scan_root>/CMakeLists.txt` unchanged (milestone-102 behavior preserved).

- **FR-003**: Recursive descent MUST be safe against symlink cycles. Implementation MUST use a canonicalized-path visited set per walker invocation (matching milestone-054's `safe_walk` pattern). Each file MUST be read at most once per scan.

- **FR-004**: Recursive descent MUST NOT follow symlinks that resolve outside the scan root (matching milestone-054's `safe_walk` cross-root boundary check).

- **FR-005**: Recursive descent MUST honor the milestone-113 `--exclude-path` flag. Any file whose path (or ancestor path) matches an operator-supplied exclusion pattern MUST NOT be walked, MUST NOT be read, and MUST NOT contribute to emission.

#### Emission shape preservation (US2)

- **FR-006**: Every `find_package(<Name> [<Version>])` call site found by the extended walker MUST emit a `PackageDbEntry` with the SAME milestone-155 shape: `pkg:generic/<lowercased-name>[@<highest-declared-version>]`, `mikebom:source-mechanism = "cmake-find-package"`, `mikebom:cmake-find-package-name` annotation when original casing differs, `mikebom:source-files` naming the extraction site. No new annotation keys introduced.

- **FR-007**: Every `pkg_check_modules(<TARGET> ...)` and `pkg_search_module(...)` call site found by the extended walker MUST emit with mechanism `"cmake-pkg-check-modules"` per milestone-155 FR-003 / FR-004 shape. No changes.

- **FR-008**: The Q1 highest-declared-version-wins rule (milestone-155 FR-002) MUST apply across the FULL discovered file set — depth-1 and depth-2+ declarations of the same package name consolidate via the same lowercased-name grouping.

- **FR-009**: FR-009 from milestone 155 (`find_package_handle_standard_args(...)` MUST NOT extract) applies uniformly to files at ALL depths. The extended walker discovers `FindLibev.cmake` at depth-2; its `find_package_handle_standard_args(Libev ...)` still MUST NOT emit a component.

#### Byte-identity + reader-scope safeguards

- **FR-010**: For scan targets whose `cmake/`, `Modules/`, `third_party/` subtrees contain ONLY depth-1 `.cmake` files (i.e., no nested subdirectories with additional `.cmake` files), milestone 156's emitted output MUST be byte-identical to milestone-155's emitted output. Verified via SC-002 against pre-existing golden fixtures.

- **FR-011**: This milestone MUST NOT change the milestone-102/103 FetchContent / ExternalProject / vendored extraction paths. Those existing paths already run against the discovered file list; extending the file list does not change how each file is parsed.

- **FR-012**: This milestone MUST NOT change milestone-155's parsing regex, emission logic, or annotation shape. The scope is EXCLUSIVELY the `discover_cmake_files` helper.

- **FR-013**: This milestone MUST NOT change any OTHER reader (dpkg, rpm, apk, vcpkg, conan, language-ecosystem, binary-tier).

- **FR-014**: This milestone MUST NOT change the milestone-133 file-tier walker or content-shape allowlist. Newly-walked files that DO contain `find_package` calls escape the file-tier orphan bucket (they now emit package-tier components); files that DON'T contain such calls (build-tree noise, if not excluded) continue to be either silently walked-and-empty or classified as orphans per the existing file-tier flow.

- **FR-015**: This milestone MUST NOT introduce a new `mikebom:*` annotation key, a new CDX property, a new SPDX annotation, or a new PURL type.

- **FR-016**: This milestone MUST NOT add any new Cargo dependency. Recursive descent uses std (`std::fs::read_dir` chained recursively with `std::fs::canonicalize` for the visited set) OR the milestone-114 `safe_walk` helper if the existing shape fits.

#### Broader scope exclusions (US2 boundary)

- **FR-017**: This milestone MUST NOT extend discovery beyond the existing three top-level directories. `<scan_root>/src/**/CMakeLists.txt`, `<scan_root>/tests/**/*.cmake`, `<scan_root>/docs/**/*.cmake`, and any other top-level tree remain UN-walked by the CMake reader in this milestone. Add_subdirectory chain following is a separate future milestone with different design considerations.

- **FR-018**: This milestone MUST NOT auto-exclude well-known CMake build directories (`build/`, `cmake-build-*/`, `out/`, etc.). Operators encountering noise from build-tree contamination MUST use the existing milestone-113 `--exclude-path` flag to filter. The CHANGELOG entry MUST document this recommendation.

### Key Entities

- **`discover_cmake_files` helper**: the file-discovery function at `mikebom-cli/src/scan_fs/package_db/cmake.rs:195`. Currently returns depth-1 discovered files; milestone 156 extends it to recursive discovery under the three well-known top-level dirs.
- **Kamailio testbed**: the concrete verification target at `/Users/mlieberman/Projects/kamailio`. Currently emits 1 identified component post-milestone-155; milestone 156 raises this to ≥10.
- **Milestone-155 Kamailio-shape fixture**: the synthetic testbed at `mikebom-cli/tests/fixtures/cmake-find-package/kamailio-shape/` that already contains a depth-2 `cmake/modules/FindLibev.cmake` file. Post-milestone-156 this file becomes discoverable — its `find_package_handle_standard_args(Libev ...)` call MUST NOT emit (FR-009 boundary verification at depth-2).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (Kamailio testbed roster)**: After milestone 156 ships, `mikebom sbom scan --path /path/to/kamailio --format cyclonedx-json` MUST produce ≥10 components carrying `mikebom:source-mechanism = "cmake-find-package"` OR `"cmake-pkg-check-modules"` (up from 1 post-milestone-155). The specific expected names — OpenSSL, Libev, NETSNMP, MariaDBClient, LibfreeradiusClient, Radius, Ldap, Unistring, Erlang, Oracle — MUST all appear at their expected `pkg:generic/*` PURLs. Verified via manual operator-cadence per quickstart.md Scenario 1.

- **SC-002 (byte-identical guard for depth-1-only trees)**: For any pre-existing fixture with only depth-1 `.cmake` files (milestone-090 cmake fixture; milestone-102/103 fixtures; the milestone-155 Kamailio-shape fixture's depth-1 files), milestone 156's emitted CDX / SPDX 2.3 / SPDX 3 MUST be byte-identical to milestone-155's emissions. The milestone-155 Kamailio-shape fixture stays at 5 cmake-find-package + 1 cmake-pkg-check-modules; the newly-discovered depth-2 `FindLibev.cmake` file contains a `find_package_handle_standard_args` call which correctly does NOT emit per FR-009.

- **SC-003 (symlink cycle safety)**: A synthetic testbed with a symlink loop inside `cmake/` (e.g., `cmake/loop -> cmake/`) MUST scan to completion in bounded time (<5s) without infinite-looping. Each `.cmake` file MUST be read at most once. Verified via a new integration test.

- **SC-004 (nested depth extraction — synthetic testbed)**: A new synthetic testbed with a `find_package(Foo)` at depth-3 (`cmake/modules/vendor/Extra.cmake`) MUST emit a `pkg:generic/foo` component with `mikebom:source-mechanism = "cmake-find-package"` and `mikebom:source-files` naming the depth-3 path. Verified via a new integration test.

- **SC-005 (multi-depth version consolidation)**: The Q1 highest-version-wins rule MUST work across depth-1 + depth-2 declarations of the same package. A synthetic testbed with `find_package(OpenSSL 1.1.0)` at depth-1 AND `find_package(OpenSSL 3.0)` at depth-2 MUST emit exactly one `pkg:generic/openssl@3.0` component whose `mikebom:source-files` names BOTH file paths.

- **SC-006 (exclude-path integration)**: A synthetic testbed with `find_package(Foo)` at `cmake/modules/FindFoo.cmake` scanned with `--exclude-path cmake/modules/` MUST emit ZERO cmake-find-package components (excluded file not walked). Verified via a new integration test.

- **SC-007 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-156 main — clippy clean + every test passes except the documented `sbomqs_parity` env-only flake. Given the SC-002 byte-identity guard, no golden regeneration should be required.

- **SC-008 (unit-test coverage)**: At least 6 new tests covering: (a) depth-2 `.cmake` file discovery + emission; (b) depth-N (N≥3) `.cmake` file discovery; (c) symlink cycle safety; (d) `--exclude-path` under a nested dir; (e) cross-depth version consolidation; (f) FR-009 boundary at depth-2 (`find_package_handle_standard_args` in a discovered depth-2 file does NOT emit).

- **SC-009 (CHANGELOG entry)**: The shipped diff MUST include an entry in `CHANGELOG.md` under `[Unreleased]` naming: (a) the walker-depth extension; (b) the Kamailio testbed impact (from 1 identified component → ≥10); (c) the exclude-path recommendation for build-tree contamination; (d) the reference back to milestone-155's F1 remediation.

- **SC-010 (no wire-format changes)**: No new `mikebom:*` annotation key. No new `docs/reference/sbom-format-mapping.md` catalog row. No CDX / SPDX 2.3 / SPDX 3 emitter code changes. The shipped diff's file-list MUST show only `cmake.rs`, the CLI arg-struct wiring for the new flag (`sbom_scan.rs` or wherever `mikebom sbom scan` args live), new/updated test files, new fixture directories, and the CHANGELOG entry.

- **SC-011 (opt-in flag off-by-default)**: A synthetic testbed with `find_package(VendoredDepDep)` at `<root>/third_party/somedep/cmake/deps.cmake` (depth-3 within third_party/) MUST emit ZERO components under `--cmake-third-party-recursive` NOT set (default). WITH `--cmake-third-party-recursive` set, MUST emit exactly one `pkg:generic/vendoreddepdep` component. Verified via a new integration test.

## Assumptions

1. **The milestone-155 emission shape is stable**: milestone 156 does NOT change how `find_package` calls are parsed or emitted. It only expands the set of files that get parsed. All 155-era annotations, PURL construction, dedup behavior, and Q1/Q2 clarifications carry forward unchanged.

2. **Kamailio's tree structure at 2026-07-02 HEAD is representative**: 9-10 additional `find_package` calls in `cmake/modules/Find*.cmake` per empirical grep during milestone-155 F1 remediation. Actual post-156 count may differ if Kamailio HEAD has moved by shipping time; SC-001's ≥10 floor accommodates this via the "expected names appear" clause rather than a strict count.

3. **Recursive descent under 3 top-level dirs is a natural scope**: extending walker breadth (adding new top-level dirs like `src/`) is deliberately out of scope. The Kamailio gap is specifically depth-2 under `cmake/`; other projects with unusual layouts (e.g., `MyProject/build-modules/*.cmake` at the root) are not covered — they need a further scope extension milestone if operator demand surfaces.

4. **Build-tree contamination is an operator-managed concern**: mikebom does NOT auto-exclude `build/`, `cmake-build-*/`, `out/`, etc. Operators encountering CMake-generated `.cmake` files in those trees can filter via `--exclude-path`. Auto-exclusion by name would break the (rare) project that genuinely has a `build/` directory in source without a build system inside.

5. **Vendored deps' own `find_package` calls are OPT-IN emissions** (per Q1 clarification 2026-07-02): `<root>/third_party/somedep/cmake/**/*.cmake` files (at depth-2+ within `third_party/`) are NOT walked by default. Depth-1 `<root>/third_party/*.cmake` and `<root>/third_party/CMakeLists.txt` files ARE walked (unchanged from milestone 102). Operators wanting the full transitive vendored-dep tree opt in via `--cmake-third-party-recursive`. Rationale: keeps the SC-001 Kamailio win noise-free (Kamailio has no `third_party/`, so behavior is identical either way) while preventing surprise-huge SBOMs for projects that vendor LLVM/Chromium/etc.

6. **`add_subdirectory` chain following is out of scope**: mikebom does NOT recursively walk `<root>/src/**/CMakeLists.txt`. Projects using nested `add_subdirectory(...)` chains for their own source tree are handled by mikebom's other readers (binary-tier / file-tier / package-DB readers scoped per ecosystem). A future milestone could add static `add_subdirectory` chain following if operator demand surfaces, but that's a bigger design problem (generated CMakeLists.txt in build trees, arbitrary path expressions, `${CMAKE_SOURCE_DIR}` interpolation) than milestone 156's tightly-scoped depth extension.

7. **The milestone-114 `safe_walk` helper is the reference pattern for cycle safety**: this milestone's implementation SHOULD reuse or match `safe_walk`'s visited-set + canonicalized-path pattern. Constitution Principle I compliance (Pure Rust, Zero C) preserved — std-only recursive descent.

8. **Recursion depth is unbounded (visited-set-protected)**: no hard N-level cap. Milestone-054's canonicalized-path visited-set already prevents infinite loops; genuine hierarchies of 100+ levels get walked completely. Practical performance is bounded by the total `.cmake` file count on disk, which is small for realistic projects (Kamailio has ~20 `.cmake` files in the whole tree).

## Dependencies

- **Milestone 155** (the emission code path being fed by the extended walker). Milestone 156 depends on milestone 155 having landed — the extended walker's newly-discovered files pass through the milestone-155 parse + emit pipeline unchanged.
- **Milestone 102 / 103** (the `discover_cmake_files` helper being modified). Only the recursion behavior changes; the top-level-dir set (`cmake/`, `Modules/`, `third_party/`) and the file-type filter (`.cmake` extension + `CMakeLists.txt` name) are preserved.
- **Milestone 054** (`safe_walk` pattern). The visited-set + canonicalized-path approach is the reference; either invoke `safe_walk` directly or replicate the pattern inline.
- **Milestone 113** (`--exclude-path` flag). The extended walker MUST integrate with this flag so operators can prune build-tree contamination.

## Out of Scope

- No recursive walk of `<root>/src/**/CMakeLists.txt` (per FR-017 + Assumption 6).
- No new top-level scan-root directories beyond `cmake/`, `Modules/`, `third_party/`.
- No `add_subdirectory` chain following (add_subdirectory targets that name paths outside the existing 3 dirs).
- No `include(...)` directive resolution / following.
- No `CMAKE_MODULE_PATH` cache variable evaluation.
- No `find_package` MODULE-mode vs CONFIG-mode differentiation (milestone-155 emits both uniformly).
- No auto-exclusion of well-known CMake build directories (per FR-018 + Assumption 4).
- No new `mikebom:*` annotation keys (per FR-015).
- No new CDX / SPDX 2.3 / SPDX 3 emitter code paths (per SC-010).
- No catalog row additions in `docs/reference/sbom-format-mapping.md` (nothing new to document — the C55 + C103 rows from milestone 155 cover everything this milestone touches).
- No changes to milestone-155's parsing regex, emission logic, or Q1/Q2 clarifications.
- No changes to any other reader or to the file-tier walker.
