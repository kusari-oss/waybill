# Feature Specification: Post-041 Small Follow-Ons

**Feature Branch**: `042-small-followons`
**Created**: 2026-04-29
**Status**: Draft
**Input**: User description: "Two small follow-on cleanup items: stale binary/predicates.rs comment + Maven sidecar Debian layout"

## User Scenarios & Testing *(mandatory)*

Two unrelated small items closing legacy-deferral debt that has
naturally come due after the milestone-037-through-041 work. Each
is independently testable; bundled together because both are
small and ready to ship without separate PR overhead.

### User Story 1 - Stale binary/predicates.rs comment cleanup (Priority: P1) 🎯 MVP

A maintainer reading mikebom's binary-walker source for the first
time encounters a comment in
`mikebom-cli/src/scan_fs/binary/predicates.rs:131` that names rpm
file-list extraction from HeaderBlob `BASENAMES` /
`DIRNAMES` / `DIRINDEXES` as deferred to a follow-on milestone.
Milestone 040 US3 closed that gap (#78); the comment now
mis-describes the codebase. A future contributor reading this
comment might assume the extraction work is missing and
re-implement it, wasting time. Removing or rewriting the comment
costs ten minutes; not removing it carries the same future-
maintainer-confusion cost the milestone-040 US1 cleanup just paid
down.

**Why this priority**: pure housekeeping. No behavioral risk.
Smallest possible MVP deliverable; sets the tone that this
milestone is incremental, not architectural.

**Independent Test**: After implementation, the comment in
`predicates.rs` no longer claims rpm file-list extraction is
deferred. Specifically, `grep -rn 'BASENAMES.*deferred\|extraction
from HeaderBlob.*deferred' mikebom-cli/src/` returns zero
matches.

**Acceptance Scenarios**:

1. **Given** the post-041 codebase, **When** a contributor greps
   for "deferred" near the rpm-related rationale comments in
   `predicates.rs`, **Then** the comment now describes the
   established rpm reader's responsibilities accurately, with no
   "deferred to a follow-on" framing pointing at work that has
   shipped.
2. **Given** the existing presume-owned heuristic for
   OS-managed-directory binaries when an rpmdb is present,
   **When** the comment is rewritten, **Then** the heuristic's
   rationale is preserved — the heuristic still applies as a
   defense-in-depth fallback even though the authoritative
   extraction is now wired up.

---

### User Story 2 - Maven sidecar Debian layout (Priority: P2)

An SBOM consumer scanning a Debian-based Java image (where
`apt-get install lib*-java` has placed POMs under
`/usr/share/maven-repo/<group-path>/<artifact>/<version>/`)
expects mikebom to recover the Maven coordinates the same way it
already does for Fedora's `/usr/share/maven-poms/` layout. Today
mikebom only reads the Fedora layout; Debian's GAV-tree layout
is parsed and discarded. This becomes visible as
`pkg:generic/<jar-filename>` PURLs instead of
`pkg:maven/<group>/<artifact>@<version>` PURLs for Debian-
sourced Java JARs.

**Why this priority**: closes the explicit "deferred to a
follow-up feature" comment in `maven_sidecar.rs:7-11`. P2 because
the Fedora layout (which mikebom already covers) is the
historically more common case for system-installed Java
libraries, but Debian's variant exists in production images
(any Debian-shaped image where `apt-get install
libcommons-lang3-java` etc. has run).

**Independent Test**: Run `mikebom sbom scan` against a
Debian-shaped rootfs that contains
`/usr/share/maven-repo/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.pom`
and the corresponding JAR. Mikebom's output emits a
`pkg:maven/org.apache.commons/commons-lang3@3.12.0` PURL for the
JAR (today: emits `pkg:generic/commons-lang3-3.12.0`).

**Acceptance Scenarios**:

1. **Given** a Debian-shaped rootfs with one or more
   `/usr/share/maven-repo/<group-path>/<artifact>/<version>/<artifact>-<version>.pom`
   files and matching JARs, **When** the user runs an SBOM scan,
   **Then** the matching JARs surface as
   `pkg:maven/<group>/<artifact>@<version>` PURLs with
   coordinates resolved from the sidecar POMs.
2. **Given** a Debian-shaped rootfs with `/usr/share/maven-repo/`
   present but EMPTY (the directory exists from
   `maven-repo-helper` install but no Java packages have been
   installed), **When** the user runs a scan, **Then** the scan
   completes without error and the output is identical to
   running it against a rootfs with no `/usr/share/maven-repo/`
   directory at all.
3. **Given** a Fedora-shaped rootfs (with
   `/usr/share/maven-poms/`), **When** the user runs a scan
   after this story ships, **Then** the Fedora-layout output is
   byte-identical to milestone 041's output (no regression on
   the existing layout).
4. **Given** a rootfs that has BOTH layouts (extreme edge case;
   wouldn't occur on a real distro), **When** the user runs a
   scan, **Then** both indexes contribute to the JAR-coordinate
   recovery, with the Fedora index winning on basename
   collision (existing milestone-041-era behavior preserved as
   the source-of-truth-for-collisions rule).

---

### Edge Cases

- **Empty `/usr/share/maven-repo/` directory**: graceful no-op,
  matches the Fedora sidecar's existing posture.
- **Malformed POM files within the GAV tree**: skipped per-file
  via `parse_pom_xml`'s existing error-tolerant return shape.
  No scan failure; a debug log records the malformed file.
- **Symlinks within the GAV tree** (Debian's
  `maven-repo-helper` does create some `<artifact>-debian.pom`
  symlinks pointing at the canonical name): follow per
  `read_dir`'s default behavior; emit one entry per resolved
  POM target, dedup by `(group, artifact, version)`.
- **Non-`.pom` files in the GAV tree**: skip silently.
- **Debian-bundled maven layout under
  `/var/lib/maven-repo/`** (rare; some legacy
  packagings): out-of-scope; report as a follow-on if
  surfaced.
- **Alpine `apk` Java packages**: out of scope. Alpine doesn't
  have a documented system-wide maven repo convention; Alpine
  Java images bundle JARs directly without sidecar POMs.

## Requirements *(mandatory)*

### Functional Requirements

#### US1 — Comment cleanup

- **FR-001**: Rewrite the comment in
  `mikebom-cli/src/scan_fs/binary/predicates.rs` (around line
  131) to drop the "RPM file-list extraction from HeaderBlob
  BASENAMES/DIRNAMES/DIRINDEXES is deferred" claim. The
  presume-owned heuristic's rationale (defense-in-depth for
  binaries under OS-managed directories when an rpmdb is
  present) MUST be preserved — it remains correct even now
  that the authoritative extraction is wired up.

- **FR-002**: Post-merge `grep -rn 'extraction from HeaderBlob
  BASENAMES.*deferred\|file-list extraction.*deferred to a
  follow-on milestone' mikebom-cli/src/` returns zero matches.

#### US2 — Maven sidecar Debian layout

- **FR-003**: `mikebom-cli/src/scan_fs/package_db/maven_sidecar.rs`
  gains a parallel index-builder that walks
  `<rootfs>/usr/share/maven-repo/` and recovers
  `(group, artifact, version)` triples from the GAV directory
  tree. The recovered coordinates are looked up by JAR basename
  the same way the existing Fedora sidecar's basename index is.

- **FR-004**: When mikebom scans a Debian-shaped rootfs that
  contains `/usr/share/maven-repo/<group-path>/<artifact>/<version>/`
  POM files plus their matching JARs (regardless of where the
  JAR lives — `/usr/share/java/`, application
  bundle directories, etc.), the resulting SBOM components
  carry `pkg:maven/<group>/<artifact>@<version>` PURLs in
  preference to the previous `pkg:generic/<filename>` PURLs.

- **FR-005**: The Fedora sidecar's basename-index winner-on-
  collision rule MUST be preserved. When both layouts contain a
  POM for the same basename (extreme edge case; wouldn't occur
  on a real distro), the Fedora-layout entry wins. Spec docs
  the chosen tie-break clearly.

- **FR-006**: Empty `/usr/share/maven-repo/` directory →
  graceful no-op, same as Fedora's empty
  `/usr/share/maven-poms/` posture. The scan completes; no
  error log.

#### Cross-cutting

- **FR-007**: No new top-level Cargo dependencies. Reuses
  `parse_pom_xml` (already in the maven module),
  `std::fs::read_dir`, etc.

- **FR-008**: All existing byte-identity goldens MUST regen
  with zero diff. None of the existing 27 fixtures contain
  `/usr/share/maven-repo/`-shaped data, so this milestone's
  behavior is invisible to those fixtures by design.

### Key Entities

- **Debian GAV-tree layout**: directories of the form
  `/usr/share/maven-repo/<group-with-slashes>/<artifact>/<version>/<artifact>-<version>.pom`
  (e.g.
  `/usr/share/maven-repo/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.pom`).
  The group's `.`-segments map to directory segments. Mikebom
  walks the tree and recovers `(group, artifact, version)` per
  POM.
- **Sidecar index**: same shape as the existing
  `FedoraSidecarIndex` — a basename-keyed `HashMap<String,
  PathBuf>` whose values are absolute paths to POM files.
  Lookups by JAR basename retrieve the matching POM for
  coordinate parsing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

#### US1

- **SC-001**: Post-merge `grep -rn 'extraction from HeaderBlob
  BASENAMES.*deferred\|file-list extraction.*deferred to a
  follow-on milestone' mikebom-cli/src/` returns zero matches
  (today: 1 in `binary/predicates.rs`).

#### US2

- **SC-002**: Inline tests verify the Debian sidecar reader
  produces the right `(group, artifact, version)` triples for
  at least 3 GAV layouts (single-segment group, multi-segment
  group, version-with-build-suffix). Quantitatively: ≥3
  tests covering ≥3 layouts in the new
  `DebianSidecarIndex::tests` module.

- **SC-003**: A synthetic-rootfs end-to-end test (constructed
  in inline tests, not from an external image fixture) shows
  that JAR-coordinate recovery via the Debian index produces
  `pkg:maven/<group>/<artifact>@<version>` PURLs equivalent
  to the Fedora-layout output for the same coordinates.

- **SC-004**: A scan of a real Fedora image (not changed by
  this milestone) and a scan of `debian:bookworm-slim` (no
  `/usr/share/maven-repo/` content; expected no change)
  produce SBOMs byte-identical to milestone-041's output
  for those rootfs shapes.

#### Cross-cutting

- **SC-005**: Existing byte-identity goldens regen with zero
  diff (FR-008).
- **SC-006**: All 3 CI lanes green.

## Assumptions

- The Debian Maven layout per
  <https://wiki.debian.org/Java/MavenBuilder> /
  <https://salsa.debian.org/java-team/maven-repo-helper> is
  stable: GAV-tree under `/usr/share/maven-repo/`. Mikebom
  reads the canonical layout; symlinks and Debian-specific
  variants (e.g. `<artifact>-debian.pom`) flow through the
  same per-POM lookup.
- mikebom's existing `parse_pom_xml` handles real-world POM
  shapes encountered in Debian's repo (single-pom-per-leaf-dir
  is the canonical layout; multi-pom-per-dir doesn't occur in
  practice on Debian).
- The integration site for the new index is
  `package_db::maven`'s call into `FedoraSidecarIndex`. We
  add a parallel `DebianSidecarIndex` and consult both during
  JAR-coordinate recovery; the maven module's existing logic
  for "use the sidecar when in-JAR metadata is absent" carries
  through unchanged.
- Alpine-equivalent layouts are out-of-scope; Alpine Java
  images don't ship a system-wide maven repo today.

## Out of scope

- Alpine maven sidecar (no documented system-wide Alpine maven
  layout).
- Debian-bundled maven layouts under
  `/var/lib/maven-repo/` (uncommon legacy variant).
- Schema-level `hashes` array refactor on `FileOccurrence` —
  pre-existing deferred item.
- Container layer attribution — the bigger architectural item.
- Other-ecosystem deep-hash audits (npm / cargo / go / python)
  — deferred for a future milestone if those ecosystems
  prove to need per-file evidence.
