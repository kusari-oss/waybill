# Feature Specification: `mikebom:component-role` annotation for build-tool / language-runtime classification

**Feature Branch**: `048-component-role`
**Created**: 2026-04-30
**Status**: Draft
**Input**: User description: "let's do option 2 and also ensure we follow similar patterns for SPDX in addition to CycloneDX"

## Background

A v0.1.0-alpha.7 conformance run against the polyglot-builder-image
fixture surfaced a real-world friction point: mikebom emits 107
top-level Maven components and the ground-truth declares 108, with
0 missing — but **3 false positives** that aren't application
dependencies:

- `pkg:maven/org.apache.maven/maven-artifact@3.1.0` —
  `/usr/share/maven/lib/maven-artifact-3.1.0.jar`. Maven's own
  internal core, installed by the Fedora `maven` rpm.
- `pkg:maven/commons-cli/commons-cli@1.5.0` — Maven's runtime
  dep for parsing CLI flags.
- `pkg:maven/org.slf4j/slf4j-simple@1.7.36` — logging binding
  shipped alongside Maven.

mikebom is **technically correct** to emit them: they're real
JARs physically present on disk with valid POMs. Class-presence
verification (milestone 009) considers them legitimate Maven
artifacts. But they're build tooling, not application dependencies
— and the conformance ground-truth (deployed-application scope)
correctly excludes them.

This is the textbook case milestone 047's framing was designed
to expose: a scope-mismatch between what mikebom emits ("everything
physically here") and what some consumers want ("application code
only, not the build tool that produced it"). Milestone 047 closed
the document-level scope-mode gap; this milestone closes the
component-level role gap.

The fix is **enrichment, not omission**. Adding a
`mikebom:component-role` annotation that tags components based on
filesystem-location heuristics lets consumers filter without
mikebom dropping any component. mikebom keeps reporting what's
there; consumers pick the role-set they want.

The same friction will appear in:

- JDK-bundled JARs in any Java image (`/usr/lib/jvm/*/lib/*`)
- Gradle / sbt / mvnw build tool jars in builder images
- Debian's `nodejs` package's bundled npm internals
  (`/usr/lib/node_modules/*` system-managed, not application
  `node_modules/*`)
- System Python packages (cpython's stdlib `.dist-info` dirs in
  `/usr/lib/python*/`)

The path-heuristic table can grow as cases surface; the
foundational annotation + parity-extractor wiring is what this
milestone ships.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Tag build-tool components in builder-image scans (Priority: P1) 🎯 MVP

A conformance-suite operator scans `polyglot-builder-image`
expecting the SBOM to represent the deployed application. mikebom
reports 110 Maven components (107 application + 3 Maven build
tooling). Today the operator has no signal in the SBOM to
distinguish the two; their tooling treats all 110 as application
deps and emits false positives in vulnerability scans / license
audits / drift reports.

After this milestone, every component mikebom emits with a
filesystem location matching a curated build-tool path heuristic
(`/usr/share/maven/lib/`, `/usr/share/gradle/`, `/opt/sbt/`,
similar) carries an explicit `mikebom:component-role = "build-tool"`
annotation. The 3 Maven jars from the polyglot fixture are
unambiguously tagged. Consumers (conformance suite, vulnerability
scanners, license auditors) can now filter on this annotation
without re-implementing path heuristics on their own.

**Why this priority**: closes the most-asked friction point from
the conformance suite. Direct, scoped, generalizes to other
build-tool images (gradle, sbt, mvnw) as the heuristic table
grows.

**Independent Test**: After implementation, scanning a fixture
with `/usr/share/maven/lib/maven-artifact-3.1.0.jar` produces a
CDX component for that JAR carrying property
`name = "mikebom:component-role"`, `value = "build-tool"`. SPDX
2.3 + SPDX 3 carry the same role via their respective annotation
envelopes.

**Acceptance Scenarios**:

1. **Given** an image with `/usr/share/maven/lib/<name>-<version>.jar`
   files (typical Fedora-built / Debian-built Java builder
   images), **When** mikebom scans, **Then** every emitted
   component sourced from that path carries
   `mikebom:component-role = "build-tool"` in CDX, SPDX 2.3, and
   SPDX 3 outputs.
2. **Given** an image with `/usr/share/java/<name>-<version>.jar`
   (the standard Debian system-Java package layout — application
   libraries shared with the system, NOT build tooling),
   **When** mikebom scans, **Then** the emitted components do
   NOT carry the `build-tool` role tag (they are application
   dependencies installed via dpkg).
3. **Given** an image with both `/usr/share/maven/lib/foo-1.0.jar`
   AND a co-named application jar at `/app/lib/foo-1.0.jar`,
   **When** mikebom scans, **Then** each emission carries the
   role appropriate to its filesystem location — one
   `build-tool`, one without the tag — even though the
   `pkg:maven/...` PURL would otherwise dedupe them.

---

### User Story 2 — Tag language-runtime components in JDK / system-package scans (Priority: P2)

The same scope friction exists for **language runtimes**: a JDK's
own bundled JARs (`/usr/lib/jvm/*/lib/*` — `tools.jar`,
`jrt-fs.jar`, etc.) and system-managed Python / Node / Ruby
packages (`/usr/lib/python*/site-packages/*`,
`/usr/lib/node_modules/*` from Debian's `nodejs` package, etc.)
should be distinguishable from application code. They're not
build tooling per se, but they're not application code either —
they're the platform the application runs on.

After this milestone, components emitted from these system-managed
runtime locations carry
`mikebom:component-role = "language-runtime"`. Consumers wanting
"just the application" filter both `build-tool` AND
`language-runtime`; consumers wanting "the full runtime image"
read the SBOM as-is.

**Why this priority**: same shape as US1, different paths.
Bundled together because the role-tagging mechanism is identical;
only the path-heuristic table grows. P2 because Maven build
tooling is the active conformance-fixture friction; language-
runtime tagging closes a slightly less acute gap.

**Independent Test**: After implementation, scanning a fixture
with `/usr/lib/jvm/<jdk>/lib/jrt-fs.jar` (or a comparable Python /
Node case) produces a component carrying
`mikebom:component-role = "language-runtime"` in all three
formats.

**Acceptance Scenarios**:

1. **Given** an image with `/usr/lib/jvm/java-21-openjdk/lib/<jar>`
   files, **When** mikebom scans, **Then** components from those
   paths carry `mikebom:component-role = "language-runtime"`.
2. **Given** an image with `/usr/lib/python3.11/site-packages/<pkg>/*`
   (system-installed Python packages from the `python3` Debian
   package, NOT a venv or application's installed packages),
   **When** mikebom scans, **Then** components from that path
   carry the `language-runtime` tag.
3. **Given** a Python application's venv at
   `/app/.venv/lib/python3.11/site-packages/<pkg>` (NOT a
   system-managed runtime path), **When** mikebom scans,
   **Then** components from that path do NOT carry the
   `language-runtime` tag — they're application dependencies.

---

### User Story 3 — Cross-format parity preserves the role tag in CDX, SPDX 2.3, SPDX 3 (Priority: P1, bundled with US1) 🎯 MVP

A consumer reading the SPDX 2.3 output of the same scan must see
the same role classification a CDX consumer does. mikebom's
parity-extractors framework asserts SymmetricEqual on every C-row
catalog annotation across all three formats; the new
`mikebom:component-role` annotation MUST land as a new C-row and
participate in that parity matrix.

**Why this priority**: bundled with US1 (not separable from it).
Per the project's
[SPDX dual-format constitution requirements](docs/reference/sbom-format-mapping.md),
every per-component property mikebom emits in CDX has a documented
target in SPDX 2.3 (annotation envelope) and SPDX 3 (annotation
envelope). Shipping the CDX side without the SPDX siblings would
violate the cross-format parity gate that
`holistic_parity` enforces.

**Independent Test**: After the milestone, the
`holistic_parity` test suite continues to pass with the new C-row
included. A scan emits the role property in CDX, the equivalent
annotation in SPDX 2.3, and the equivalent annotation in SPDX 3,
and the parity-extractors framework's `SymmetricEqual` check
holds for the new row.

**Acceptance Scenarios**:

1. **Given** a scan that produces at least one component carrying
   `mikebom:component-role = "build-tool"`, **When**
   `cargo +stable test -p mikebom --test holistic_parity` runs,
   **Then** all 11 ecosystems pass with the new C-row
   participating in the SymmetricEqual matrix.
2. **Given** the catalog row added in
   `docs/reference/sbom-format-mapping.md` for
   `mikebom:component-role`, **When** the
   `every_catalog_row_has_an_extractor` test runs, **Then** the
   new row has matching extractors registered for all three
   formats.

---

### Edge Cases

- **Components with no filesystem location**: lockfile-only
  components (no on-disk JAR / no `evidence.occurrences[]` path)
  CAN'T be classified by path heuristic. The annotation is
  omitted for these; absence means "not classified by this
  heuristic" — NOT "definitely application code". Spec docs the
  three-state semantics: `build-tool`, `language-runtime`,
  absent.
- **Components with multiple filesystem locations**: when a
  component's `evidence.occurrences[]` lists paths in BOTH a
  classified location AND an unclassified one (e.g., a JAR shows
  up in both `/usr/share/maven/lib/` AND `/app/lib/`), the
  annotation reflects the FIRST path that hits a heuristic —
  whichever role is observed at all gets recorded. Consumers
  needing per-occurrence role data walk
  `evidence.occurrences[]`. (Per-occurrence role tagging is
  out-of-scope; component-level annotation is the contract.)
- **Path heuristic false positive risk**: a path matches the
  heuristic but the component genuinely IS application code
  (e.g., an application that ships under `/usr/share/maven/lib/`
  by convention — extremely rare). Acceptable risk: the heuristic
  table is curated and limited to well-known system-managed paths
  whose contents are conventionally build-tool / runtime. False
  positives can be hand-corrected per-fixture in conformance GT
  via `severity: advisory` if they ever appear.
- **Other non-application roles**: `test-fixture`,
  `documentation`, `localization`, `vendored-source`, etc. all
  exist conceptually but are out of scope for this milestone. The
  enum is open for future extension.
- **CDX vs SPDX value-encoding**: CDX uses `properties[]` with
  `value` as a string. SPDX 2.3 / SPDX 3 use the `mikebom:`
  annotation envelope with the same string value. No format-
  specific value transformation needed.

## Requirements *(mandatory)*

### Functional Requirements

#### US1 — `build-tool` role tagging

- **FR-001**: mikebom MUST emit a `mikebom:component-role =
  "build-tool"` annotation on every component whose
  filesystem location (one of `evidence.occurrences[].location`
  or the component's primary on-disk path) matches one of the
  curated build-tool path heuristics.
- **FR-002**: The initial build-tool path heuristic table MUST
  cover at least:
  - `/usr/share/maven/lib/` (Maven, Fedora + Debian default
    install location)
  - `/usr/share/gradle/lib/` (Gradle, Debian default install
    location)
  - `/opt/sbt/` (sbt, Lightbend default install location)
- **FR-003**: When a component's filesystem location does NOT
  match any curated heuristic path, the
  `mikebom:component-role` annotation MUST be omitted (NOT set
  to `application` or `unknown`). Three-state semantics:
  `build-tool`, `language-runtime`, absent.

#### US2 — `language-runtime` role tagging

- **FR-004**: mikebom MUST emit a `mikebom:component-role =
  "language-runtime"` annotation on every component whose
  filesystem location matches one of the curated
  language-runtime path heuristics.
- **FR-005**: The initial language-runtime path heuristic table
  MUST cover at least:
  - `/usr/lib/jvm/*/lib/` (JDK system-installed bundled JARs)
  - `/usr/lib/node_modules/` (Debian / Ubuntu's `nodejs`
    package — system-managed, NOT application
    `node_modules/`)
  - `/usr/lib/python*/site-packages/` (system Python
    `python3-*` apt packages, NOT venv-installed)
  - `/usr/lib/python*/dist-packages/` (Debian's distinction
    between Debian-packaged and locally-installed Python
    packages)

#### US3 — Cross-format parity (bundled with US1)

- **FR-006**: A new C-row in
  `docs/reference/sbom-format-mapping.md` MUST document the
  `mikebom:component-role` annotation. The row MUST classify the
  annotation as `Present` × 3 formats × `SymmetricEqual`.
- **FR-007**: The CDX serializer MUST emit
  `mikebom:component-role` as a `properties[]` entry on the
  component (string value).
- **FR-008**: The SPDX 2.3 serializer MUST emit
  `mikebom:component-role` as a `packages[].annotations[]` entry
  using the existing `mikebom:` envelope shape.
- **FR-009**: The SPDX 3 serializer MUST emit
  `mikebom:component-role` as a top-level `annotations[]` entry
  per the existing pattern.
- **FR-010**: The `holistic_parity` test MUST pass with the new
  C-row participating; `every_catalog_row_has_an_extractor` MUST
  pass with the new row's extractor registered for all three
  formats.

#### Cross-cutting

- **FR-011**: No new top-level Cargo dependencies. The path-
  heuristic logic uses `std::path` primitives only.
- **FR-012**: 27 byte-identity goldens regen with deltas only
  on fixtures that contain heuristic-matched paths. Most
  synthetic fixtures (cargo, gem, pip, npm, golang, rpm, deb,
  apk, maven-3-deps) have no `/usr/share/maven/lib/`-style
  paths, so their goldens MUST regen with zero diff.
- **FR-013**: No CLI flag changes. No
  `--include-declared-deps` semantics change. No
  `mikebom:sbom-tier` value changes.

### Key Entities

- **Component role**: a string-valued annotation classifying a
  component's filesystem-determined role in the image.
  Permitted values for this milestone: `build-tool`,
  `language-runtime`, absent. Open enum — future milestones
  MAY add values (`test-fixture`, `documentation`, etc.).
- **Path heuristic**: a static (path-prefix, role) pair
  consulted at component-emission time. The heuristic table
  lives in code (a small `&[(prefix, role)]` slice); future
  additions are mechanical edits.

## Success Criteria *(mandatory)*

### Measurable Outcomes

#### US1

- **SC-001**: A scan of a fixture with
  `/usr/share/maven/lib/maven-artifact-3.1.0.jar` produces a
  CDX component carrying property
  `name = "mikebom:component-role"`, `value = "build-tool"`.
  Verifiable via `jq '.components[] | select(...)
  | .properties[] | select(.name == "mikebom:component-role")
  | .value'`.

#### US2

- **SC-002**: A scan of a fixture with
  `/usr/lib/jvm/<jdk>/lib/<jar>` produces a CDX component with
  `mikebom:component-role = "language-runtime"`.

#### US3

- **SC-003**: `cargo +stable test -p mikebom --test
  holistic_parity` passes with all 11 cases including the new
  C-row in the SymmetricEqual matrix.
- **SC-004**: `every_catalog_row_has_an_extractor` test passes
  with the new C-row registered for all three formats.

#### Cross-cutting

- **SC-005**: Existing byte-identity goldens that DO NOT touch
  heuristic-matched paths regen with **zero diff**. Specifically:
  the cargo, gem, pip, npm, golang, rpm, deb, apk, and
  maven-3-deps goldens MUST be byte-identical pre-and-post
  milestone (no synthetic fixture there carries
  `/usr/share/maven/lib/` or other heuristic paths).
- **SC-006**: `./scripts/pre-pr.sh` clean (clippy + workspace
  tests). All 3 CI lanes green on the milestone PR.
- **SC-007**: A new inline test in the binary scanner /
  filesystem walker covers the heuristic dispatch — given a
  classified path, the role is emitted; given an unclassified
  path, the annotation is absent.

## Assumptions

- The audit-grounded conformance friction (3 Maven build-tool
  JARs flagged as FALSE_POSITIVE in the polyglot-builder-image
  fixture) is real and solvable through annotation rather than
  emission removal. mikebom's "report what's physically here"
  semantics are deliberate and align with milestone 047's
  scope-mode framing.
- Path heuristics are sufficient for the canonical cases. The
  initial table covers Maven / Gradle / sbt / JDK / system
  Python / system Node — the cases conformance fixtures and
  real-world builder images surface today. New cases extend the
  table as they appear.
- Per-component annotation is the right granularity (vs
  per-occurrence). When a single component has multiple
  on-disk paths and only some match the heuristic, the
  component carries the role of any matched path. Per-occurrence
  role tagging is feasible but not in scope.
- The CDX `properties[]` + SPDX 2.3/3 annotation envelope shape
  is unchanged from how the existing C-rows already emit. No
  schema migration; one new C-row.
- No CHANGELOG entry needed beyond a single Added line. This is
  a small additive emission. The conformance-suite's
  consumption of the new annotation is a follow-on in the
  sbom-conformance repo.

## Out of scope

- The conformance-suite consuming the new annotation
  (`severity: advisory` declarations keyed on
  `mikebom:component-role` in the polyglot fixture's GT). That
  belongs in the sbom-conformance repo's follow-on work.
- Per-occurrence role annotation. A single component with mixed
  paths gets one role at the component level, not a per-path
  role on each `evidence.occurrences[]` entry. Future milestone
  if real consumers need it.
- Other role values (`test-fixture`, `documentation`,
  `localization`, `vendored-source`, etc.). Open enum;
  extensions are mechanical follow-ons.
- Heuristic refinement based on package metadata (e.g.,
  recognizing a Maven JAR by its POM's `groupId=org.apache.maven`
  rather than its filesystem location). Filesystem-only is
  cleaner; package-metadata-based detection is a separable
  axis if it ever proves needed.
- Adding heuristic-bypass / explicit-classification flags to
  the CLI. The path-heuristic table is the source of truth;
  configurability is a future-milestone concern if real
  consumers ask for it.
- Any change to mikebom's emission decisions (which components
  get emitted, when class-presence verification fires, etc.).
  This milestone is purely additive metadata.
- Removing or renaming any existing annotation. The
  `mikebom:sbom-tier` (per-component scope tier) and
  `mikebom:component-role` (per-component role) are independent
  axes; both ship.
