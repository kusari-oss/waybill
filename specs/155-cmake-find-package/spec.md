# Feature Specification: CMake `find_package` + `pkg_check_modules` extraction — closing the source-tree-only C/C++ visibility gap

**Feature Branch**: `155-cmake-find-package`
**Created**: 2026-07-02
**Status**: Draft
**Input**: User description: "155 — extend the CMake reader to parse `find_package` + `pkg_check_modules` declarations; source-tree scans of C/C++ projects currently emit zero identified components"

## Origin & context

Ad-hoc verification against the [Kamailio SIP server source tree](https://github.com/kamailio/kamailio) surfaced the gap. Kamailio is a mature C-language open-source project (~1400 `.c` files + ~1350 `.h` files, CMake-based build system, no vendored deps, no package manifest of its own). It declares its external dependencies exclusively via CMake:

```cmake
# cmake/defs.cmake
find_package(OpenSSL 1.1.0)

# cmake/modules/Find*.cmake
find_package(Libev ...)
find_package(NETSNMP ...)
find_package(MariaDBClient ...)
find_package(LibfreeradiusClient ...)
find_package(Radius ...)
find_package(Ldap ...)
find_package(Unistring ...)
find_package(Erlang ...)
find_package(Oracle ...)

# and via pkg-config:
pkg_check_modules(RADIUS REQUIRED IMPORTED_TARGET radcli)
```

10+ external declared dependencies with version constraints across the whole tree, none of which end up in the mikebom SBOM. **However, walker-scope matters here** (see next paragraph): only 1 of those `find_package` calls lives at Kamailio's depth-1 `cmake/*.cmake` — the rest live at depth-2 inside `cmake/modules/Find*.cmake`, which mikebom's `discover_cmake_files` helper does NOT walk in its current form. Milestone 155 as scoped closes the parsing gap at depth-1 (the code-behavior change); walker-depth extension to reach the remaining 9+ calls is a separate future milestone (see FR-013 + Assumption 1 + Assumption 6).

**What mikebom currently produces for this scan target** (mikebom at HEAD, `mikebom sbom scan --path /path/to/kamailio --format cyclonedx-json`):

```
scan complete components=0 relationships=0
file-tier walker complete file_tier_components=111 mode=Orphan shape_skipped=6759
SBOM written components=111
```

- **Zero components identified** by any package-DB / language-ecosystem / CMake / binary-tier reader.
- **111 file-tier "orphan" components** (milestone 133 US1.C default) — but almost all are noise: shell scripts, sample git hooks, test-framework config files. None describe Kamailio's actual declared dependencies.
- **6759 files skipped** by the content-shape allowlist — the actual C/H sources fall in this bucket (correctly, per milestone 133 design).

**Empirical depth-1 count for Kamailio** (verified 2026-07-02 during `/speckit-analyze` remediation via `grep -hE '^[^#]*\bfind_package\s*\(' CMakeLists.txt cmake/*.cmake` on `/Users/mlieberman/Projects/kamailio`):

- Depth-1 `find_package` calls: **1** (`find_package(OpenSSL 1.1.0)` in `cmake/defs.cmake:164`)
- Depth-1 `pkg_check_modules` calls: **0**
- Depth-2 `find_package` calls (in `cmake/modules/Find*.cmake` — NOT walked): 2+ (would raise the count with a walker-depth-extension milestone)

The whole-tree "10+" claim above is a raw grep count that IGNORES walker depth. Milestone 155's SC-001 uses the depth-1 empirical count (≥1) as its honest floor.

**Why the CMake reader is silent**: milestones 102 + 103 (`mikebom-cli/src/scan_fs/package_db/cmake.rs`) explicitly refuse to parse `find_package` per FR-007 of milestone 102. The stated rationale:

> "`find_package(X)` declarations are NOT parsed per FR-007 — they resolve to system-installed packages and would double-count against OS-package readers + vcpkg + Conan."

That rationale held when the primary mikebom use case was scanning installed rootfs (where dpkg / rpm / apk catch OpenSSL via `libssl` package). It breaks down for **source-tree-only scans** — the case where the operator holds a `.tar.gz` or a Git checkout of a C/C++ project and wants to know its declared dependencies without doing a full system install.

Since milestone 102 shipped, the workspace has gained the milestone-105 dedup pipeline (`SourceMechanism` enum + `mikebom:also-detected-via` collision merging). This lets us emit `find_package`-derived entries with a `mikebom:source-mechanism = cmake-find-package` tag, and the existing dedup infrastructure cleanly merges them with OS-package-tier or vcpkg/Conan-tier entries when they collide. The double-counting concern is empirically resolved.

This milestone reverses milestone-102 FR-007 (and adds `pkg_check_modules` handling too), tagging emitted entries with the appropriate `mikebom:source-mechanism` values so the production `resolve::deduplicator` pass merges same-canonical-PURL entries automatically (e.g., a `find_package(openssl 1.1.0)` and a `FetchContent_Declare(openssl URL ...openssl-1.1.0.tar.gz)` both produce `pkg:generic/openssl@1.1.0` and merge). Cross-namespace transparency (via milestone-105 `scan_fs::dedup`'s `mikebom:also-detected-via` list) is out of milestone-155 scope — see §User Story 2 cross-tier note.

## Clarifications

### Session 2026-07-02

- Q: When the same package is declared at different versions across multiple `.cmake` files (e.g., `find_package(OpenSSL 1.1.0)` in one file and `find_package(OpenSSL 3.0)` in another), which version does mikebom emit in the PURL? → A: **Highest declared version wins**. CMake's `find_package(<Name> <Version>)` treats `<Version>` as a MINIMUM — the highest declared floor is the strictest lower-bound the project's build demands. Deterministic across scans of the same tree independent of file-walk order. Vulnerability scanners get the more-conservative signal.
- Q: How does mikebom handle CMake meta / build-tool packages like `find_package(Threads)`, `find_package(PkgConfig)`, `find_package(Doxygen)`, `find_package(Git)`, `find_package(Python3)`, `find_package(GNUInstallDirs)` that are not runtime dependencies? → A: **Emit uniformly** — no denylist, no build-tool role tagging in this milestone. Constitution Principle X (transparency) favors showing what's in the CMake tree; consumers filter by name post-emission if desired. A curated build-tool classifier is a natural follow-up milestone if operator demand surfaces, but adding it here would double the milestone's behavioral surface (parse + classify) instead of keeping it focused on the load-bearing "0 → ≥1 identified component at depth-1 walker scope" fix (per the 2026-07-02 F1 remediation).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Compliance auditor scanning a C/C++ source tree gets declared deps (Priority: P1)

A compliance auditor holding a `.tar.gz` (or a Git checkout) of a CMake-based C or C++ project runs mikebom to inventory its declared external dependencies. Today they get zero identified components + a pile of file-tier orphans. After this milestone, they get one component per distinct `find_package(<Name>)` and per `pkg_check_modules(<TARGET> <deps>)` module — with the declared version constraint captured — so downstream vulnerability scanning and license auditing have a workable input.

**Why this priority**: The direct user-facing gap. A C/C++ source-tree scan currently produces near-zero actionable output; after this milestone the same scan produces the roster of external libraries the project depends on. This is the biggest ROI per LOC any recent milestone has hit for source-tier C/C++ coverage.

**Independent Test**: Run mikebom against a checkout of the Kamailio project (or any comparable CMake-based C/C++ project with declared `find_package` deps at the CMakeLists.txt / depth-1 `cmake/*.cmake` layer) with the milestone-155 build. Assert: (a) the emitted `components[]` array contains at least one `pkg:generic/<name>` entry per distinct `find_package(<Name>)` call from the project's `CMakeLists.txt` + depth-1 `cmake/*.cmake` files; (b) version constraints from the `find_package(<Name> <Version>)` form are preserved as the PURL's `@<version>` segment; (c) each emitted component carries a `mikebom:source-mechanism = "cmake-find-package"` (or `"cmake-pkg-check-modules"`) annotation for downstream dedup transparency; (d) an mikebom-scanned Kamailio produces ≥1 identified component from the CMake reader — the empirical depth-1 count is 1 (`OpenSSL` in `cmake/defs.cmake`). The ≥1 floor is the milestone-155-scoped commitment; hitting the whole-tree count (10+) requires walker-depth extension to depth-2, which is a separate future milestone. Projects with all-depth-1 `find_package` layouts (e.g., typical vcpkg / conan / Ninja-first projects that keep `find_package` in the top-level CMakeLists.txt) will yield higher counts immediately.

**Acceptance Scenarios**:

1. **Given** a `CMakeLists.txt` containing `find_package(OpenSSL 1.1.0)`, **When** mikebom scans the source tree, **Then** the emitted SBOM MUST contain a component with `purl = "pkg:generic/openssl@1.1.0"` (lowercased name per PURL convention), `mikebom:source-mechanism = "cmake-find-package"`, and `evidence.source_file_paths` naming the CMake file where the declaration appeared.
2. **Given** a `CMakeLists.txt` containing `pkg_check_modules(RADIUS REQUIRED IMPORTED_TARGET radcli)`, **When** mikebom scans the source tree, **Then** the emitted SBOM MUST contain a component with `purl = "pkg:generic/radcli"` and `mikebom:source-mechanism = "cmake-pkg-check-modules"`.
3. **Given** a project with multiple `.cmake` files each declaring `find_package(OpenSSL <version>)` at different versions (e.g., `find_package(OpenSSL 1.1.0)` in `cmake/defs.cmake` and `find_package(OpenSSL 3.0)` in `cmake/modules/FindOpenSSL.cmake`), **When** mikebom scans it, **Then** the emitted SBOM MUST contain exactly ONE `pkg:generic/openssl` entry (deduped) with `@<highest-declared-version>` (i.e., `pkg:generic/openssl@3.0` for that example) per the Q1 clarification; every declaration site's file path is preserved in the merged component's `evidence.source_file_paths` list.
4. **Given** a `find_package(SomeName REQUIRED)` declaration WITHOUT any version constraint, **When** mikebom scans it, **Then** the emitted SBOM MUST contain a component with `purl = "pkg:generic/somename"` (no `@<version>`); the `mikebom:source-mechanism` annotation still applies.
5. **Given** a `find_package(SomeName QUIET)` or other non-load-bearing modifier keyword between the name and any version, **When** mikebom parses it, **Then** the emitted PURL MUST use the correct name (extracted from the first position) and correct version (if present in the second position); noise modifiers MUST NOT contaminate the name or version.
6. **Given** a project's CMake tree contains `find_package(<Name>)` AND the same project ALSO has vcpkg (`vcpkg.json`) or Conan (`conanfile.txt`) declaring the same dep, **When** mikebom scans it, **Then** the emitted SBOM MUST contain exactly ONE component for that dep — the milestone-105 dedup pipeline merges the two via `mikebom:also-detected-via` per its existing behavior. This closes the FR-007 double-count concern.

---

### User Story 2 — Same-PURL cross-mechanism dedup produces exactly one component (Priority: P2)

An operator scanning a C/C++ project whose CMake tree declares the SAME package via TWO different CMake mechanisms — for example, `find_package(openssl 1.1.0)` in one `.cmake` file AND `FetchContent_Declare(openssl URL https://.../openssl-1.1.0.tar.gz)` in another — expects the emitted SBOM to contain exactly ONE `pkg:generic/openssl@1.1.0` component, not two. The production `resolve::deduplicator` pass (grouping by `(ecosystem, name, version, parent_purl)`) handles the merge automatically since both mechanisms produce identical canonical PURLs.

**Why this priority**: Backward compatibility with mikebom's existing FetchContent extraction. Milestone 155 MUST NOT cause the same declared package to appear twice in an SBOM when the project uses BOTH `find_package(X)` AND `FetchContent_Declare(X ...)` (a valid CMake pattern for optional-vendored / fallback deps). Guarantees the milestone-155 emission path composes cleanly with the milestone-102/103 FetchContent path.

**Cross-tier scope note**: A stronger cross-tier story — the same package identified by CMake (source-tier) AND by an OS-package-DB reader like dpkg/rpm/apk (installed-tier) — is out of scope for milestone 155. The cross-namespace PURL correspondence (`pkg:generic/openssl` ↔ `pkg:deb/debian/libssl3`) requires either the milestone-111 `--pkg-alias-binding` infrastructure (operator-configured, not automatic) OR the milestone-105 `scan_fs::dedup` open-enum pipeline to be wired to production emission (that pipeline is currently `#[allow(dead_code)]` per its module doc at `mikebom-cli/src/scan_fs/dedup.rs:24-28`; wiring it is a milestone-105-follow-up, not a milestone-155 deliverable).

**Independent Test**: Construct a synthetic scan target with a `CMakeLists.txt` containing `find_package(openssl 1.1.0)` AND a `cmake/deps.cmake` containing `FetchContent_Declare(openssl URL https://example.com/openssl-1.1.0.tar.gz)`. Emit SBOM. Assert: exactly ONE component with PURL `pkg:generic/openssl@1.1.0` appears; the winner's `mikebom:source-mechanism` annotation is one of `{"cmake-find-package", "cmake-fetchcontent-url"}` (specification does NOT prescribe the winner because `resolve::deduplicator`'s confidence-based tie-break is opaque and may change across future milestones).

**Acceptance Scenarios**:

1. **Given** a scan target with a `CMakeLists.txt` containing `find_package(openssl 1.1.0)` AND a `cmake/deps.cmake` containing `FetchContent_Declare(openssl URL https://.../openssl-1.1.0.tar.gz)`, **When** mikebom scans, **Then** the emitted SBOM MUST contain exactly ONE `pkg:generic/openssl@1.1.0` component, NOT two.
2. **Given** the merged OpenSSL component, **When** the consumer inspects its `mikebom:source-mechanism` annotation, **Then** it MUST carry either `"cmake-find-package"` OR `"cmake-fetchcontent-url"` (the specific winner is `resolve::deduplicator` confidence-tie-break-dependent and NOT prescribed by this spec). A `mikebom:also-detected-via` annotation MAY be present if a future milestone-105-completion pipeline lands before milestone 155; its absence is NOT a failure in milestone 155 (per the module-doc note about the `scan_fs::dedup` pipeline being dead-code at production emission time).

---

### Edge Cases

- **`find_package(<Name>)` with a CMake `NO_MODULE` / `CONFIG` / `MODULE` mode flag**: the modifier is a CMake mechanism internal — it doesn't change the identity of the package. mikebom extracts the name unchanged.
- **`find_package(<Name> COMPONENTS a b c)`**: the `COMPONENTS` subclause lists sub-modules of a single package (e.g., `find_package(Boost COMPONENTS system filesystem thread)` requests 3 Boost components). mikebom emits ONE component for the parent package (`pkg:generic/boost@<version>`), NOT one per component. Component-list preservation via an annotation (`mikebom:cmake-components`) is deferred to a follow-up milestone if operator demand surfaces.
- **`find_package_handle_standard_args(<Name> ...)`**: this is CMake internal used inside `Find<Name>.cmake` scripts — it's NOT a package declaration. Milestone 155 MUST NOT extract these as components. (Distinguishable from `find_package` by the function name prefix.)
- **`find_package(PkgConfig)`**: a bootstrap call to enable pkg-config; not itself a project dependency. mikebom emits it as an entry (consistent behavior) but it's a low-signal component; consumers may filter it out via package name.
- **`pkg_check_modules(<TARGET> [REQUIRED] [IMPORTED_TARGET] [GLOBAL] <deps>)`**: `<TARGET>` is a CMake variable name (NOT a package). `<deps>` is one or more pkg-config module names, possibly with version constraints (e.g., `pkg_check_modules(GLIB REQUIRED glib-2.0>=2.42)`). Milestone 155 extracts each `<dep>` (stripping any embedded version-comparison operator + version), emits `pkg:generic/<dep>` per module.
- **`pkg_search_module` and `pkg_check_modules` name collision**: the two are sibling CMake macros for the same purpose. Milestone 155 handles both.
- **Case normalization**: CMake identifiers are case-insensitive; PURL names are typically lowercased. mikebom lowercases the extracted name for the PURL `name` segment; the original casing is preserved in the emitted `mikebom:cmake-find-package-name` annotation for traceability.
- **Commented-out declarations**: `# find_package(SomeUnusedDep)` inside a same-line comment MUST NOT emit a component. The extractor uses a line-anchored regex prefix `^[^#\n]*?` (research §R2.1) to reject lines where `#` precedes `find_package`. Multi-line block comments (`#[[ ... ]]`) are a documented limitation — they may be extracted incorrectly; see FR-011 known-limitation note.
- **String interpolation in the name**: `find_package(${SOME_VAR})` is rare but possible. The extractor MUST tolerate this without emitting an obviously-malformed PURL like `pkg:generic/${some_var}`. Skip and log at `tracing::debug` level (consistent with milestone 102/103 error handling).

## Requirements *(mandatory)*

### Functional Requirements

#### Core extraction (US1)

- **FR-001**: The CMake reader (`mikebom-cli/src/scan_fs/package_db/cmake.rs`) MUST parse `find_package(<Name> [<Version>] [REQUIRED|QUIET|EXACT|CONFIG|NO_MODULE|MODULE|COMPONENTS|OPTIONAL_COMPONENTS] ...)` declarations from every discovered CMake file (`CMakeLists.txt` + `cmake/**/*.cmake` files per the existing discovery scope in the reader).

- **FR-002**: For each `find_package(<Name> [<Version>] ...)` call, mikebom MUST emit a `PackageDbEntry` with PURL `pkg:generic/<lowercased-name>[@<version>]`. The version segment is present IFF the declaration includes a version position (second positional argument that parses as a valid version string — e.g., `1.1.0`, `2.4`, `3.0.14`); absent otherwise (per US1 A4). When the same lowercased name appears in multiple `find_package` sites at different declared versions, mikebom MUST select the **highest declared version** for the merged component's PURL (per the Q1 clarification). Version comparison is SemVer-style component-wise numeric where all parts parse as digits (e.g., `3.0` > `1.1.0`); ties or mixed-format versions where SemVer ordering is undefined fall back to lexicographic ordering with a `tracing::warn` diagnostic. When one declaration has a version and another has none, the versioned declaration wins (its version populates the merged PURL).

- **FR-003**: mikebom MUST parse `pkg_check_modules(<TARGET_VAR> [REQUIRED] [IMPORTED_TARGET] [GLOBAL] [QUIET] <module1> [<module2> ...])` declarations. For each `<module>` in the module list, mikebom MUST emit a `PackageDbEntry` with PURL `pkg:generic/<module-name>` (stripping any embedded version-comparison operator + version — e.g., `glib-2.0>=2.42` yields name `glib-2.0`, no version segment on the PURL).

- **FR-004**: mikebom MUST also parse `pkg_search_module(<TARGET_VAR> [REQUIRED] [IMPORTED_TARGET] <module1> [<module2> ...])` — the sibling macro of `pkg_check_modules` with the same semantic; same emission rules per FR-003.

- **FR-005**: Every emitted `PackageDbEntry` from this milestone MUST carry the annotation `mikebom:source-mechanism` with value `"cmake-find-package"` (for FR-001 / FR-002 emissions) or `"cmake-pkg-check-modules"` (for FR-003 / FR-004 emissions). The annotation flows through the production `resolve::deduplicator` pipeline via its milestone-109 `extra_annotations` folding logic (`mikebom-cli/src/resolve/deduplicator.rs:190-209`) — the winner's mechanism value survives, non-conflicting loser annotations get folded in. Cross-namespace `mikebom:also-detected-via` transparency (via milestone-105 `scan_fs::dedup`) is NOT emitted by production at milestone-155 landing; that's a milestone-105-completion follow-up.

- **FR-006**: Every emitted `PackageDbEntry` MUST carry the declaration-site path in its singular `PackageDbEntry.source_path: String` field (per the existing struct at `mikebom-cli/src/scan_fs/package_db/mod.rs` used since milestones 002/003, unchanged). Downstream, when the resolve pipeline creates the corresponding `ResolvedComponent`, the milestone-148 union pass populates the plural `ResolvedComponent.evidence.source_file_paths: Vec<String>` from every same-PURL entry's `source_path`. Milestone 155 does NOT touch the resolve pipeline; it relies on the existing union behavior. Result: multi-file same-name declarations surface every declaration site's path in the merged component's evidence — verified via US1 A3's acceptance scenario and R6 test #4.

- **FR-007 (REVERSAL)**: Milestone 155 EXPLICITLY REVERSES the milestone-102 FR-007 non-extraction rule. The updated rule: `find_package(<Name>)` declarations ARE parsed by mikebom (as of milestone 155). Cross-tier double-counting is prevented by the milestone-105 dedup pipeline via the `mikebom:source-mechanism` annotation added by FR-005. The milestone-102-era code comment at `cmake.rs:15-17` (or wherever the rule was documented) MUST be updated to reflect the new behavior.

#### Case handling + edge cases

- **FR-008**: Extracted CMake package names MUST be lowercased in the emitted PURL's `name` segment (per PURL convention). The original CMake-file casing MUST be preserved in a per-entry annotation `mikebom:cmake-find-package-name` for traceability.

- **FR-009**: mikebom MUST NOT extract `find_package_handle_standard_args(...)` as a package declaration — this is a CMake internal used inside `Find<Name>.cmake` scripts and does not indicate a project dependency. Distinguishable from `find_package` by the exact function name string.

- **FR-010**: mikebom MUST tolerate `find_package(${VARIABLE})` (name comes from CMake variable interpolation) by emitting NO component + logging at `tracing::debug` level. mikebom does not attempt CMake variable resolution.

- **FR-011**: mikebom MUST tolerate line-commented `find_package(...)` declarations — i.e., any occurrence of `#` on the SAME line before the `find_package` token. Line-commented declarations MUST NOT emit components. **Known limitation**: multi-line block-comment syntax (`#[[ ... find_package(...) ... ]]`) is NOT handled at milestone-155 time because the extraction regex operates line-by-line via a `^[^#\n]*?` prefix (see research §R2.1). Block-comment-enclosed declarations may be extracted incorrectly; operators can excise via `--exclude-path` if needed. A block-comment-aware extractor is a follow-up milestone if operator demand surfaces.

#### Byte-identity safeguards + reader scope

- **FR-012**: For scan targets containing zero CMake files, or CMake files with zero `find_package` / `pkg_check_modules` / `pkg_search_module` calls, mikebom's emitted output MUST be byte-identical to pre-milestone-155 output. Verified via SC-002 byte-identity against pre-milestone-155 golden fixtures (which don't contain these patterns today).

- **FR-013**: This milestone MUST NOT change the CMake reader's `FetchContent_Declare` / `ExternalProject_Add` / `add_subdirectory` extraction paths from milestones 102 / 103. These paths are strictly additive-not-modified.

- **FR-014**: This milestone MUST NOT change any OTHER reader (no dpkg / rpm / apk / vcpkg / Conan / language-ecosystem changes). The production `resolve::deduplicator` pipeline's downstream behavior is exercised (same-PURL merging + milestone-109 `extra_annotations` folding), not modified. The milestone-105 `scan_fs::dedup` open-enum pipeline (currently `#[allow(dead_code)]`) is neither exercised nor modified; wiring it in is a milestone-105-completion follow-up.

- **FR-015**: This milestone MUST NOT introduce a new `mikebom:*` annotation key beyond `mikebom:cmake-find-package-name` (per FR-008). The `mikebom:source-mechanism` key is unchanged — its open-enum value space is extended by two new values (`"cmake-find-package"` + `"cmake-pkg-check-modules"`). Constitution Principle V audit: `mikebom:cmake-find-package-name` is a parity-bridging annotation (no native construct in CDX / SPDX 2.3 / SPDX 3 for "the original casing of a name we normalized"); catalog row for it MAY be added in a follow-up documentation-refresh milestone (SC-007 accepts the catalog-row-later posture consistent with prior additive-annotation milestones like milestone 105).

- **FR-016**: This milestone MUST NOT change the milestone-133 file-tier walker or content-shape allowlist. The file-tier "orphan" behavior for the Kamailio scan (111 files matching the allowlist) is preserved — but the identified-component count goes from 0 to ≥1 for the Kamailio testbed at depth-1 walker scope (see F1 remediation on 2026-07-02), so the file-tier layer's relative signal-density DROPS marginally in this milestone. A walker-depth-extension follow-up milestone would raise the identified count further, moving more real deps into the identified tier and tightening the orphan set proportionally.

- **FR-017 (uniform emission — no build-tool denylist)**: mikebom MUST emit ALL `find_package(<Name>)` targets uniformly regardless of whether `<Name>` names a CMake meta-package or build-tool (e.g., `PkgConfig`, `Threads`, `Doxygen`, `Git`, `Python3`, `Perl`, `GNUInstallDirs`). Per the Q2 clarification, mikebom does NOT ship a curated build-tool denylist in this milestone — Constitution Principle X (transparency) favors showing what the CMake tree declares; downstream consumers may filter by name if they want a runtime-only roster. A build-tool classifier + `mikebom:component-role = "build-tool"` (milestone-127 role) tagging pass is a natural follow-up if operator demand surfaces.

### Key Entities

- **`find_package` declaration**: name (case-preserved in annotation, lowercased in PURL), optional version constraint, optional modifier keywords (REQUIRED / QUIET / etc. — non-load-bearing for extraction).
- **`pkg_check_modules` / `pkg_search_module` declaration**: target variable name (discarded — not a package), one or more module names each with optional embedded version comparison (stripped for extraction).
- **`mikebom:source-mechanism` annotation values** (new for this milestone): `"cmake-find-package"` and `"cmake-pkg-check-modules"`. Consumed by the milestone-105 dedup pipeline to produce `mikebom:also-detected-via` collision transparency.
- **`mikebom:cmake-find-package-name` annotation** (new for this milestone): preserves the original CMake-file-casing of an extracted name. Optional; only emitted when the name required lowercasing to produce the PURL (i.e., when input name was not all-lowercase).
- **The Kamailio SC-001 testbed**: the source tree at `/Users/mlieberman/Projects/kamailio` used as the concrete verification target. Not shipped in the mikebom repo — the maintainer clones or points at a local checkout for SC-001 verification.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (Kamailio testbed lower-bound)**: After milestone 155 ships, `mikebom sbom scan --path /path/to/kamailio --format cyclonedx-json` MUST produce **≥1** identified component from the CMake reader (up from 0 pre-155). The empirical depth-1 count is 1 (`OpenSSL 1.1.0` from `cmake/defs.cmake`); the remaining 9+ Kamailio `find_package` calls live at depth-2 inside `cmake/modules/Find*.cmake` and would require walker-scope extension to reach (a separate future milestone; see FR-013 + Assumption 1 + Assumption 6). The emitted OpenSSL component MUST carry `purl = "pkg:generic/openssl@1.1.0"`, `mikebom:source-mechanism = "cmake-find-package"`, `mikebom:cmake-find-package-name = "OpenSSL"`, and `evidence.source_file_paths` naming `cmake/defs.cmake`. Projects with all-depth-1 `find_package` layouts (typical vcpkg / conan / Ninja-first projects that keep declarations in top-level CMakeLists.txt or depth-1 `cmake/*.cmake`) will produce higher counts immediately; SC-004's synthetic testbed exercises a ≥5 component shape to verify the extraction code path scales cleanly.

- **SC-002 (byte-identical happy path)**: Scanning the milestone-090 sibling-fixture testbeds (`transitive_parity/cargo`, `transitive_parity/npm`, `transitive_parity/go`, `transitive_parity/pip_*`) — none of which contain CMake `find_package` calls in their test fixtures — with the milestone-155 build produces byte-identical CDX + SPDX 2.3 + SPDX 3 output compared to pre-milestone-155. Verified via existing golden tests.

- **SC-003 (cross-mechanism same-PURL dedup)**: A synthetic scan target containing BOTH a `find_package(openssl 1.1.0)` declaration AND a `FetchContent_Declare(openssl URL https://.../openssl-1.1.0.tar.gz)` declaration (in different CMake files, both discoverable at the depth-1 walker scope) produces exactly ONE `pkg:generic/openssl@1.1.0` component in the emitted SBOM. The `mikebom:source-mechanism` annotation on the surviving component is either `"cmake-find-package"` OR `"cmake-fetchcontent-url"` per `resolve::deduplicator`'s confidence tie-break (this spec does NOT prescribe the winner). Verified via a new integration test at `mikebom-cli/tests/cmake_find_package_dedup_integration.rs`. **NOTE**: A cross-namespace cross-tier dedup story (cmake-find-package + dpkg-status merging OpenSSL/libssl3) is out of scope per US2's cross-tier note — that requires either milestone-111 alias-binding infrastructure OR milestone-105-`scan_fs::dedup`-completion wiring, both follow-ups.

- **SC-004 (new integration test testbed synthesis)**: The milestone ships a small synthetic testbed at `mikebom-cli/tests/fixtures/cmake-find-package/` (or reuses an existing milestone-102/103 fixture augmented with `find_package` calls) that exercises: single `find_package(Name)`, single `find_package(Name 1.2.3)`, `pkg_check_modules(TARGET REQUIRED name1 name2>=3.0)`, commented-out `# find_package(...)`, `find_package(${VAR})` no-op, `find_package_handle_standard_args(...)` no-op. Each case has a corresponding acceptance-scenario unit test.

- **SC-005 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-155 main (clippy clean + every test passes except the documented `sbomqs_parity` env-only flake).

- **SC-006 (new unit-test coverage)**: At least 8 new unit tests covering: (a) simple `find_package(Name)`, (b) `find_package(Name Version)`, (c) case-normalization (uppercase name → lowercase PURL + `mikebom:cmake-find-package-name` annotation), (d) `pkg_check_modules` single module, (e) `pkg_check_modules` multi-module with embedded version comparison, (f) `find_package_handle_standard_args` NOT extracted, (g) `find_package(${VAR})` NOT extracted, (h) commented-out `find_package` NOT extracted.

- **SC-007 (no wire-format changes beyond intended)**: The shipped diff MUST NOT touch `docs/reference/sbom-format-mapping.md` in this milestone. The new `mikebom:source-mechanism` VALUES `cmake-find-package` / `cmake-pkg-check-modules` are additive to the existing C55 catalog row's open-enum value space; the new `mikebom:cmake-find-package-name` annotation is a natural catalog-addition candidate for a follow-up docs-refresh milestone, matching the prior additive-annotation pattern from milestone 105. The CycloneDX + SPDX 2.3 + SPDX 3 emitters MUST be unchanged.

- **SC-008 (CHANGELOG entry)**: The shipped diff MUST include an entry in `CHANGELOG.md` under `[Unreleased]` naming: (a) the reversal of milestone-102 FR-007; (b) the new `find_package` / `pkg_check_modules` / `pkg_search_module` extraction; (c) the two new `mikebom:source-mechanism` values `cmake-find-package` + `cmake-pkg-check-modules`; (d) the new `mikebom:cmake-find-package-name` annotation key; (e) the Kamailio testbed impact (0 → ≥1 component at depth-1 walker scope per the 2026-07-02 F1 remediation; walker-depth extension is a separate follow-up milestone); (f) the production `resolve::deduplicator` handles same-PURL cross-mechanism merges automatically (`cmake-find-package` + `cmake-fetchcontent-url` both emitting `pkg:generic/openssl@1.1.0` collapse to one component); cross-namespace dedup is out of scope; (g) the Q1 clarification (highest declared version wins); (h) the Q2 clarification (no build-tool denylist; consumers filter by name).

## Assumptions

1. **The milestone-102/103 CMake reader's file discovery scope is sufficient**: `CMakeLists.txt` + `cmake/**/*.cmake` walked at depth-1 per the existing helper (`discover_cmake_files`). Milestone 155 does NOT extend the discovery scope; it only adds new pattern extraction to the files already discovered. If a project has `find_package` declarations in unusual locations (e.g., inside `src/subproject/CMakeLists.txt` at deeper depth), coverage may be incomplete; extending the discovery scope is a separate future milestone if operator demand surfaces.

2. **The production `resolve::deduplicator` handles same-PURL cross-mechanism merges**: the production dedup path groups `ResolvedComponent` instances by `(ecosystem, name, version, parent_purl)` and merges `extra_annotations` per the milestone-109 pattern at `mikebom-cli/src/resolve/deduplicator.rs:190-209`. Both new `mikebom:source-mechanism` values (`cmake-find-package`, `cmake-pkg-check-modules`) are STRING annotation values that flow through this dedup path unchanged — no pipeline code change required. The milestone-105 `scan_fs::dedup` closed enum (at `mikebom-cli/src/scan_fs/dedup.rs:58`) will need extension with `CmakeFindPackage` + `CmakePkgCheckModules` variants IF a future milestone-105-completion wires that pipeline in; that expansion is out of milestone-155 scope.

3. **`pkg:generic/<name>` is the correct PURL type for `find_package`-extracted deps**: no ecosystem-specific PURL type exists for "arbitrary CMake `find_package` target" — the `generic` type is the honest catch-all. Downstream consumers may re-classify via `deps.dev` or manual mapping (this is outside milestone 155's scope).

4. **Version constraints are captured verbatim, not resolved**: `find_package(OpenSSL 1.1.0)` yields `pkg:generic/openssl@1.1.0` even though CMake would resolve this to whatever installed OpenSSL satisfies `>= 1.1.0` at build time. The declared version constraint is a useful lower-bound signal for compliance / vulnerability workflows even without resolution.

5. **No `Find<Name>.cmake` script content is parsed**: this milestone only parses `find_package(...)` CALL sites at the top-level of a CMakeLists.txt or included .cmake file. The `Find<Name>.cmake` scripts themselves are CMake-internal helpers that DEFINE how to find a package — they don't declare that the project depends on it.

6. **The Kamailio SC-001 lower-bound of ≥1 is a walker-scope-honest floor**: mikebom's `discover_cmake_files` helper walks depth-1 in `cmake/`, `Modules/`, `third_party/` (per the existing milestone-102/103 scope). Kamailio's `find_package` declarations are unusually distributed — most live in `cmake/modules/Find*.cmake` at depth-2 (NOT walked) rather than the more typical location of the top-level CMakeLists.txt or depth-1 `.cmake` includes. The empirical depth-1 count (verified 2026-07-02 via `grep -hE '^[^#]*\bfind_package\s*\(' CMakeLists.txt cmake/*.cmake` on a fresh Kamailio checkout) is 1 (`OpenSSL 1.1.0`). Milestone 155's floor of ≥1 reflects this walker-scope reality without over-promising. A follow-up milestone extending the walker to depth-2 (or resolving `include(...)` directives) would raise the empirically-observable count for Kamailio; that scope expansion is deliberately out of milestone-155 scope per FR-013 (existing FetchContent / ExternalProject / add_subdirectory / discovery paths unchanged) + Assumption 1 (discovery scope inherited from milestones 102/103). Milestone 155 delivers the parsing code path; scope expansion delivers Kamailio-specific reach.

7. **Case normalization to lowercase for the PURL matches the PURL spec**: the PURL spec (per `packageurl.io`) treats `pkg:generic/OpenSSL@1.1.0` and `pkg:generic/openssl@1.1.0` as different components; downstream consumers may or may not case-normalize. mikebom lowercases at emission time to produce consistent output across scans of the same project with different casings in the CMake files.

8. **Constitution Principle V (standards-native fields first) audit**: `mikebom:source-mechanism` is an existing parity-bridging annotation (C55 catalog row); this milestone extends its open-enum value space. `mikebom:cmake-find-package-name` is a NEW parity-bridging annotation — no native construct in any of the three formats carries "original-casing of an extracted name we normalized." The audit is deferred to the follow-up docs-refresh milestone; per prior additive-annotation milestone precedent (milestone 105) this is an acceptable ship posture as long as the annotation's shape is documented in the milestone's spec/plan.

## Dependencies

- **Milestone 102** (CMake reader — the file being modified): `mikebom-cli/src/scan_fs/package_db/cmake.rs`. Milestone 155 REVERSES its FR-007 non-extraction rule for `find_package` and adds the `pkg_check_modules` / `pkg_search_module` handling.
- **Milestone 105** (dedup pipeline + `SourceMechanism` enum + `mikebom:also-detected-via`): critical for cross-mechanism collision transparency. Milestone 155 depends on this pipeline treating the new `SourceMechanism` values uniformly.
- **Milestone 133** (file-tier walker): unaffected. The Kamailio scan's file-tier "orphan" behavior for shell scripts + sample git hooks is unchanged; the added `find_package` extraction operates at a different tier (source-tier package DB reader).

## Out of Scope

- No `Find<Name>.cmake` script parsing (per Assumption 5).
- No `find_package_handle_standard_args` extraction (per FR-009).
- No CMake variable resolution / `${VAR}` interpolation (per FR-010).
- No `find_package(<Name> COMPONENTS a b c)` sub-component extraction (per Edge Cases; single-component-per-package emission for now).
- No autotools (`configure.ac` / `AC_CHECK_LIB` / `PKG_CHECK_MODULES` in shell script form) — separate milestone if operator demand surfaces.
- No pkg-config `.pc` file parsing (those live in the installed system, not the source tree).
- No CMake preset (`CMakePresets.json`) parsing — that's a build-system configuration, not a dependency declaration.
- No `add_subdirectory` extraction changes (milestone-102's `include_vendored` opt-in path is preserved unchanged).
- No `FetchContent_Declare` / `ExternalProject_Add` extraction changes (milestone-102/103 paths preserved unchanged).
- No compile_commands.json parsing (a natural follow-up milestone if operators start emitting these and want richer resolved-version data; explicitly out of scope here).
- No new catalog row for `mikebom:cmake-find-package-name` in this milestone (see FR-015 + SC-007; documented in a follow-up docs-refresh milestone).
