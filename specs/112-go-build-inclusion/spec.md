# Feature Specification: Go Build-Inclusion Clarity

**Feature Branch**: `112-go-build-inclusion`
**Created**: 2026-06-11
**Status**: Draft
**Input**: User description: "Go build-inclusion clarity: consumer-visible build-inclusion-unknown markers for go.sum-fallback/flat-attached Go components, plus opt-in package-level build-graph reachability classification via Go toolchain shell-out (go mod why) with graceful degrade"

## Context

A source-tier Go scan today can emit components whose participation in a
production build is unknown, and the SBOM gives consumers no way to tell
them apart from real runtime dependencies. Empirical anchor
(kusaridev/kusari-cli, post-PR-#332): mikebom emits **87** golang
components; **61** are linker-confirmed when a built binary is present;
cyclonedx-gomod's build list is **64**; **20** source-only components sit
outside that build list with no consumer-visible scope — 16 carry only
internal provenance markers (`mikebom:resolver-step: go-sum-fallback`,
`mikebom:orphan-reason: flat-attached-fallback`) and 4 carry nothing at
all (check.v1, go-internal, kr/pretty, kr/text). A consumer running a
vulnerability triage against this SBOM treats all 20 as production
dependencies.

Two complementary capabilities close the gap:

- **Part B (always available)**: components whose only discovery evidence
  is the go.sum flat fallback or the orphan flat-attach backfill, and
  which no higher-fidelity signal confirms, carry an explicit
  consumer-visible "build inclusion unknown" signal in every emitted
  SBOM format.
- **Part C (when a Go toolchain is available)**: package-level
  build-graph reachability classification, asking the Go toolchain
  itself whether the main module needs each module — the same evidence
  source cyclonedx-gomod uses. Modules the main module does not need are
  marked excluded; modules reachable only through test packages get test
  lifecycle scope; production-reachable modules are untouched.

## Clarifications

### Session 2026-06-11

- Q: How should not-needed modules appear in the default SBOM output? → A: Keep them in the SBOM with scope `excluded` + derivation annotation (never dropped, regardless of `--include-dev`); test-tagged modules keep the existing drop-unless-`--include-dev` behavior.
- Q: What total time budget does package-level analysis get per scan before abandoning and falling back to Part B markers? → A: 60 seconds total across all toolchain queries.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Consumer-visible "build inclusion unknown" signal (Priority: P1)

A security engineer ingests a mikebom source-tier SBOM for a Go project
into their vulnerability-management tooling. For every Go component they
can determine from the SBOM alone whether mikebom (a) confirmed it
participates in a production build, (b) confirmed it does not, or
(c) cannot tell. Components in category (c) — discovered only via the
go.sum flat fallback or the orphan backfill, with no confirming signal —
carry an explicit machine-readable marker, so the engineer can deprioritize
or separately triage them instead of treating them as confirmed runtime
dependencies.

**Why this priority**: This is the always-on half that works on every
host (no toolchain, offline, image scans of source trees). Without it,
the 20-component ambiguity in the anchor repo is invisible to consumers.
It delivers value standalone even if Part C never ships.

**Independent Test**: Scan a Go source tree offline (no module cache, no
toolchain assistance) so the go.sum fallback fires; assert every
fallback-discovered, unconfirmed component carries the
build-inclusion-unknown marker in CycloneDX, SPDX 2.3, and SPDX 3
outputs, and every confirmed component does not.

**Acceptance Scenarios**:

1. **Given** a Go source scan where some modules are claimed only by the
   go.sum flat fallback, **When** the SBOM is emitted, **Then** each such
   component carries a consumer-visible build-inclusion-unknown marker in
   all three output formats.
2. **Given** a component flat-attached to the main module by the orphan
   backfill, **When** the SBOM is emitted, **Then** it carries the same
   marker.
3. **Given** a module confirmed by a higher-fidelity signal (built-binary
   BuildInfo match, module-graph production reachability, or Part C
   package-level analysis), **When** the SBOM is emitted, **Then** it
   does NOT carry the unknown marker.
4. **Given** a scan of the kusari-cli anchor repo without a Go toolchain,
   **When** the SBOM is emitted, **Then** all 4 previously-bare
   components (check.v1, go-internal, kr/pretty, kr/text) and the 16
   provenance-marked components carry the unknown marker, and the
   component count is unchanged (87).

---

### User Story 2 - Package-level build-graph classification (Priority: P2)

A developer scans their Go repository on a workstation or CI host where
the Go toolchain is installed. mikebom asks the toolchain, per module,
whether the main module's production build needs it. The emitted SBOM
then matches the precision of dedicated Go SBOM tooling: modules the
build does not need are marked excluded with an explanatory annotation;
modules reachable only through test packages get test lifecycle scope;
production modules are unchanged. The unknown marker from User Story 1
disappears for every module the analysis classified.

**Why this priority**: This converts "unknown" into a definitive answer
and reaches parity with cyclonedx-gomod's filtering, but it depends on a
toolchain being present and on User Story 1's marker machinery for the
fallback path.

**Independent Test**: Scan the kusari-cli anchor repo on a host with the
Go toolchain; assert the 20 outside-build-list components are classified
(excluded or test), none of the 64 build-list modules is excluded, and
zero components carry the unknown marker.

**Acceptance Scenarios**:

1. **Given** a Go source scan on a host with a working Go toolchain,
   **When** package-level analysis runs, **Then** modules the main module
   does not need are scope-excluded and annotated with the derivation.
2. **Given** a module whose only import chain passes through a test
   package, **When** package-level analysis runs, **Then** it receives
   test lifecycle scope with a derivation annotation consistent with the
   existing test-scope annotations.
3. **Given** a module the production build needs, **When** package-level
   analysis runs, **Then** its scope and annotations are unchanged from
   pre-feature output.
4. **Given** the kusari-cli anchor repo, **When** scanned with the
   toolchain available, **Then** every module in cyclonedx-gomod's
   64-module build list remains non-excluded (superset invariant: no
   false exclusions).

---

### User Story 3 - Graceful degrade and pre-feature compatibility (Priority: P3)

An operator runs mikebom in a minimal CI container without a Go
toolchain, or offline, or against a Go tree that does not build. The
scan completes exactly as it does today — package-level analysis is
skipped with a warning log, the User Story 1 markers still apply, and
nothing else in the output changes. Scans of non-Go projects and Go
scans where no fallback-discovered components exist are byte-identical
to pre-feature output.

**Why this priority**: Protects the existing operational envelope (image
scans, offline scans, hermetic CI) and the repo's byte-identity golden
discipline. It is the safety net for the other two stories.

**Independent Test**: Run the full existing golden suite plus a
no-toolchain Go scan; assert zero drift outside the documented Part B
marker additions, and assert a scan with a deliberately broken toolchain
(e.g., stub `go` that exits non-zero) completes successfully with a
warning.

**Acceptance Scenarios**:

1. **Given** no Go toolchain on PATH, **When** a Go source scan runs,
   **Then** the scan succeeds, package-level analysis is skipped with a
   warn-level log, and output equals pre-feature output plus Part B
   markers.
2. **Given** the toolchain subprocess fails or times out mid-analysis,
   **When** the scan runs, **Then** the scan still succeeds and falls
   back to Part B-only behavior for unclassified modules.
3. **Given** `--offline` mode, **When** a Go source scan runs, **Then**
   no toolchain invocation performs network access (or the analysis is
   skipped entirely if it cannot run without network).
4. **Given** a non-Go project scan, **When** the SBOM is emitted,
   **Then** output is byte-identical to pre-feature output.

---

### Edge Cases

- Module confirmed by BuildInfo (binary present) but reported not-needed
  by package-level analysis (e.g., build-tag/platform differences):
  linker evidence wins — BuildInfo remains authoritative, no exclusion.
- Platform-conditional modules (e.g., Windows-only `mousetrap`,
  `coninput`): the toolchain answers for the scan host's GOOS/GOARCH;
  classification reflects the host platform and the derivation
  annotation makes the evidence source auditable.
- Vendored repositories (`vendor/` present): `vendor/` does not carry
  the module graph, so package-level analysis still requires a usable
  module cache regardless of vendor mode (verified empirically,
  go 1.26). Analysis runs in module mode; when the cache cannot support
  resolution (e.g., offline with a cold cache), the reliability
  preflight fails and analysis degrades to unknown markers — vendor/
  presence never produces false verdicts.
- Silent toolchain false-negatives: when module resolution fails
  mid-query (cold module cache offline, unreachable proxy), `go mod
  why` can exit 0 while wrongly reporting modules as not needed —
  including directly-imported ones (verified empirically, go 1.26). A
  package-load reliability preflight (`go list all`) MUST gate
  classification per main module: preflight failure → analysis skipped
  per FR-007; not-needed verdicts are never accepted without a passing
  preflight.
- Multi-module workspaces / multiple main modules in one tree:
  classification applies per main module; a module needed by ANY main
  module in the scan is not excluded.
- Toolchain present but go.mod requires a newer Go version than
  installed: treated as subprocess failure → graceful degrade.
- A module that is both fallback-discovered AND classified by Part C:
  the classification replaces the unknown marker (no contradictory
  signals on one component).
- Existing test-scope tagging (direct `_test.go` imports, PR #332
  closure): Part C must not downgrade an existing test tag to required;
  agreement is expected, conflicts resolve toward the package-level
  answer with the derivation recorded.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Emitted SBOMs MUST carry a consumer-visible, machine-readable
  build-inclusion-unknown marker on every Go component whose only
  discovery evidence is the go.sum flat fallback or the orphan
  flat-attach backfill and which no higher-fidelity signal (built-binary
  BuildInfo match, module-graph production reachability, package-level
  analysis) confirms. The marker MUST appear in CycloneDX, SPDX 2.3, and
  SPDX 3 outputs with annotation-parity coverage.
- **FR-002**: The unknown marker MUST NOT change the component's scope:
  unknown components remain unscoped (consumers default to treating them
  as required — the conservative posture for security triage). The
  marker is additive metadata, not an exclusion.
- **FR-003**: When a working Go toolchain is available on the scan host,
  the system MUST perform package-level build-graph reachability
  classification for the scanned main module's resolved module set by
  querying the Go toolchain (module-need analysis equivalent to
  `go mod why`), enabled by default with a flag to disable it.
- **FR-004**: Modules the main module's production build does not need
  MUST remain in the emitted SBOM, marked scope-excluded and annotated
  with a derivation discriminator identifying package-level analysis as
  the evidence source (Constitution Principle X). They are NEVER dropped
  from output, independent of `--include-dev`.
- **FR-005**: Modules whose shortest import chain passes through a test
  package MUST receive test lifecycle scope, emitted consistently with
  the existing test-scope representation (CycloneDX `excluded` +
  `mikebom:lifecycle-scope: test`; SPDX 2.3 `TEST_DEPENDENCY_OF`), with
  a derivation discriminator distinguishing package-level analysis from
  the existing manifest-derived tags.
- **FR-006**: Modules the production build needs MUST be emitted with
  scope and annotations unchanged from pre-feature output, and MUST NOT
  carry the unknown marker.
- **FR-007**: Absence of a toolchain, offline operation, a non-buildable
  tree, subprocess failure, or exceeding the analysis time budget —
  **60 seconds total across all toolchain queries per scan** — MUST
  degrade gracefully: the scan completes successfully, a warn-level log
  records the skip reason, and affected modules fall back to FR-001
  marking. Package-level analysis failures MUST never fail the scan.
- **FR-008**: Scans on hosts without a Go toolchain MUST produce output
  byte-identical to pre-feature output except for the FR-001 markers.
  Non-Go scans MUST remain fully byte-identical (zero golden drift
  outside Go fixtures).
- **FR-009**: Every marked classification outcome (unknown,
  excluded-not-needed, test-only) MUST be auditable from the SBOM alone
  via provenance annotations recording the evidence source.
  Production-needed components are intentionally unannotated (FR-006):
  their status is signaled by the absence of build-inclusion and
  test-scope markers (consumer default = required).
- **FR-010**: Built-binary evidence MUST take precedence over
  package-level analysis: a module confirmed linked by BuildInfo is
  never excluded or marked unknown, regardless of the toolchain's
  answer.
- **FR-011**: Not-needed classification MUST NOT remove components:
  pre-feature components stay in the output with changed scope and
  annotations only, preserving the superset-of-cyclonedx-gomod
  invariant. The sole permitted set reduction is the PRE-EXISTING
  `--include-dev` handling: modules newly test-tagged by package-level
  analysis follow the same drop-unless-`--include-dev` behavior as all
  other test-scoped dependencies.
- **FR-012**: In `--offline` mode the package-level analysis MUST NOT
  cause network access; if the toolchain cannot answer without network
  (cold module cache), the analysis is skipped per FR-007.
- **FR-013**: Scan logs MUST report classification counts (modules
  analyzed, excluded, test-tagged, unknown-marked, skipped) at info
  level for operator observability.

### Key Entities

- **Build-inclusion status**: per-Go-component classification — one of
  *confirmed-linked* (binary evidence), *production-needed*
  (package-level analysis), *test-only*, *not-needed* (excluded), or
  *unknown* (fallback-discovered, unconfirmed). Exactly one status is
  derivable per component from the emitted SBOM.
- **Evidence source hierarchy**: built-binary BuildInfo > package-level
  toolchain analysis > module-graph reachability > go.sum fallback /
  flat-attach. Higher sources override lower ones; every marked status
  records its source (production-needed is expressed by the absence of
  markers, per FR-009).
- **Classification derivation annotation**: the machine-readable
  discriminator attached alongside scope changes, naming which evidence
  source produced the status.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the kusari-cli anchor repo scanned WITHOUT a Go
  toolchain, 100% of the 20 outside-build-list components carry the
  build-inclusion-unknown marker, and 0 of the remaining components do.
- **SC-002**: On the kusari-cli anchor repo scanned WITH a Go toolchain,
  0 components carry the unknown marker, every component outside
  cyclonedx-gomod's 64-module build list is classified excluded or
  test, and 0 of the 64 build-list modules are excluded (no false
  exclusions).
- **SC-003**: Scans never fail due to toolchain absence or subprocess
  failure: a deliberately-broken-toolchain scan exits 0 with a warning
  and emits a valid SBOM.
- **SC-004**: Existing golden suite passes with zero drift outside Go
  fixtures; Go-fixture drift is limited to the documented FR-001
  markers and FR-004/FR-005 classifications.
- **SC-005**: A consumer can determine the build-inclusion status of
  every Go component from the SBOM alone, in all three output formats,
  without access to the scanned source tree.

## Assumptions

- **Default-on with opt-out**: package-level analysis runs by default
  when a toolchain is detected, mirroring the existing `go mod graph`
  shell-out posture (milestone 055) rather than adding a new opt-in
  flag. A disable flag preserves determinism-sensitive workflows.
  Host-dependent output variation already exists in the resolver ladder;
  this assumption extends, not introduces, it.
- **Unknown stays unscoped**: CycloneDX has no "unknown" scope value;
  marking unknowns `excluded` would be factually wrong. The marker is a
  property/annotation, leaving consumer default-required semantics
  intact (FR-002).
- **Golden churn is intentional**: Go-fixture goldens will be
  regenerated once to absorb the Part B markers; the byte-identity
  discipline then re-pins the new shape (same pattern as prior
  annotation-adding milestones).
- **Source-tier scans only**: image scans and binary-led analysis are
  untouched; BuildInfo remains the authoritative signal when a built
  binary is present (FR-010).
- **PR #332 lands first**: the test-only-closure propagation
  (`fix/go-test-closure-propagation`) is assumed merged before this
  milestone's implementation; Part C's test classification composes
  with, and supersedes where they overlap, that closure tagging.
- **Anchor-repo numbers are point-in-time**: the 87/61/64/20 counts from
  kusaridev/kusari-cli are validation anchors, not contractual values;
  tests pin hermetic fixtures, not the live repo.
- **Toolchain answers are host-platform answers**: classification
  reflects the scan host's platform configuration; cross-platform
  build-inclusion variance is documented, not resolved, by this feature.
