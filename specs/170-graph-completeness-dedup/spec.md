# Feature Specification: Dedup document-scope `mikebom:graph-completeness` annotation

**Feature Branch**: `170-graph-completeness-dedup`
**Created**: 2026-07-06
**Status**: Draft
**Input**: User description: "Dedup document-scope mikebom:graph-completeness annotation — currently emitted twice on any scan with a Go component (once from m061 C44 site, once from m158 C104 site)"

## Background

mikebom emits a document-scope `mikebom:graph-completeness` annotation to communicate how complete the SBOM's dependency graph is. Two independent code paths currently write this same annotation key to the CDX metadata properties array, resulting in **duplicate emission on any scan that includes a Go component**.

**Reproduction (byte-verified against `mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json`)**:

```json
"properties": [
  { "name": "mikebom:graph-completeness", "value": "partial" },       // Site 1 (m061 / C44)
  { "name": "mikebom:trace-integrity-…", "value": "0" },
  { "name": "mikebom:graph-completeness", "value": "partial" },       // Site 2 (m158 / C104)
  { "name": "mikebom:graph-completeness-reason", "value": "orphaned-components-detected: 1 component(s) not reachable from root" }
]
```

The two emission sites carry semantically distinct signals wearing the same annotation key:

- **Site 1** (catalog row **C44**, milestone 061 closes #119): Go-scoped graph-completeness — "did we resolve every `go.sum` transitive edge?" Fires conditionally when a Go scan produced a `go_graph_completeness: Option<GraphCompleteness>` result. There are **three parallel emission points** in Site 1 — one per output format: CDX at `mikebom-cli/src/generate/cyclonedx/metadata.rs:228-245`, SPDX 2.3 at `mikebom-cli/src/generate/spdx/annotations.rs:546-567`, and SPDX 3 at `mikebom-cli/src/generate/spdx/v3_annotations.rs:524-539`. All three must be removed in lockstep to keep emission across the three formats consistent.
- **Site 2** (catalog row **C104**, milestone 158): Universal always-emit graph-completeness — "is every component in this SBOM reachable from a root?" Fires on every scan (multi-root BFS orphaned-components check). Same three-per-format shape — CDX at `mikebom-cli/src/generate/cyclonedx/metadata.rs:471-491`, SPDX 2.3 + SPDX 3 at analogous positions in their respective annotations files. These emissions STAY as the sole surviving `mikebom:graph-completeness` carriers.

Milestone 160 subsequently introduced `mikebom:go-transitive-coverage` (catalog row **C110**) with the same three-value enum (`complete|partial|unknown`) plus a richer reason-code vocabulary — that annotation is now the canonical home for the Go-specific completeness signal. Site 1's C44 emission is dead weight from milestone 061 that milestone 160 obsoleted; it was never retired.

The parity-extractor table at `mikebom-cli/src/parity/extractors/mod.rs:256` and `:451` even declares BOTH catalog rows with identical `label = "mikebom:graph-completeness"` — the duplicate emission has silently propagated through five subsequent milestones without triggering the m071 catalog integrity gate.

**Impact on emission formats**:

- **CDX 1.6**: two `properties[]` entries with the same `name` — schema-legal but semantically ambiguous.
- **SPDX 2.3**: two `annotations[]` entries with the same field via envelope decoding — same ambiguity.
- **SPDX 3.0.1**: two graph-element `Annotation` elements on the SpdxDocument root IRI carrying identical `statement.field` — same ambiguity.

**Consumer harm** (concrete):

- A consumer's `jq '.properties[] | select(.name == "mikebom:graph-completeness") | .value'` returns TWO values with no ordering guarantee. There is no defined winner. `docs/reference/reading-a-mikebom-sbom.md` (§3.3) presents this signal as singular; consumer code following the guide breaks in undefined ways.
- Policy tools consuming graph-completeness as a filter (e.g., "downgrade CVE severity when graph is incomplete") non-deterministically pick which of the two values wins.

## Clarifications

### Session 2026-07-06

- Q: When C44 is removed, does its emission's information need to be re-homed (given that Site 1 and C110 currently produce divergent values in the same scan)? → A: **Option A — pure removal**. Accept the semantic loss. C110 is the modern canonical home even if its current `unknown` verdict is coarser than C44's `partial`. Track the follow-up as a separate investigation: **[issue #516](https://github.com/kusari-oss/mikebom/issues/516)** — determine whether the C44 information is reconstructible from remaining signals (universal `mikebom:graph-completeness` + per-component `mikebom:source-type` + PURL scheme), and if not, file a follow-up milestone to migrate the m061 computation into C110's decision logic (or introduce a new distinct key). Not urgent — post-m170 state is strictly better than pre-m170 state regardless of the outcome.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Duplicate emission eliminated (Priority: P1)

An operator scans a Go project (or any polyglot project with at least one Go component) and inspects the emitted SBOM. The document-scope `mikebom:graph-completeness` annotation appears **exactly once** in each of the three output formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1). The single emission carries the milestone-158 universal semantic ("is every component reachable from a root?"), matching how the docs already describe it.

**Why this priority**: This is the core defect. Every downstream consumer breaks in undefined ways until fixed. No workaround exists — the emission logic is authoritative.

**Independent Test**: Emit a CDX SBOM for the existing `tests/fixtures/golden/cyclonedx/golang.cdx.json` fixture's source input. Assert that `.properties[] | select(.name == "mikebom:graph-completeness") | length == 1`. Repeat for SPDX 2.3 (`.annotations[]` envelope decoded) and SPDX 3.0.1 (`@graph[]` typed Annotation elements).

**Acceptance Scenarios**:

1. **Given** a scan target with Go components AND at least one orphaned component in the resolved graph, **When** the operator emits CDX 1.6 output, **Then** the document metadata's `properties[]` array contains exactly one `{name: "mikebom:graph-completeness", value: <enum>}` entry.
2. **Given** the same scan target, **When** the operator emits SPDX 2.3 output, **Then** the document annotations decode to exactly one `mikebom:graph-completeness` field.
3. **Given** the same scan target, **When** the operator emits SPDX 3.0.1 output, **Then** exactly one graph-element `Annotation` targeting the SpdxDocument carries `statement.field == "mikebom:graph-completeness"`.
4. **Given** a scan target WITHOUT any Go components (e.g., a pure npm project), **When** the operator emits any of the three formats, **Then** the annotation still emits exactly once (universal always-emit semantic preserved from milestone 158).

---

### User Story 2 — Go-specific signal preserved via C110 (Priority: P1)

An operator scanning a Go project still needs the Go-specific "did we resolve every `go.sum` transitive edge?" signal — the same information the milestone-061 C44 site was communicating. That signal remains available via the milestone-160 `mikebom:go-transitive-coverage` (C110) annotation, which is the modern canonical home.

**Why this priority**: Co-P1 with US1. Removing C44 without preserving its semantic would regress transparency (Constitution Principle X). C110 has to already carry the signal — the fix is verifying the semantic transfer is complete, not adding new emission.

**Independent Test**: Emit CDX for a Go scan where `go mod graph` degrades (produce the m160 `go-mod-graph-degraded` reason code). Assert `mikebom:go-transitive-coverage` is present with value `partial` and `mikebom:go-transitive-coverage-reason` carries the reason string. Assert that pre-170 mikebom would have emitted the same information via the (now-removed) C44 site.

**Acceptance Scenarios**:

1. **Given** a Go scan where at least one module ended `Unresolved` per the milestone-055 ladder, **When** the operator emits any format, **Then** `mikebom:go-transitive-coverage` is present with value `partial` (or `unknown` per the m160 reason-code decision rules).
2. **Given** a Go scan where every module resolved cleanly, **When** the operator emits any format, **Then** `mikebom:go-transitive-coverage` is present with value `complete`.
3. **Given** a scan target with NO Go components, **When** the operator emits any format, **Then** `mikebom:go-transitive-coverage` is absent (matches milestone-160 FR-005 emission gating).

---

### User Story 3 — Catalog integrity gate closed (Priority: P2)

A future contributor extending the parity-extractor table cannot accidentally introduce two catalog rows with the same annotation label without a CI failure surfacing the duplicate before merge.

**Why this priority**: Refinement — closes the mechanism gap that let C44 and C104 co-exist for five milestones. Not a P1 because the P1 fix (removing C44) already resolves the immediate consumer breakage; this is prevention-of-recurrence.

**Independent Test**: Add a unit test in `mikebom-cli/src/parity/extractors/mod.rs::tests` asserting that every distinct `label` appears in exactly one `ParityExtractor` entry. Verify test fails on a synthesized two-row-same-label scenario and passes on the current table.

**Acceptance Scenarios**:

1. **Given** the current parity-extractor table with C44 removed, **When** the test runs, **Then** it passes.
2. **Given** a hypothetical PR that reintroduces two rows with `label = "mikebom:foo"`, **When** the test runs, **Then** it fails with a message naming the duplicate label and both row IDs.

---

### Edge Cases

- **Trace mode scans** (`mikebom trace`): the m061 emission site fires in build-trace scans too. Removal must not regress the trace-attestation emission path. Verified by re-running the milestone-001 build-trace golden.
- **Trivially-complete graphs** (`GraphCompletenessResult::trivially_complete()`): the universal m158 site emits `value = "complete"` with no reason codes. Removal of C44 must NOT alter this value on Go scans that happen to have a complete graph.
- **The catalog integrity gate MUST be tolerant of intentional presence-only rows** (like `E1 ecosystem completeness` which reuses the CDX-native `compositions[]` shape). If the parity-extractor system has legitimate cases where two rows share a label, the gate becomes a list of known-allowed duplicates rather than an absolute rule. Investigation during planning will confirm whether any such cases exist beyond the C44/C104 pair; if none, the rule is absolute.
- **Non-CDX golden fixtures**: SPDX 2.3 and SPDX 3.0.1 goldens for Go inputs also carry the duplicate emission and must be regenerated in lockstep.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `mikebom:graph-completeness` document-scope annotation MUST be emitted exactly once per SBOM document in each of CDX 1.6, SPDX 2.3, and SPDX 3.0.1 output formats.
- **FR-002**: The single emission's semantic MUST match the milestone-158 universal signal ("is every component in this SBOM reachable from a root?") — the semantic the docs already describe under §3.3 of the reading guide + row C4 of the mapping doc.
- **FR-003**: The Go-specific "did we resolve every `go.sum` transitive edge?" signal MUST remain available to consumers via `mikebom:go-transitive-coverage` (catalog row C110, milestone 160). This is a NO-CODE-CHANGE requirement — C110's emission logic already exists.
- **FR-004**: The parity-extractor table MUST not contain two rows sharing the same `label` value. A CI-gating unit test MUST enforce this invariant.
- **FR-005**: Golden fixtures containing the duplicate emission (`tests/fixtures/golden/cyclonedx/golang.cdx.json` and any SPDX counterparts in the sibling `mikebom-test-fixtures` repo) MUST be regenerated in lockstep with the code change. The golden diff MUST show only the removal of the duplicated annotation entry — no other byte-changes.
- **FR-006**: Post-fix, `docs/reference/sbom-format-mapping.md` MUST reflect the retirement of catalog row C44. The row is either (a) removed entirely with a note in the docs archive; or (b) marked `~~C44~~` (strikethrough) with a "REMOVED in milestone 170" annotation preserving the historical record (matches the C6 precedent already in the file).
- **FR-007**: The retirement of C44 MUST NOT break any consumer who was already reading `mikebom:graph-completeness` — because the single surviving emission (from C104) carries the same value space (`complete|partial|unknown`) at the same location. Only the redundant duplicate goes away.
- **FR-008**: Pre-PR gate (`./scripts/pre-pr.sh`) MUST pass, including the m071 parity gate that surfaced C116 during milestone 169. If FR-004's new duplicate-label test fails on the current table (as it should, since C44 and C104 currently share `label = "mikebom:graph-completeness"`), the fix MUST land the code change AND the test simultaneously.

### Key Entities

- **`mikebom:graph-completeness` (document-scope annotation)**: Three-value enum `complete|partial|unknown` communicating multi-root BFS reachability of the SBOM graph. Present on every emitted SBOM regardless of ecosystem. This entity is what US1 dedups.
- **`mikebom:go-transitive-coverage` (document-scope annotation, C110)**: Three-value enum `complete|partial|unknown` communicating Go-specific transitive-edge resolution status. Present only when the scan includes ≥1 Go component. This entity carries what the removed C44 site was communicating.
- **Catalog row (in `docs/reference/sbom-format-mapping.md` + `mikebom-cli/src/parity/extractors/mod.rs`)**: A pairing of a `label` (annotation name) with an extractor triple for CDX/SPDX 2.3/SPDX 3.0.1. Every `label` MUST be unique across the table post-170.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For every emitted CDX 1.6 SBOM, `jq '[.metadata.properties[] | select(.name == "mikebom:graph-completeness")] | length'` returns exactly `1`.
- **SC-002**: For every emitted SPDX 2.3 SBOM, decoding `.annotations[]` envelopes yields exactly one entry with `field == "mikebom:graph-completeness"`.
- **SC-003**: For every emitted SPDX 3.0.1 SBOM, exactly one `@graph[]` element with `type == "Annotation"` and `statement.field == "mikebom:graph-completeness"` targets the SpdxDocument root IRI.
- **SC-004**: The m071 parity-catalog integrity gate passes after the C44 row is retired and the m170 duplicate-label test is added.
- **SC-005a** (main repo diff): `git diff main -- 'mikebom-cli/tests/fixtures/golden/**'` on the m170 branch shows ONLY the retirement of the duplicate `mikebom:graph-completeness` entry in `tests/fixtures/golden/cyclonedx/golang.cdx.json`. No unrelated byte-changes on any local golden.
- **SC-005b** (sibling repo diff): `git diff main -- 'tests/fixtures/{spdx,spdx3}/**'` in the companion `mikebom-test-fixtures` repo shows ONLY the retirement of the duplicate `mikebom:graph-completeness` annotation envelope (SPDX 2.3) and typed Annotation element (SPDX 3) in the Go-ecosystem goldens. No unrelated byte-changes on any sibling-repo golden.
- **SC-006**: Pre-PR gate (`./scripts/pre-pr.sh`) passes green — including the m071 catalog gate, the m138+ integration tests, and the new m170 duplicate-label unit test.
- **SC-007**: Post-fix, a consumer running the exact jq recipe from `docs/reference/reading-a-mikebom-sbom.md` §3.3 (`mikebom:graph-completeness`) against any m170-or-later CDX SBOM receives a single value (no ambiguity).
- **SC-008**: No regression: for every ecosystem golden in the m090 sibling-repo test suite (apk, cargo, deb, gem, go, maven, npm, pip, rpm), byte-identity is preserved outside the targeted duplicate-removal deltas.

## Assumptions

- The universal m158 `mikebom:graph-completeness` emission (Site 2 at `metadata.rs:471-491`) is the canonical home. The m061 Go-scoped C44 emission (Site 1) is dead weight from a milestone that m160 obsoleted. This assumption is verifiable by inspecting the two sites' semantics against the reading guide's §3.3 documentation — the reading guide describes Site 2's universal semantic, not Site 1's Go-specific one.
- The C110 `mikebom:go-transitive-coverage` annotation is the modern canonical home for the Go-specific "transitive edges resolved?" signal. Fully specified in milestone 160.
- No consumer relies on the specific position (index) of the duplicate `mikebom:graph-completeness` entries in the properties array. Consumers filter by `name`, not by index.
- The parity-extractor duplicate-label gate can be a hard rule (no allowlist needed). Investigation during planning will confirm this by scanning the current EXTRACTORS table for any other `label` collisions beyond C44/C104; if the C44/C104 pair is the only collision, the rule is absolute. If other legitimate cases exist (e.g., presence-only rows sharing a nominal label), the gate becomes a small allowlist. **This is a planning-phase confirmation, not a spec-phase ambiguity — reasonable default is "absolute rule + tiny allowlist if planning surfaces legitimate cases."**
- Golden regeneration is the well-established `MIKEBOM_UPDATE_GOLDENS=1 cargo test` workflow already in use for milestones 010, 090, 119, 134, 158.
- The change scope is user-space Rust only. `mikebom-ebpf` is untouched. No new Cargo dependencies. No new subprocess calls. No new network access.
- The milestone-078 SPDX 3 conformance gate (`spdx3-validate` pinned to 0.0.5) continues to pass — retiring one duplicate annotation cannot introduce a validator violation.
