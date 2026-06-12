# Feature Specification: User-Supplied Directory Exclusion for `mikebom scan`

**Feature Branch**: `113-exclude-path-flag`
**Created**: 2026-06-12
**Status**: Draft
**Input**: User description: "Issue #334 — add `--exclude-path` flag to `mikebom scan` so users can opt-in to excluding directories that contain fixture/sample projects (e.g. `tests/fixtures`, `examples/sample-projects`). Repeatable, glob-compatible, applied at every ecosystem walker's descent point, off by default, no behavioral change without the flag. Solves the same shape of inverted-dependency-edge bug that the Go `testdata/` fix solved for Go, but for ecosystems where there's no documented language convention (cargo, maven, gem, pip, npm, gradle, nuget, yocto)."

## Clarifications

### Session 2026-06-12

- Q: How does the scanner decide whether an exclusion entry is a literal path or a pattern? → A: Implicit by metacharacters — a single argument; presence of `*`, `?`, or `[` classifies the entry as a pattern, otherwise it is a literal anchored at scan root.
- Q: Can an exclusion entry target a single file, or directories only? → A: Directories only — entries match directory names/paths; an operator who wants to suppress one manifest names its containing directory instead. Matches every built-in skip in the scanner today.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Exclude a single fixture directory across every ecosystem (Priority: P1)

A developer scans a polyglot monorepo whose `tests/fixtures/` subtree contains throwaway sample projects in several ecosystems (a Cargo crate, a Maven submodule, a pip package, an npm package) used purely to drive integration tests. By default, mikebom walks into those subdirectories, discovers their manifests, and emits each fixture as a real component — sometimes also recording synthetic dependency edges declared in the fixture manifests. The developer adds one CLI argument naming that subtree and the fixture components vanish from the resulting SBOM regardless of which ecosystem the fixture belongs to.

**Why this priority**: This is the MVP. It directly addresses the bug class motivating the feature (inverted/spurious edges from fixture manifests) and matches the most common operator pattern (a single `tests/fixtures` or `examples/` dir). Without it, operators have no way to opt out of false-positive fixture components in any non-Go ecosystem. Every higher-priority case dissolves once a single literal exclusion path works across all walkers.

**Independent Test**: Run a scan on a fixture repository with a `tests/fixtures/<ecosystem>/<manifest>` subtree for at least three different ecosystems, with and without the exclusion argument. With the argument set to `tests/fixtures`, none of the fixture components appear in the emitted SBOM; without it, they all do. Delivers the entire value of the feature when used in isolation.

**Acceptance Scenarios**:

1. **Given** a repository with a real Cargo workspace at the root and a throwaway crate at `tests/fixtures/sample-crate/Cargo.toml`, **When** the user scans the repository and supplies an exclusion entry for `tests/fixtures`, **Then** the emitted SBOM contains the real workspace components but does not contain a component derived from the fixture crate.
2. **Given** a repository with a real npm package at the root and a fixture package at `tests/fixtures/sample-app/package.json`, **When** the user scans with the same exclusion entry, **Then** the fixture package is absent from the SBOM and no dependency edge references it.
3. **Given** the same repository, **When** the user scans without supplying any exclusion entry, **Then** the emitted SBOM is byte-identical to the SBOM that a pre-feature build of the scanner would have produced (no behavioral change without the flag).
4. **Given** a repository where a fixture manifest declares a synthetic dependency on the parent project, **When** the user scans with the fixture path excluded, **Then** no dependency edge from any fixture to the parent appears in the emitted SBOM.

---

### User Story 2 — Pattern-match multiple fixture directories scattered across a monorepo (Priority: P2)

A platform team owns a large monorepo where every service in `services/*/` carries its own `testdata/` subdirectory containing fixture manifests for that service's integration tests. There are dozens of such directories; enumerating each one literally is verbose and error-prone. The team supplies one pattern that matches every `testdata/` directory regardless of depth, and the SBOM cleanly contains only the real service components.

**Why this priority**: The literal-path MVP (US1) covers the common single-directory case but is impractical for monorepos with N fixture trees. Pattern support is a clear next layer once the wiring exists, and lets a single argument express the intent "any directory named X anywhere in the tree." It is a usability layer on top of US1, not a separate capability.

**Independent Test**: Run a scan on a synthetic monorepo with multiple `services/<name>/testdata/<manifest>` subtrees, supplying a single pattern argument that matches the `testdata` name at any depth. The emitted SBOM contains every real service component and none of the fixture components.

**Acceptance Scenarios**:

1. **Given** a repository with `services/a/testdata/x/Cargo.toml`, `services/b/testdata/y/Cargo.toml`, and `services/c/testdata/z/Cargo.toml`, **When** the user supplies a single pattern matching every `testdata` directory regardless of depth, **Then** no fixture component appears in the SBOM and all three real services do.
2. **Given** the same repository, **When** the user supplies two distinct pattern arguments (e.g. one for `testdata` and one for `_archive`), **Then** both patterns are honored simultaneously.

---

### User Story 3 — Discover the exclusion mechanism without reading source (Priority: P3)

An operator new to mikebom hits the fixture-as-component problem on their first scan. They run `mikebom scan --help` and see a flag in the help output whose description points them to the user-guide chapter explaining the exclusion mechanism. They read that section, supply the appropriate argument, and re-scan without needing to file a bug report or read source code.

**Why this priority**: The feature only delivers value when operators can find it without external help. The exclusion behavior is straightforward once known, but undiscoverable behavior is effectively absent for many users.

**Independent Test**: A user who has never read this spec invokes `mikebom scan --help`. The output references the exclusion mechanism and points to its documentation. The referenced documentation, in turn, gives at least one worked example for a non-Go ecosystem and clarifies the relationship to the built-in skip set (`vendor/`, `node_modules/`, etc.) and to the Go-specific `testdata` / `_`-prefix skip that ships unconditionally.

**Acceptance Scenarios**:

1. **Given** an operator running `mikebom scan --help`, **When** they read the output, **Then** the exclusion mechanism is listed with a one-line description and a pointer to the user-guide section.
2. **Given** the user-guide CLI reference, **When** an operator searches for the exclusion behavior, **Then** the section explains usage with at least one fully-worked non-Go example and notes that built-in skips and the Go convention skips still apply on top of any user-supplied exclusions.

---

### Edge Cases

- What happens when the user supplies a path that does not exist in the repository? The scan proceeds normally; no warning is required for a benign no-match.
- What happens when the user supplies a path that excludes the scan root itself? The emitted SBOM contains no ecosystem components — equivalent to a scan of an empty directory. The metadata component (the scan target itself) is unaffected.
- What happens when a user-supplied exclusion path matches a directory that the scanner would have skipped anyway (`vendor/`, `node_modules/`, `target/`, etc.)? No harm; the exclusion is additive on top of the built-in skip set.
- What happens when the operator supplies a malformed pattern? The scan refuses to start with a clear error citing the bad pattern; partial application is never attempted.
- What happens when a single component is discovered through two parallel walkers (e.g. a project root that holds both `Cargo.toml` and `package.json`) and the user excludes its directory? Both walkers honor the exclusion; the component disappears entirely.
- What happens when the user supplies a path with platform-specific separators (Windows backslashes on a Linux scan, or vice versa)? The exclusion still matches the intended directory; users do not need to know which platform the scanner is running on.
- What happens when a symlink inside the repository points into an excluded directory? The symlink target is still considered excluded; users do not have to enumerate every symlinked alias to a fixture tree.
- What happens when the user supplies the same path twice (or a literal path plus a pattern that also matches it)? Idempotent; the exclusion behaves as if supplied once.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan command MUST accept one or more user-supplied directory-exclusion entries, each of which suppresses every component the scanner would otherwise have derived from manifests beneath the matching subtree.
- **FR-002**: Each exclusion entry MUST be honored across every ecosystem walker that contributes components to the emitted SBOM (today: cargo, maven, gem, pip, npm, gradle, nuget, yocto, Go source, Go binary, and any future walker that discovers project roots by descent).
- **FR-003**: When the operator supplies zero exclusion entries, the emitted SBOM MUST be byte-identical to the SBOM produced by a pre-feature build of the scanner against the same repository and otherwise identical inputs.
- **FR-004**: The exclusion mechanism MUST be additive on top of the scanner's built-in skip set (`vendor/`, `node_modules/`, `target/`, `dist/`, `build/`, `__pycache__/`, the Go-specific `testdata` / `_`-prefix / `go/pkg/mod` skips, etc.). User-supplied entries cannot enable scanning of a directory the scanner skips by default.
- **FR-005**: Multiple exclusion entries supplied in a single scan MUST be honored simultaneously; a directory matched by any one of them is excluded.
- **FR-006**: A single CLI argument MUST accept both literal directory paths and patterns; the scanner classifies each entry by inspecting its text for glob metacharacters (`*`, `?`, `[`). An entry containing any of those characters is treated as a pattern matching directory names at arbitrary depth in the tree; an entry containing none is treated as a literal path interpreted relative to the scan root. Operators do not pass a separate flag to switch modes.
- **FR-007**: The scanner MUST reject malformed exclusion patterns with a clear, actionable error message naming the bad entry, and MUST NOT begin scanning when any supplied entry is malformed.
- **FR-008**: An exclusion entry that does not match any directory in the scanned tree MUST be treated as a no-op (no warning, no error) so operators can maintain a stable, reusable exclusion list across heterogeneous repositories.
- **FR-009**: Exclusion entries MUST work identically on Linux, macOS, and Windows scans; operators MUST NOT need to vary their entries by host platform.
- **FR-010**: Component suppression from an exclusion MUST also suppress every dependency edge whose endpoint is the suppressed component, so no dangling references survive in the emitted SBOM.
- **FR-011**: The exclusion behavior MUST be documented in the user-guide CLI reference with at least one fully-worked non-Go example, and the scanner's help text MUST point operators to that documentation.
- **FR-012**: The exclusion mechanism MUST be discoverable from `mikebom scan --help` output (e.g. listed in the global-flags section) without the operator having to read source code or release notes.
- **FR-013**: Excluding a directory MUST suppress not only components derived from manifests in that subtree but also any binary-tier components derived from binaries in that subtree, so a fixture binary in `tests/fixtures/foo/bin/foo` does not appear when `tests/fixtures` is excluded.
- **FR-014**: When at least one exclusion entry is in effect for a scan, the emitted SBOM MUST carry an envelope-level transparency annotation enumerating every active entry (literal entries normalized to forward-slash form, pattern entries verbatim, source-order preserved) in each emitted format. Standards-native audit per Constitution Principle V bullet 5: CycloneDX 1.6 has no native field carrying "operator excluded paths at scan time" semantics (`metadata.lifecycles` expresses build phase only); SPDX 2.3 has no equivalent native construct (only the free-form `creationInfo.creatorComment`, which lacks the required structure); SPDX 3.0.1 has no equivalent — the structured `Annotation` element is the format's general extension surface. The annotation therefore qualifies under Principle V bullet 5's parity-bridging carve-out for "finer-grained information the standard does not express" and MUST be emitted with the `mikebom:` prefix. The annotation is required for compliance with Constitution Principle X (Transparency), which mandates structured metadata whenever completeness is intentionally narrowed.

### Key Entities

- **Exclusion entry**: A user-supplied directive identifying one or more directories whose contents the scanner must ignore when discovering project roots and binary components. Each entry is classified by the scanner as either a literal path (relative to the scan root) or a pattern (matching directory names at arbitrary depth) based on whether its text contains glob metacharacters. Entries combine by union; a directory matched by any entry is excluded.
- **Exclusion-aware walker**: A scanner component that descends into a directory tree to discover ecosystem-specific project markers (manifests, lock files, binaries). Every exclusion-aware walker consults the active exclusion set at each descent decision and skips matched subtrees before any per-walker emission occurs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For every ecosystem walker listed in FR-002, a scan with a single exclusion entry targeting a fixture subtree of that ecosystem produces zero components derived from manifests beneath the excluded subtree, while preserving every component derived elsewhere in the repository.
- **SC-002**: A scan run without any exclusion entry produces an SBOM that is byte-for-byte identical to one produced by the pre-feature scanner against the same repository, across every emitted format (CDX, SPDX 2.3, SPDX 3).
- **SC-003**: A scan run with one or more well-formed exclusion entries against a representative polyglot fixture repository completes in time no greater than 110% of the same scan with no exclusion entries — i.e. exclusion processing adds no more than 10% overhead in the worst case.
- **SC-004**: An operator who reads only the user-guide CLI reference for this feature can write a working exclusion entry for a non-Go fixture directory on the first attempt, without needing to inspect source code or open a support request.
- **SC-005**: A scan with a malformed exclusion entry exits non-zero before any walker begins, with a single error line that names the bad entry verbatim.
- **SC-006**: After this feature ships, the reporter's bug class (inverted or spurious dependency edges caused by a fixture manifest's synthetic requires) can be resolved for any non-Go ecosystem by adding exclusion entries to the operator's scan command — no further scanner changes required.
- **SC-007**: A scan with at least one exclusion entry MUST emit the `mikebom:exclude-path` transparency annotation in all three formats (CycloneDX 1.6, SPDX 2.3, SPDX 3.0.1); a scan with zero exclusion entries MUST NOT emit the annotation in any format.

## Assumptions

- The feature's scope is operator-supplied scan-time directory exclusion. Auto-detecting fixture/sample directories by heuristic (e.g. parent-directory naming, presence of test markers) is explicitly out of scope: that path was already considered for cargo/maven/gem/pip/npm/gradle/nuget/yocto and rejected in issue #334 because no documented language convention exists for those ecosystems, leaving the choice inherently per-repo.
- The Go-specific unconditional skip of `testdata/` and `_`-prefixed directories (shipped in the sibling fix for the Go walkers) remains in place and is not affected by this feature. Operators do not need to add `testdata` to their exclusion list for Go projects.
- Existing built-in skips (`vendor/`, `node_modules/`, `target/`, `dist/`, `build/`, `__pycache__/`, `go/pkg/mod`, `.`-prefixed directories) remain in place and are not affected by this feature. Operators do not need to re-state these to keep current behavior.
- Exclusion entries are scoped to a single `mikebom scan` invocation. Persistent project-level configuration (e.g. a `.mikebomignore` file) is out of scope for this feature; if needed later it can wrap the same underlying mechanism.
- Path interpretation is relative to the scan root (the directory or container image root being scanned). Absolute paths are not required because exclusion entries describe locations within the scanned tree, not on the host filesystem.
- The exclusion mechanism applies to walkers that emit components into the SBOM. Walkers used purely for evidence augmentation (e.g. a deep-hash file walker that decorates an already-emitted component with content hashes) are not in scope; nothing they do creates a new component that could be a false positive.
- "Byte-identical" in FR-003 / SC-002 is measured against deterministic-emission inputs (fixed timestamp via `MIKEBOM_FIXED_TIMESTAMP`, masked serial number) — i.e. the comparison strips run-to-run nondeterminism that already exists today and is not introduced or aggravated by this feature.
- Negation patterns ("exclude X but re-include Y inside X") are out of scope for v1. Operators who need that level of control can structure their exclusion entries to name only the dirs they want excluded.
