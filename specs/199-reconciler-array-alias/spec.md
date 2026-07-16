# Feature Specification: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Feature Branch**: `199-reconciler-array-alias`
**Created**: 2026-07-15
**Status**: Draft
**Input**: User description: "pr-d" (m197 US5 + US6 carved out as a standalone milestone — the reconciler slice deferred from m197's split-PR delivery)

## Overview

Milestone 197 shipped as four PRs (epoch fix, versionless-PURL extension,
fuzz test, and this deferred reconciler slice). PR-A / PR-B / PR-C landed
in main during m197 → m198 execution; this milestone is the fourth slice
landing as a standalone spec-driven delivery. Two m197 user stories
combined:

- **US6 (m197)**: rotate the m191 reconciler's declaration-provenance
  annotations from singular scalars (`mikebom:requirement-range`,
  `mikebom:source-manifest`) to always-array shape
  (`mikebom:requirement-ranges`, `mikebom:source-manifests`) — uniform
  across single-vs-multi-declaration cases per m197 Q1 clarification.
  Closes #565.
- **US5 (m197)**: teach the m191 reconciler to recognize npm-alias
  declarations (`"my-alias": "npm:actual-pkg@1.0.0"`), match design-tier ↔
  source-tier by resolved identity (not alias name), and preserve the
  original alias name(s) as a new `mikebom:declared-as` annotation on
  the survivor. Closes #564.

These land together as PR-D because both touch `mikebom-cli/src/resolve/
reconciler.rs`, and US5 uses the array-emission pattern US6 establishes.
Also bundles the m197 Q1 exception: the singular-→-array rotation
requires regenerating existing goldens that exercise the m191 reconciler
survivor code path.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reconciler survivor emits declaration-provenance as uniform arrays (Priority: P1)

An SBOM consumer looking at a component reconciled by m191 sees a
uniform annotation shape whether one manifest or twenty manifests
declared the underlying dep — `mikebom:requirement-ranges` and
`mikebom:source-manifests` are always JSON arrays. No conditional
key-name parsing based on declaration count; no lookup fork like "check
singular first, fall back to plural." Consumers can key on `.length`
to distinguish single-vs-multi cases.

**Why this priority**: The m191 shipped shape was inconsistent (single
scalar for the one-manifest case, plural array only when the multi-
manifest code path fired — which shipped only in an unreleased m197
draft, never in a merged m191 PR). Post-m199 the shape is stable and
uniform, which unblocks downstream provenance-consumer contracts.
Closes #565.

**Independent Test**: Fixture with a monorepo declaring the same dep
from 2+ sibling manifests → survivor carries both ranges + both
manifest paths as arrays. Separate fixture with one manifest → survivor
carries 1-element arrays (NOT scalars).

**Acceptance Scenarios**:

1. **Given** a monorepo with `packages/foo/package.json` declaring
   `commander: "^11.0"` and `packages/bar/package.json` declaring
   `commander: "^11.1.0"`, root lockfile resolving both to
   `commander@11.1.0`,
   **When** mikebom emits the SBOM,
   **Then** the survivor `pkg:npm/commander@11.1.0` component carries
   `mikebom:requirement-ranges: ["^11.0", "^11.1.0"]` (2-element array,
   ordered lex-by-manifest) AND `mikebom:source-manifests:
   ["packages/bar/package.json", "packages/foo/package.json"]`
   (2-element array).

2. **Given** a single-manifest project (no monorepo),
   **When** mikebom emits the SBOM,
   **Then** the survivor's annotations use the array shape:
   `mikebom:requirement-ranges: ["^1.0.0"]` (1-element) and
   `mikebom:source-manifests: ["package.json"]` (1-element). The
   singular scalars `mikebom:requirement-range` /
   `mikebom:source-manifest` do NOT appear anywhere in the emitted
   SBOM.

3. **Given** the array shape is uniform across all three emitted
   formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1),
   **When** a consumer parses each format,
   **Then** the wire representation of the annotation is the same
   shape (JSON-encoded array-in-string per contracts/annotation-shapes.md).

---

### User Story 2 - npm-alias declarations reconcile by resolved identity (Priority: P1)

An npm project declares a dep via alias: `"my-alias":
"npm:actual-pkg@1.0.0"`. The design-tier component m191 emits is keyed
on `my-alias` (the alias name); the source-tier component the lockfile
resolves to is keyed on `actual-pkg` (the resolved name). Pre-m199 the
reconciler misses the match (different name keys) and emits two
components — one design-tier `pkg:npm/my-alias` phantom and one
source-tier `pkg:npm/actual-pkg@1.0.0` real component. Post-m199 the
reconciler recognizes the alias syntax, matches by resolved identity,
merges the design-tier hit into the source-tier survivor, and stamps
`mikebom:declared-as: ["my-alias"]` on the survivor for provenance.

**Why this priority**: npm-alias patterns are common in React /
frontend projects (versioned framework migration, monorepo workspace-
alias patterns). Missing the reconciler match produces duplicate
components in the emitted SBOM — the m191 problem class this milestone
is closing. Closes #564.

**Independent Test**: Fixture with a `package.json` declaring
`"my-alias": "npm:actual-pkg@1.0.0"` + a resolving `package-lock.json`.
Post-scan, assert (a) no `pkg:npm/my-alias` phantom component, (b)
exactly one `pkg:npm/actual-pkg@1.0.0` component, (c) that component
carries `mikebom:declared-as: ["my-alias"]`.

**Acceptance Scenarios**:

1. **Given** a `package.json` declaring `"my-alias":
   "npm:actual-pkg@1.0.0"`,
   **When** mikebom emits the SBOM,
   **Then** exactly one `pkg:npm/actual-pkg@1.0.0` component appears
   AND it carries `mikebom:declared-as: ["my-alias"]` (single-element
   array).

2. **Given** a monorepo where two sibling `package.json`s declare the
   same resolved dep via DIFFERENT aliases (`packages/foo` uses
   `"my-alias": "npm:pkg@1"`, `packages/bar` uses `"another-alias":
   "npm:pkg@1"`),
   **When** mikebom emits the SBOM,
   **Then** exactly one `pkg:npm/pkg@1` component appears AND it
   carries `mikebom:declared-as: ["another-alias", "my-alias"]`
   (2-element array, sorted lex, deduped per m197 data-model E1
   validation rules).

3. **Given** a `package.json` with NO alias declarations (regular
   `"pkg": "^1.0.0"` deps only),
   **When** mikebom emits the SBOM,
   **Then** no `mikebom:declared-as` annotation appears on ANY
   component in the emitted SBOM.

---

### Edge Cases

- **US6 array ordering determinism**: `mikebom:source-manifests` sorted
  lex; `mikebom:requirement-ranges` reordered 1:1 with source-manifests
  (Nth range corresponds to Nth manifest). Prevents golden-diff
  churn across reruns.
- **US5 alias + monorepo combined** (US2 acceptance scenario 2): both
  the declared-as array AND the source-manifests array grow correctly
  (each has the right cardinality for its own semantic — declared-as
  deduped, source-manifests preserved-with-duplicates).
- **US5 alias to scoped package**: `"my-alias":
  "npm:@scope/actual@1.0.0"` — resolved identity is `@scope/actual`,
  not `actual`. The alias parser must handle the scoped-name variant.
- **US6 empty-arrays never emitted**: if no design-tier hit contributed
  a range (rare — the reconciler only creates a survivor when at least
  one design-tier match fired), no annotation is emitted (empty array
  is an emission bug per m197 data-model E2/E3 validation rules).
- **US5 without npm reader involvement**: for non-npm ecosystems that
  don't have an alias-declaration syntax (cargo, maven, pip, etc.),
  the reconciler behaves EXACTLY as m191 shipped (no new behavior).
  `mikebom:declared-as` only ever appears on npm-emitted survivors.
- **US5 alias colliding with a real package name**: a `package.json`
  declaring both `"my-alias": "npm:actual@1"` AND `"my-alias": "^2.0"`
  (dep listed twice with different values — technically invalid but
  possible via workflow tooling). Ecosystem last-wins per npm's own
  parser; mikebom follows suit and treats the last-seen declaration
  as authoritative.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The m191 reconciler at `mikebom-cli/src/resolve/reconciler.rs`
  MUST emit `mikebom:requirement-ranges` (JSON array) and
  `mikebom:source-manifests` (JSON array) on every reconciler survivor
  with at least one design-tier match, regardless of single-vs-multi
  declaration count. This supersedes the m191 singular scalars
  `mikebom:requirement-range` / `mikebom:source-manifest` — those
  field names MUST NOT appear on reconciler survivors post-m199 (US6).
- **FR-002**: For multi-declaration cases (N design-tier hits reconciled
  onto one survivor), both arrays MUST have length N (preserving
  every range + every manifest path — no first-wins truncation) (US6).
- **FR-003**: The arrays MUST be sorted deterministically:
  `mikebom:source-manifests` lex-ascending; `mikebom:requirement-ranges`
  reordered 1:1 to match (Nth range corresponds to Nth manifest).
  Ensures golden byte-identity across reruns (US6).
- **FR-004**: The reconciler MUST recognize the npm-alias declaration
  syntax `"<alias>": "npm:<actual>@<version>"` at `mikebom-cli/src/
  scan_fs/package_db/npm/alias_mapping.rs` (extending the m159 pnpm-
  alias handler pattern) and stamp the emitted design-tier component
  with `mikebom:declared-as: [<alias>]` (US5).
- **FR-005**: When a design-tier component carrying `mikebom:declared-as`
  is reconciled onto a source-tier survivor, the reconciler MUST match
  by RESOLVED identity (source-tier PURL, not alias name) and
  accumulate the alias(es) onto the survivor's `mikebom:declared-as`
  as a JSON array. Multi-manifest aliases dedupe + sort lex (US5).
- **FR-006**: `mikebom:declared-as` MUST be emitted ONLY when at least
  one alias was involved in the reconciliation. Components with no
  alias involvement MUST NOT carry the annotation (US5).
- **FR-007**: All 3 emission formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1)
  MUST emit the same wire shape for the 3 new/rotated annotations
  per m197 contracts/annotation-shapes.md. The wire shape is
  JSON-encoded array-in-string inside each format's annotation
  carrier (CDX `properties[].value`, SPDX 2.3 `annotations[].comment`
  JSON-in-string, SPDX 3 `Annotation.statement` JSON-in-string).
- **FR-008**: Existing goldens exercising the m191 reconciler survivor
  code path (identified via grep for `"mikebom:requirement-range"` or
  `"mikebom:source-manifest"` singular scalars) MUST be regenerated
  to reflect the always-array shape. Every other golden byte-
  identically holds per FR-007 additive-only intent.
- **FR-009**: `./scripts/pre-pr.sh` MUST continue to pass green after
  m199 lands.
- **FR-010** (Principle V audit citation): The new `mikebom:declared-as`
  annotation MUST be audited against native CDX 1.6, SPDX 2.3, and
  SPDX 3.0.1 constructs before introduction, and the audit result
  cited here per Constitution Principle V. **Audit result**: no
  native construct across CDX 1.6 (`Component`, `evidence`,
  `properties`), SPDX 2.3 (`Package`, `Annotation`,
  `relationships[]`), or SPDX 3.0.1 (`Annotation`,
  `Software::Package.attributionText`,
  `SoftwareIdentifier`) expresses the alias-vs-resolved-identity
  provenance semantic — an alias name is a source-manifest-local
  mapping, not a package identifier or an evidentiary claim about
  the resolved package. `mikebom:declared-as` is therefore
  Principle-V-compliant. Audit inherited by reference from m197
  plan constitution check (specs/197-purl-reconciler-followups/
  plan.md), restated here to satisfy Principle V's "MUST cite the
  audit result in the spec's Functional Requirements" clause.

### Key Entities

- **`mikebom:requirement-ranges` annotation (rotated from m191 singular)**: JSON
  array of range strings. One entry per design-tier declaration
  reconciled onto the survivor. Preserves duplicates (unlike E1 dedup
  behavior — per m197 data-model contract).
- **`mikebom:source-manifests` annotation (rotated from m191 singular)**: JSON
  array of workspace-relative manifest paths. One entry per design-tier
  declaration reconciled. Sorted lex; ordering 1:1 with
  `mikebom:requirement-ranges`.
- **`mikebom:declared-as` annotation (new)**: JSON array of npm alias
  names as they appeared in source manifests. Sorted lex, deduped
  (same alias declared in multiple manifests → single array entry).
  Emitted only when at least one alias participated in the
  reconciliation.
- **`AliasResolution` (extended)**: Existing type at `mikebom-cli/src/
  scan_fs/package_db/npm/alias_mapping.rs`. Extends to carry the
  raw alias name distinct from the resolved name (or adds a new
  parallel field/method).
- **Reconciler survivor**: The component that remains after m191
  reconciliation. m199 extends its annotation bag with the 3 fields
  above; other survivor fields unchanged.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a monorepo fixture with 2+ sibling manifests
  declaring the same dep, the emitted SBOM shows both manifest paths
  and both ranges on the surviving component as 2-element arrays.
- **SC-002**: For a single-manifest fixture, the survivor carries
  1-element arrays for `mikebom:requirement-ranges` +
  `mikebom:source-manifests`. Zero occurrences of the m191 singular
  scalars anywhere in the emitted SBOM.
- **SC-003**: For an npm-alias fixture, the emitted SBOM shows
  exactly one component per resolved identity + a
  `mikebom:declared-as` annotation carrying the original alias(es)
  as a JSON array.
- **SC-004**: Two consecutive scans of the same fixture produce byte-
  identical arrays (SBOM-level byte-identity — determinism check).
- **SC-005**: Existing pre-m199 goldens are regenerated where they
  exercise the m191 reconciler survivor code path. Diff-review
  confirms every diff is EXCLUSIVELY the singular-→-array shape
  rotation OR the addition of `mikebom:declared-as` where applicable
  — no other class of diff.
- **SC-006**: Follow-up issues #564 and #565 are closed by the m199
  PR via `Closes #564` and `Closes #565` in the commit / PR body.
- **SC-007**: `./scripts/pre-pr.sh` completes with wall-clock delta
  ≤ 5 seconds vs pre-m199 baseline (matches m195-m198 SC threshold
  pattern).

## Assumptions

- **m191 reconciler code path is stable**: this milestone extends the
  existing reconciler transfer logic in-place at `mikebom-cli/src/
  resolve/reconciler.rs`. If mid-implementation a broader reconciler
  refactor is discovered as needed, that becomes a separate follow-up
  milestone and m199 ships the minimal in-place change per Q1
  exception.
- **npm alias parsing is stateless**: the `"npm:<actual>@<ver>"`
  detection is a regex or `str::split_once` against the declaration
  value. No external state (network, cache, config) needed.
- **Non-npm ecosystems don't have alias declarations**: cargo has
  `[dependencies] my-alias = { package = "actual", version = "..." }`
  which is technically an alias, but m199 explicitly scopes to npm.
  Cargo alias handling deferred to a future milestone.
- **Golden regen scope is bounded**: T-shirt-size estimate: 5-15
  existing goldens will be affected. Bounded by a T039-style grep
  audit (identical mechanism to m194 golden regen).
- **`mikebom:declared-as` is a new annotation**: audited against
  native CDX / SPDX constructs in m197 plan constitution check — no
  native alternative exists for alias-provenance semantic; extension
  is Principle-V-compliant.
- **US6 breaks strict m191 byte-identity for reconciler survivors**:
  this is the ONLY FR-008 exception; every other golden holds
  byte-identically per FR-007 spirit. Documented + acknowledged.
