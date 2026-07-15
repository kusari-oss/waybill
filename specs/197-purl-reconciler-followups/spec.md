# Feature Specification: m190 + m191 Follow-Up Bundle

**Feature Branch**: `197-purl-reconciler-followups`
**Created**: 2026-07-15
**Status**: Draft
**Input**: User description: "m190/m191 followups"

## Overview

Milestones 190 (ipk emission parity — closed 6 issues) and 191 (design-tier
/ source-tier reconciliation — closed 2 issues) each surfaced a small set
of scoped-out follow-up items filed as GitHub issues at ship time. Six of
those issues remain open (#562, #563, #564, #565, #566, #567) — none
large individually, but collectively enough that treating them as one
bundle makes reviewer and merge-cadence sense.

This milestone bundles the six follow-ups into a single spec-driven work
stream so they land together rather than trickling through six sequential
PRs. Two m190-lineage items are PURL-emission audits (dpkg + apk epoch
handling); four m191-lineage items are reconciler + versionless-PURL
completeness work (npm alias handling, multi-declaration preservation, a
round-trip fuzz test across all 11 ecosystems, and extending the
versionless-PURL fix to the six ecosystems the m191 MVP deferred).

## Clarifications

### Session 2026-07-15

- Q: How should the multi-value declaration-provenance annotations shape? → A: Always-array. Every m191 reconciler survivor gets `mikebom:requirement-ranges` (array) + `mikebom:source-manifests` (array), single-element for the common one-manifest case. Consumer contract is uniform (`.length` inspection); no conditional-key parsing. Breaks FR-007 byte-identity for the survivor's declaration-provenance fields (m191 singular scalars rotate to 1-element arrays); existing goldens covering the m191 reconciler path require regen. This is the ONLY class of FR-007 exception m197 introduces.
- Q: Should the epoch audit include rpm as a third target? → A: Yes — audit rpm alongside dpkg + apk. All three ecosystems likely share the same class of bug (naïve `Version:` field parsing that doesn't split on `:`) inherited from the m190 opkg fix path. Auditing all three closes the epoch-bug class comprehensively rather than leaving a fourth ecosystem exposed. Rpm audit is m197-native (no pre-existing GH issue) — the m197 PR body notes the discovery/coverage, no `Closes #NNN` line.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Debian/Ubuntu epoch-versioned deb packages emit correct PURLs (Priority: P1)

An operator scans a rootfs (or `.deb` artifact) that contains a Debian
package with an epoch-prefixed version like `Version: 1:2.0-r0`. Today
the dpkg reader may embed the epoch inline in the PURL version segment
(`pkg:deb/debian/foo@1:2.0-r0`) instead of using the `?epoch=1` qualifier
per purl-spec. Downstream vuln scanners keyed off purl-spec form
would match wrong-version or miss the vuln entirely. This story mirrors
the m190 opkg-side fix (#552) for the dpkg reader, ensuring epoch-
versioned deb packages emit as `pkg:deb/debian/foo@2.0-r0?epoch=1`.

**Why this priority**: Debian/Ubuntu are the most common Linux base
image families mikebom scans. Any epoch handling bug directly affects
production-image SBOMs at customer scale. Closes #562.

**Independent Test**: Build a synthetic `.deb` fixture with
`Version: 1:2.0-r0`. Scan it. Assert the emitted CDX / SPDX
components carry `pkg:deb/debian/<name>@2.0-r0?epoch=1` — NOT
`pkg:deb/debian/<name>@1:2.0-r0`.

**Acceptance Scenarios**:

1. **Given** a synthetic `.deb` package with an epoch in `Version:`,
   **When** mikebom scans it,
   **Then** the emitted PURL uses the `?epoch=<N>` qualifier form.
2. **Given** a `.deb` package with NO epoch,
   **When** mikebom scans it,
   **Then** no `?epoch=` qualifier appears and the version is
   unchanged from the pre-milestone shape.

---

### User Story 2 - Alpine .apk epoch-versioned packages emit correct PURLs (Priority: P1)

Same class of bug as US1, for the apk reader. Alpine's `apk` package
manager uses versions like `1:2.0-r0` in some contexts. Verifies apk
reader emits `pkg:apk/alpine/<name>@2.0-r0?epoch=1`, not
`pkg:apk/alpine/<name>@1:2.0-r0`.

**Why this priority**: Alpine is the other common Linux base image
family alongside Debian. Closes #563.

**Independent Test**: Same shape as US1 with an `.apk` fixture.

**Acceptance Scenarios**:

1. **Given** a synthetic `.apk` package with an epoch prefix in its
   version,
   **When** mikebom scans it,
   **Then** the emitted PURL uses `?epoch=<N>` qualifier form.
2. **Given** an `.apk` package with NO epoch,
   **When** mikebom scans it,
   **Then** no `?epoch=` qualifier appears.

---

### User Story 2b - RPM epoch-versioned packages emit correct PURLs (Priority: P1)

Per Q2 clarification: same class-of-bug audit as US1 / US2, extended to
the rpm reader. Verifies rpm emission of epoch-prefixed versions uses
`?epoch=<N>` qualifier form (per purl-spec rpm-type extension) instead
of inline `<N>:<version>` embedded in the PURL version segment. If the
rpm reader is already-correct (has an existing epoch-qualifier code
path from m003 / m004 / m144), the audit closes the loop with a
non-regression fixture. If broken, mirror the same fix as US1 / US2.

**Why this priority**: CentOS / Rocky / Fedora / RHEL images are the
third major rpm-based Linux family alongside deb / apk; the same
customer-scan class of bug applies. Native to m197 — no pre-existing
GH issue; the m197 PR body notes the discovery.

**Independent Test**: Synthetic `.rpm` fixture with an epoch-prefixed
version (e.g., `Version: 1:2.0`). Scan; assert emitted CDX / SPDX
PURLs use `pkg:rpm/<distro>/<name>@2.0?epoch=1`.

**Acceptance Scenarios**:

1. **Given** a synthetic `.rpm` package with an epoch prefix in its
   version,
   **When** mikebom scans it,
   **Then** the emitted PURL uses `?epoch=<N>` qualifier form.
2. **Given** an `.rpm` package with NO epoch,
   **When** mikebom scans it,
   **Then** no `?epoch=` qualifier appears and the version segment
   is byte-identical to pre-m197 output.

---

### User Story 3 - Extend versionless-PURL fix to 6 additional ecosystems (Priority: P1)

The m191 MVP (#558) shipped the versionless-PURL fix for 5 primary
ecosystems (npm, cargo, maven, gem, pip). Six additional ecosystems
mikebom emits — composer, dart, cocoapods, scala, haskell, erlang —
still use pre-m191 code paths that may emit invalid PURLs when the
version is absent (bare `pkg:composer/foo@` with trailing `@` and no
version, per purl-spec being invalid). This story extends the m191
fix to those 6 ecosystems.

**Why this priority**: purl-spec conformance is a cross-cutting
consumer contract. Any ecosystem that emits malformed PURLs breaks
vuln-lookup pipelines that key off well-formed PURLs. Closes #567.

**Independent Test**: For each of the 6 ecosystems, construct a
synthetic scan target with a versionless dep (e.g., a `composer.json`
declaring a dep without pinning a version). Assert the emitted PURL
follows the versionless canonical form (no trailing `@`) per
purl-spec.

**Acceptance Scenarios**:

1. **Given** a scan target that produces a versionless dep in one of
   the 6 ecosystems (composer / dart / cocoapods / scala / haskell /
   erlang),
   **When** mikebom emits the SBOM,
   **Then** the PURL matches the purl-spec canonical form for the
   versionless case (no trailing `@`).
2. **Given** the same 6 ecosystems' targets WITH versions pinned,
   **When** mikebom emits the SBOM,
   **Then** the PURLs are byte-identical to pre-m191 output (m191 was
   version-form-preserving).

---

### User Story 4 - Fuzz-test versionless PURL round-trip across all 11 ecosystems (Priority: P2)

m191 spec SC-004 called for a fuzz test covering all 11 ecosystems.
The MVP shipped targeted unit tests per ecosystem, not a fuzz-style
exhaustive corner-case sweep. This story adds a
proptest-style-or-equivalent generator that produces 100+ synthetic
versionless PURLs across all 11 ecosystems (npm scoped-name grammar,
maven groupId+artifactId variants, cargo package-name segment
grammar, gem underscores, pip PEP 508 normalization edge cases,
etc.) and verifies (a) each PURL parses successfully as a `Purl`,
(b) parsed → re-serialized round-trip is byte-identical, (c)
`.ecosystem()` and `.name()` accessors return the expected values.

**Why this priority**: Catches per-ecosystem corner cases the
targeted unit tests miss (URL-encoded segments, max-length names,
scoped-name grammars, ecosystem-specific normalization quirks).
Closes #566.

**Independent Test**: `cargo test -p mikebom-common -- versionless_purl_fuzz`
completes with zero failures and covers a minimum of 100 synthetic
inputs per ecosystem (11 × 100 ≥ 1100 total invocations).

**Acceptance Scenarios**:

1. **Given** the fuzz-test suite,
   **When** it runs,
   **Then** it exercises at least 100 synthetic versionless PURL
   inputs per ecosystem across all 11 ecosystems and every input
   round-trips byte-identically.
2. **Given** a hypothetical regression in the `Purl` type's canonical
   serialization,
   **When** the fuzz suite runs post-regression,
   **Then** at least one input surfaces a byte-drift failure with
   an actionable diagnostic (which ecosystem, which name shape,
   observed vs expected).

---

### User Story 5 - npm alias declarations resolve correctly in the reconciler (Priority: P2)

npm's `"my-alias": "npm:actual-pkg@1.0.0"` pattern lets a package.json
declare a dep under a name different from its published identity. The
m191 reconciler matches design-tier ↔ source-tier by
`(ecosystem, canonical_name, source_manifest_dir)`. When an alias is
declared, the design-tier component was keyed on `my-alias` but the
source-tier resolved component is keyed on `actual-pkg` — the
reconciler misses the match. This story extends the reconciler to
recognize npm-alias declarations and match by resolved-identity,
adding a `mikebom:declared-as` annotation on the survivor recording
the original alias for provenance.

**Why this priority**: npm-alias patterns are common in React /
frontend projects (versioned framework migration, monorepo
workspace-alias patterns). Missing the reconciler match produces
duplicate components in the emitted SBOM — the m191 problem class.
Closes #564.

**Independent Test**: Fixture with a `package.json` declaring a dep
via `npm:actual-pkg@1.0.0` alias. Post-scan, assert (a) no duplicate
components emitted, (b) surviving component carries
`mikebom:declared-as: <alias>` annotation.

**Acceptance Scenarios**:

1. **Given** a `package.json` declaring `my-alias:
   npm:actual-pkg@1.0.0`,
   **When** mikebom emits the SBOM,
   **Then** exactly one `pkg:npm/actual-pkg@1.0.0` component appears
   AND it carries `mikebom:declared-as: my-alias`.
2. **Given** a `package.json` with NO alias declarations,
   **When** mikebom emits the SBOM,
   **Then** no `mikebom:declared-as` annotation appears anywhere.

---

### User Story 6 - Reconciler preserves ALL declared ranges + paired manifests on the survivor (Priority: P2)

Workspace pattern: `packages/foo/package.json` declares
`commander: "^11.0"` and `packages/bar/package.json` declares
`commander: "^11.1.0"`. Root lockfile resolves both to
`commander@11.1.0`. m191 MVP transfers only the FIRST reconciled
range onto the survivor — subsequent declarations are dropped, losing
provenance. This story preserves all declared ranges + all paired
source manifests as arrays on the survivor.

**Why this priority**: The dropped-range data is provenance signal
consumers use to answer "why is this dep here?". Losing it in
monorepos undermines the m191 reconciler's stated purpose (rich
provenance-preserving dedup). Closes #565.

**Independent Test**: Fixture with 2 sibling manifests declaring
different ranges resolving to the same source-tier component.
Assert both ranges + both manifest paths appear on the survivor's
annotations as arrays, not first-wins scalars.

**Acceptance Scenarios**:

1. **Given** N sibling manifests declaring different-but-compatible
   ranges of the same dep resolving to the same source-tier
   component,
   **When** mikebom emits the SBOM,
   **Then** the survivor carries `mikebom:requirement-ranges` (array
   of N ranges) and `mikebom:source-manifests` (array of N manifest
   paths), NOT just the first.
2. **Given** a single-manifest case (no multi-declaration),
   **When** mikebom emits the SBOM,
   **Then** per Q1 clarification the survivor emits
   `mikebom:requirement-ranges` (single-element array) +
   `mikebom:source-manifests` (single-element array) — uniform
   with the multi-declaration case. The m191 singular scalars
   `mikebom:requirement-range` / `mikebom:source-manifest` no longer
   appear on reconciler survivors post-m197.

---

### Edge Cases

- **Reconciler-alias + multi-declaration combined**: `my-alias` in
  one manifest, `another-alias` in a sibling manifest, both
  resolving to the same source-tier component. Survivor carries both
  aliases in `mikebom:declared-as` (array) AND both manifests in
  `mikebom:source-manifests`.
- **Epoch of `0`**: `Version: 0:1.0-r0` (Debian's canonical form for
  "no epoch" is implicit; explicit `0:` is unusual but permitted).
  US1/US2: `?epoch=0` qualifier still emitted (accurate to source),
  not stripped.
- **Versionless PURL with URL-encoded segments**: `pkg:composer/foo/bar`
  where `foo` needs URL-encoding — US3 canonicalization must preserve
  the encoding through the versionless form.
- **Fuzz-test max-length names**: US4 generator produces
  ecosystem-specific max-length names (e.g., 214-char npm name limit,
  200-char cargo limit) to catch length-related panics.
- **Reconciler survivor carrying > 100 declared ranges**: US6 arrays
  are unbounded in principle. If they grow to a length that harms
  SBOM readability, a follow-up milestone can cap + truncate; not
  in scope for m197.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The dpkg reader MUST emit PURLs for epoch-versioned deb
  packages using the `?epoch=<N>` qualifier form per purl-spec, NOT
  inline in the version segment (US1).
- **FR-002**: The apk reader MUST emit PURLs for epoch-versioned apk
  packages using the `?epoch=<N>` qualifier form per purl-spec (US2).
- **FR-002a**: The rpm reader MUST emit PURLs for epoch-versioned rpm
  packages using the `?epoch=<N>` qualifier form per purl-spec (US2b,
  per Q2 clarification). If the reader is already-correct, the FR is
  satisfied by a non-regression fixture; if broken, the fix mirrors
  FR-001 / FR-002.
- **FR-003**: The versionless-PURL emission fix from m191 (#558) MUST
  be extended to the composer, dart, cocoapods, scala, haskell, and
  erlang readers so their versionless outputs match purl-spec
  canonical form (US3).
- **FR-004**: A fuzz-style round-trip test MUST exist covering all 11
  ecosystems mikebom emits (npm, cargo, maven, gem, pip, composer,
  dart, cocoapods, scala, haskell, erlang) with ≥ 100 synthetic
  inputs per ecosystem (US4).
- **FR-005**: The m191 reconciler MUST recognize npm-alias
  declarations and match design-tier ↔ source-tier by resolved
  identity, preserving the original alias name as a
  `mikebom:declared-as` annotation on the survivor (US5).
- **FR-006**: The m191 reconciler MUST preserve ALL declared ranges
  and paired source-manifest paths on the survivor. Per Q1
  clarification, emission is **always-array**: every survivor emits
  `mikebom:requirement-ranges` (JSON array of range strings) +
  `mikebom:source-manifests` (JSON array of manifest paths), single-
  element for the one-manifest case and N-element for the N-manifest
  case. Consumers key on `.length`; no conditional field-name
  parsing. This supersedes the m191 singular scalars
  `mikebom:requirement-range` / `mikebom:source-manifest` for the
  survivor — those field names no longer appear on reconciler
  survivors post-m197.
- **FR-007**: All 6 pre-existing readers (npm / cargo / maven / gem /
  pip / opkg — the m190 + m191 primary set) MUST remain byte-
  identical for outputs that don't hit the new code paths (no
  epoch, no versionless, no alias, no multi-declaration) — this
  milestone is purely additive to those readers **with ONE explicit
  exception**: the m191 reconciler-survivor declaration-provenance
  fields rotate from singular scalars to always-array shape per Q1
  clarification (see FR-006). Existing goldens exercising the m191
  reconciler code path require regen; every other golden byte-
  identically holds.
- **FR-008**: `./scripts/pre-pr.sh` MUST continue to pass green after
  m197 lands.

### Key Entities

- **Epoch-qualified PURL**: A PURL of the form
  `pkg:<type>/<namespace>/<name>@<version-without-epoch>?epoch=<N>`.
  The `?epoch=` qualifier is a purl-spec-blessed extension; consumers
  are expected to combine `version` + `epoch` for canonical dep
  identity.
- **Versionless PURL (canonical)**: `pkg:<type>/<namespace>/<name>`
  (no `@` sigil, no trailing empty version). Per purl-spec, this is
  the correct form for a dep whose version is unknown / unset —
  mikebom's design-tier components produced from range-only
  declarations that don't resolve to a lockfile version.
- **Reconciler Survivor**: The single component that remains after
  the m191 design-tier ↔ source-tier reconciliation pass. This
  milestone extends the survivor's annotations to include arrays of
  alias names + declared ranges + source-manifest paths.
- **npm Alias Declaration**: A `dependencies` entry of the form
  `"alias-name": "npm:actual-name@version-spec"`. The alias-name is
  what appears in `require()` calls; the actual-name is what npm
  resolves and installs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every open follow-up issue in the set {#562, #563,
  #564, #565, #566, #567} is closed by the m197 PR (linked via
  `Closes #NNN` in the commit / PR body). Additionally, the rpm
  epoch audit added per Q2 clarification (US2b / FR-002a) is
  documented in the PR body — either as a confirmed non-regression
  (existing code path correct) or as an inline fix (mirroring
  US1 / US2). No pre-existing GH issue exists for rpm; the m197 PR
  is authoritative.
- **SC-002**: Any deb / apk / rpm scan against an epoch-versioned
  package produces PURLs that pass a purl-spec canonicalization
  round-trip (parse → re-serialize → byte-compare) without loss.
  Includes rpm per the Q2 clarification (US2b non-regression audit) —
  the fixture from T012 verifies rpm alongside the deb + apk fixtures.
- **SC-003**: The fuzz suite exercises ≥ 1100 total synthetic PURL
  inputs (11 ecosystems × ≥ 100 inputs) and completes with zero
  failures.
- **SC-004**: For a monorepo fixture with 2+ sibling manifests
  declaring the same dep, the emitted SBOM shows both source-
  manifest paths and both ranges on the surviving component.
- **SC-005**: For an npm-alias fixture, the emitted SBOM shows
  exactly one component per resolved identity, with a
  `mikebom:declared-as` annotation carrying the original alias.
- **SC-006**: Pre-m197 goldens for non-epoch, versioned,
  non-alias scans remain byte-identical, WITH the one Q1-
  clarification exception: goldens exercising the m191 reconciler
  survivor code path drift due to the singular-→-always-array
  shape rotation. Every affected golden is regenerated in the same
  PR; diff review confirms the drift is exclusively the shape
  rotation, not other structural change.
- **SC-007**: `./scripts/pre-pr.sh` completes with wall-clock delta
  ≤ 5s vs pre-m197 baseline (matches m195/m196 SC-006 threshold).

## Assumptions

- **The `?epoch=` qualifier form is what downstream consumers
  expect**: purl-spec defines `?epoch=` explicitly for the deb /
  rpm / apk / opkg ecosystems; osv.dev + deps.dev + Anchore Grype
  key off it. Adopting the qualifier form is the correct fix, not
  a stylistic preference.
- **The 6 additional ecosystems in FR-003 have equivalent
  versionless-PURL grammar to the m191 primary 5**: composer / dart
  / cocoapods / scala / haskell / erlang all follow the purl-spec
  canonical rule (no trailing `@` when version absent). No
  ecosystem-specific weirdness expected; the fix is symmetric to
  the m191 pattern.
- **Fuzz-test generator complexity is bounded**: US4's fuzz suite
  targets 100 inputs per ecosystem via a small hand-rolled generator
  (not property-based `proptest` if that adds a new Cargo dep — see
  Plan phase decision). Corner cases are catalog-driven, not
  exhaustive-space-exploring.
- **npm alias `npm:pkg@spec` grammar is stable**: per npm docs the
  `npm:` prefix is the canonical alias marker. Yarn / pnpm follow
  the same syntax. Non-alias declarations don't collide with this
  prefix per npm's own registry-name reservation.
- **Multi-declaration survivor arrays are unbounded but reasonable**:
  no cap enforced this milestone. Real-world monorepos rarely have
  > 20 sibling declarations of the same dep; a cap would be
  premature optimization.
- **Pre-existing test fixtures cover the FR-007 additive-only
  guarantee**: existing byte-identity goldens for non-edge-case scans
  form the regression tripwire. No new "prove non-regression"
  fixtures needed.
- **All 6 issues can land in one PR without becoming a review
  nightmare**: bundling assumes reviewers can digest ~500-1000 LOC
  across 6 targeted changes. If review-time signals otherwise, a
  follow-up spec can split the bundle.
