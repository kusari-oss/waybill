# Feature Specification: Nix closure scope from the runtime closure

**Feature Branch**: `1423-nix-runtime-scope`
**Created**: 2026-11-10
**Status**: Draft
**Input**: User description: "For --nix-closure scans, derive component scope from the evaluated output's runtime closure, not the build-graph classifier. Closes #1423. Measured 2026-11-10 (main@f891b4fe, nixpkgs a799d3e3, grype 0.120.0, 5 targets): 951 findings sit on components outside the runtime closure, and only 175 of them are on components marked scope:excluded; grype/Trivy/osv-scanner ignore CDX scope. Scope taken from `nix path-info -r` (~1 s, no build, local store or cache.nixos.org) gives 350 findings, covering all of sbomnix runtime's 243. Fallback when the output isn't available: a root-reachability walk (95.7% precision / 94.6% recall on findings). Record the method used in a document-scope annotation. The three formats must agree on scope: today CDX marks non-artifact members scope:excluded with no lifecycle scope, which `sbom edit` reads as development, while SPDX 2.3/3 do not, so on moat `--drop scope=development,build,test` selects 1,320 components in CDX and 11 in SPDX (found in #1428). Ship a documented path to a runtime-only document (an `sbom edit` recipe or a scan option), since scanners don't honour scope. Constraints: zero new Cargo deps, no network beyond --nix-closure's existing reach, every number traceable to a committed probe."

Every figure below is from `measurements/README.md` unless it says otherwise.

## Background

A `--nix-closure` document lists the full build closure of the evaluated package: the package itself, everything linked into it, and every compiler, build tool and script used along the way. A vulnerability-management user mostly wants to know what ships. sbomnix gives them that as a separate runtime SBOM.

waybill already marks members that are not in the artifact. CycloneDX gets `scope: excluded`; SPDX 2.3 and SPDX 3 get `BUILD_TOOL_OF`. The marks come from the milestone-1277 role classifier. Measured on five targets:

- **The marks miss most build-only components.** 951 grype findings sit on components outside the runtime closure. Only 175 of them are on marked components; the other 776 are on components the document leaves looking like runtime. The classifier's role is a property of the whole build graph: a dependency is an `artifact-input` if *any* derivation in the closure links it, not only the one being scanned. That is why curl's closure presents python3, libxml2 and pcre2 as runtime.
- **No scanner reads the marks.** grype, Trivy and osv-scanner all ignore CycloneDX `scope` (#1423). As a findings filter, today's marks score 31.1% precision.
- **Nix already knows the answer.** `nix path-info -r` returns the exact runtime closure of a built output in 0–1 s, without building anything. Marking scope from it leaves 350 findings across the five targets, a superset of the 243 that sbomnix's runtime SBOM yields.

## Clarifications

### Session 2026-11-10

- Q: When the output isn't in the local store, may waybill query remote stores for its runtime closure? → A: Yes. Query the substituters the user's nix is already configured to trust (option B), never ones waybill adds; `--offline` suppresses it.
- Q: What form does the runtime-only step take? → A: A documented `sbom edit` recipe (option A), with no new scan flag.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Scope reflects what ships when the output is available (Priority: P1)

A user scans a flake whose package is built locally, or is available from a substituter the user's nix already trusts. Every component outside the package's runtime closure is marked not-in-artifact. Every component inside it, including Go modules and Rust crates compiled into a runtime binary, is not marked. The document says the marks came from the runtime closure.

**Why this priority**: This is the change that makes scope true. Everything else builds on it. It turns a 31%-precision mark into an exact one on the targets measured.

**Independent Test**: Scan each of the five wrapper flakes with the output in the local store, then compare the unmarked set with the components matched from `nix path-info -r` (the `runtime.py` matching rule). Both directions must agree: no runtime component marked, no non-runtime component left unmarked.

**Acceptance Scenarios**:

1. **Given** curl's flake with its output in the local store, **When** it is scanned with `--nix-closure`, **Then** python3, libxml2 and pcre2 are marked not-in-artifact in all three formats, and curl, openssl, nghttp2, libssh2, zlib and brotli are not.
2. **Given** gh's flake, **When** it is scanned, **Then** the Go modules compiled into the gh binary are not marked, because their host binary is in the runtime closure.
3. **Given** openssh's flake, **When** it is scanned, **Then** `dns-root-data`, which the role classifier calls `build-tooling` but which is in openssh's runtime closure, is not marked.
4. **Given** any of the five, **When** it is scanned, **Then** a document-scope annotation records that scope came from the runtime closure, and which store answered the query.

---

### User Story 2 - A fallback that still beats today's marks (Priority: P2)

A user scans a flake whose output isn't available: never built, and not in any cache nix will query. moat is the measured case. Instead of today's build-graph marks, waybill walks from the evaluated package over runtime-linking edges and marks everything the walk doesn't reach. The document says the marks are an approximation and names the method.

**Why this priority**: Haskell flakes are the motivating case for this whole effort, and moat's output was the one not cached. Without a fallback, the scans that most need better marks would get none of the improvement.

**Independent Test**: Run the walk on the five targets with the store query disabled, then score it against `rt/<t>.paths`. Finding-level precision and recall must both be at or above the committed figures (95.7% / 94.6%).

**Acceptance Scenarios**:

1. **Given** moat's flake with its output in neither the local store nor the cache, **When** it is scanned, **Then** scope comes from the walk and the method annotation says so.
2. **Given** a target where the walk misses runtime members that carry no findings (the darwin SDK derivations on httpie), **When** it is scanned, **Then** the method annotation tells the reader the marks are an approximation. The document must not present walk-derived marks as exact.

---

### User Story 3 - One answer to "what is runtime" in all three formats (Priority: P2)

A user who filters a closure document gets the same set whichever format they started from. Today `sbom edit --drop 'scope=development,build,test'` selects 1,320 components in moat's CycloneDX and 11 in its SPDX 2.3 and SPDX 3 (found during #1428). The documents carry the same information; `sbom edit` reads the not-in-artifact mark as `development` in CycloneDX and doesn't read SPDX's `BUILD_TOOL_OF` at all.

**Why this priority**: A recipe that gives three answers for one scan isn't a recipe.

**Independent Test**: For each of the six targets, apply the same not-in-artifact selection to all three formats and compare the selected PURL sets. They must be equal.

**Acceptance Scenarios**:

1. **Given** moat's three documents, **When** the not-in-artifact selection is applied to each, **Then** the selected sets are identical.
2. **Given** a non-Nix scan whose components carry real lifecycle scopes (an npm project with dev dependencies), **When** `scope=development` is applied, **Then** the selection is unchanged from today. Not-in-artifact and development must not be confused in either direction.

---

### User Story 4 - A runtime-only document a scanner can use (Priority: P3)

A user who wants runtime-only scanner results follows one documented step and gets a document from which every not-in-artifact component has been removed. grype, Trivy and osv-scanner then report only what ships.

**Why this priority**: Scanners don't honour scope, so marks alone don't change scanner results. It's P3 because once US1–US3 hold, this is a recipe and its documentation.

**Independent Test**: Follow the documented step on the five targets and run grype. The finding count must equal the runtime-truth count (350 in total), and no finding may be lost relative to the runtime-truth document.

**Acceptance Scenarios**:

1. **Given** a scanned closure document, **When** the documented runtime-only step is applied, **Then** the result is a valid document in its format, all relationships resolve, and no not-in-artifact component remains.
2. **Given** moat's CycloneDX, which carries VEX statements about build-only components, **When** the step is applied, **Then** it succeeds. This depends on #1428.

### Edge Cases

- **Output partly present.** For a multi-output package (`out`, `lib`, `dev`), use the outputs the evaluated attribute selects; a runtime path present in another output still counts.
- **The store query fails or times out** (nix daemon down, cache unreachable, offline). Fall back to the walk (US2), record why, and don't fail the scan.
- **`--offline`.** Must not query any remote store. The local store is still used.
- **The query succeeds but some runtime paths match no component.** Darwin's `libSystem` and `libiconv`, for example, have no closure component, so there is nothing to mark. Count them and report the count; don't invent components for them.
- **A component matches a runtime path by name but not by version.** It is not runtime. Report the version mismatch rather than guessing.
- **Exempt members.** The evaluated package (`exempt-evaluated-package`) and compiler-supplied boot libraries (`exempt-compiler-supplied`) keep their exemptions; nothing here may mark them.
- **Members whose scope a lifecycle pass already set** (the milestone-1269 test-dependency propagation, e.g. `hspec`). Runtime-closure truth wins: a test dependency outside the runtime closure stays marked, and one inside it is unmarked.
- **Scans without `--nix-closure`.** Nothing changes.

## Requirements *(mandatory)*

### Functional Requirements

**Deriving scope**

- **FR-001**: For a `--nix-closure` scan whose evaluated output's runtime closure can be obtained, waybill MUST mark exactly the components outside that closure as not-in-artifact, and MUST NOT mark any component inside it. This holds in both directions: a component today's classifier marks but the runtime closure contains MUST lose the mark.
- **FR-002**: A component compiled into a runtime member (Go modules and Rust crates read from a runtime binary, milestone 1378) MUST inherit that member's runtime status.
- **FR-003**: Matching a runtime store path to a component MUST use derivation identity (the `.drv` that produced the path) where the closure records it. Name-and-version matching MAY be used only where derivation identity is unavailable, and every path left unmatched MUST be counted in the method annotation (FR-007).
- **FR-004**: The runtime closure MUST be obtained without building anything. It MUST be read from the local store first. If the output isn't there, waybill MUST query the substituters the user's nix is already configured to trust, in nix's configured order, and use the first complete answer. It MUST NOT add substituters of its own or override the user's trust settings.
- **FR-005**: When the runtime closure cannot be obtained, waybill MUST derive scope from a reachability walk: from the evaluated package over runtime-linking edges only, marking every member the walk does not reach. It MUST NOT fall back to today's classifier marks.
- **FR-006**: The exemptions from milestones 1276 and 1377 MUST be preserved. No method may mark an exempt member.

**Recording the method**

- **FR-007**: Every `--nix-closure` document MUST record, at document scope in all three formats:
  - the method that produced its scope marks: `runtime-closure`, `reachability-walk`, or `none` when the closure tier degraded;
  - for `runtime-closure`, which store answered;
  - for `reachability-walk`, why the runtime closure was unavailable;
  - the count of runtime paths that matched no component.

  Before adding a `waybill:*` row for this, the plan MUST audit CycloneDX, SPDX 2.3 and SPDX 3 for a native document-scope construct (Principle V), and cite the result.
- **FR-008**: The per-component C203 `waybill:scope-classification-source` MUST name the method that decided each closure member's mark. Its closed enum (today `closure-role`, `closure-role+lifecycle-propagation`, and the two exemptions) gains values for the runtime closure and the walk. A component's C203 value and its native marks MUST never disagree, and invariant I10 MUST accept the new values.

**Format agreement**

- **FR-009**: The not-in-artifact mark MUST be expressed natively in each format: CycloneDX `scope: excluded`, SPDX 2.3 `BUILD_TOOL_OF`, and the SPDX 3 equivalent emitted today. The three formats MUST mark the same set of components.
- **FR-010**: `sbom edit` MUST select not-in-artifact components the same way in all three formats, and MUST NOT read a not-in-artifact mark as `development`, `build` or `test`. The selector vocabulary is decided in planning (a new `scope=` value, or a separate selector key). Existing `scope=` selections on documents with real lifecycle scopes MUST select the same components as before.

**Runtime-only output**

- **FR-011**: The user guide MUST document an `sbom edit` recipe that turns a closure document into a runtime-only document. It uses the not-in-artifact selection from FR-010 and works unchanged on all three formats. This feature adds no new scan flag.
- **FR-012**: The runtime-only result MUST be valid against its format's schema, MUST keep the evaluated package as its subject, and MUST keep VEX statements about the components that remain.

**Constraints**

- **FR-013**: No new Cargo dependencies.
- **FR-014**: Network access is limited to `--nix-closure`'s existing reach plus the substituter query in FR-004, which goes only to caches the user's nix already trusts. `--offline` MUST suppress every remote query, the substituter query included.
- **FR-015**: Every number this feature's documentation states about scope or findings MUST trace to a probe committed under `specs/1423-nix-runtime-scope/measurements/`.

### Key Entities

- **Runtime closure**: the set of store paths the evaluated output references at runtime, as nix reports it. Exact by construction when available.
- **Scope method**: which of `runtime-closure`, `reachability-walk` or `none` produced a document's marks, with the store that answered (or why none did) and the count of unmatched runtime paths.
- **Not-in-artifact mark**: the per-component assertion, expressed natively in each format, that a closure member does not ship. Paired with C203.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the five wrapper flakes with outputs available, grype run on the runtime-only result reports exactly the 350 findings the committed runtime truth gives. That is no finding lost and none added, down from 1,301 for the full document.
- **SC-002**: Those findings include all 243 that sbomnix's runtime SBOM yields for the same targets.
- **SC-003**: With the store query disabled, the walk's finding-level precision and recall on the five targets are each ≥ 95.7% and ≥ 94.6%. These are the committed measurements, used as a floor rather than a target.
- **SC-004**: On all six targets, the not-in-artifact selection picks identical PURL sets in CycloneDX, SPDX 2.3 and SPDX 3. Today on moat it picks 1,320 vs 11.
- **SC-005**: Obtaining the runtime closure adds at most 2 s of wall time per scan on the measured targets. The committed measurement is 0–1 s per target; the bound allows for variance and gets re-measured in planning.
- **SC-006**: On non-Nix corpus targets, every `scope=` selection result and every emitted document is unchanged.

## Assumptions

- **One document.** waybill keeps emitting one document that carries every closure member with marks; it does not split into runtime and buildtime documents the way sbomnix does. The runtime-only view is derived from it (FR-011).
- **Runtime-linking edges for the walk.** These are the derivation's own runtime dependency fields, the ones milestone 1280 projects into dependency edges. The plan will pin down the exact field set against the committed `reach.py`.
- **Embedded components.** Go modules and Rust crates compiled into a binary come from milestone 1378's closure-contents reading. The Linux CI moat golden reads 0 member outputs (1,086 absent, 503 no-outputs), so embedded inheritance is tested on the local wrapper flakes rather than on that golden.
- **The corpus golden takes the walk.** moat's output is in neither store on Linux CI, so its golden exercises the walk (US2), not the runtime closure.
- **#1428 lands first.** It fixes `sbom edit`'s handling of VEX on dropped components and its `role=` selector; US4's recipe depends on it.
- **Out of scope.** Changing what CycloneDX `scope` means for non-Nix scans; vulnerability matching inside waybill. Vulnerabilities belong in VEX, not in the SBOM's component list.
