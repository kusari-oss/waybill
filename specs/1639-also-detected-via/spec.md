# Feature Specification: Record which readers found a merged component (C56)

**Feature Branch**: `1639-also-detected-via`
**Created**: 2026-11-21
**Status**: Draft
**Input**: #1558. `waybill:also-detected-via` (C56) is never emitted for a cross-reader merge. Refs #1515, #1442.

## Clarifications

### Session 2026-11-21

- Q: How is the merged component's C55 chosen (FR-003)? → A: Keep today's rule (highest evidence confidence, then reader order) and only record the losers in C56. Milestone 105's precedence module is removed.
- Q: Which merges record C56 (FR-004)? → A: Every merge whose members' C55 values differ, at every merge site: the general PURL deduplicator, spec 1538 R9, the design-source reconciler and binary-to-source attribution.
- Q: What happens to consumers of milestone 108's fingerprint-collision key when FR-007 moves it? → A: Rename outright, with no alias. The old key only ever means source mechanisms, and the CHANGELOG records the rename as a wire change.
- Planning amendment (research R5): C56 is recorded only when the survivor carries a C55 after the merge. The design-source reconciler does not fold annotations, so a source-tier survivor with no C55 absorbing a design-tier member with one would otherwise gain C56 with no C55 beside it, breaking FR-012. FR-001, FR-002 and the edge cases are amended.

## Background

Milestone 105 designed C56. When two readers produce the same canonical PURL, one component is emitted. A deterministic precedence picks its `waybill:source-mechanism` (C55), and C56 records the losing mechanisms. The following was traced in code at `main` `4eec5a4`. None of it has been measured yet; SC-001 does that.

- **Nothing records the losers.**
  - The general PURL deduplicator keeps the member with the highest evidence confidence, and reader order breaks ties.
  - It folds every other member's annotations into the survivor, first value wins, so the losers' C55 values are dropped.
  - Neither component-level CycloneDX evidence call passes a mechanism.
  - The SPDX 2.3 and SPDX 3 emitters have no C56 path.
- **The precedence module is unused.** Milestone 105's precedence module (`scan_fs/dedup.rs`: detection records, a precedence ranking, a dedup function) has no caller outside its own file. Its ranking covers a closed set of 21 mechanisms. Readers emit at least 25 distinct C55 values, counted from string literals at `4eec5a4`, among them `maven-jar-nested`, `pkg-config`, `cmake-package-version`, `dotnet-deps-json` and `pip-requirements-txt`. Values set through constants, such as `git-submodule`, are not in that count, so it is a lower bound.
- **The planned CycloneDX carrier is schema-invalid.** Milestone 105 chose a `waybill-source-mechanism` key on `evidence.identity[0].methods[]`. CycloneDX 1.6 declares `methods[]` items `additionalProperties: false`, allowing only `technique`, `confidence` and `value` (#1442; recorded on the C207 row of `docs/reference/sbom-format-mapping.md`). Wiring C56 as designed would make every merged component's document invalid.
- **The key already means something else.** The symbol-fingerprint matcher (milestone 108 FR-013) writes `waybill:also-detected-via` when one binary's symbols match two libraries, such as OpenSSL and LibreSSL. The value is the other libraries' names, not mechanisms. It is emitted generically, as a CycloneDX property and an SPDX annotation. The C56 CycloneDX extractor reads only `methods[]`, so if that path fired, the formats would disagree on C56.
- **Two comments are false.** `scan_fs/package_db/mod.rs` (the Yocto reader calls) states that opkg-installed, image-manifest and recipe collisions record C56. They do not.
- **Nothing is ignored.** No `#[ignore]`d test remains to restore. Spec 1530 rewrote milestone 105's SC-010 test (`tests/dedup_precedence_determinism.rs`), which now records that C56 is not emitted.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A merged component says which other readers found it (Priority: P1)

An SBOM consumer sees one component for a library that a git submodule pins and a CMake `FetchContent_Declare` also fetches at the same commit. Today the consumer learns one mechanism and cannot tell that a second reader independently found the same thing. After this change the component names both: one in C55 and the other in C56.

**Why this priority**: This is the defect #1558 reports. The agreement of independent readers is evidence a consumer can weigh, and waybill currently discards it.

**Independent Test**: Scan a synthetic super-project with a submodule of `waybill-fixture-*` at commit `C` and a `FetchContent_Declare` of the same repository at `C`. Check that the scan has one component with C55 set to one mechanism and C56 listing the other, in all three formats.

**Acceptance Scenarios**:

1. **Given** a submodule and a `FetchContent_Declare` of one repository at one commit, **When** scanned, **Then** one component is emitted, with C55 set to the survivor's mechanism and C56 `["<the other mechanism>"]`.
2. **Given** an opkg-installed database and a Yocto image manifest naming the same package and version, **When** scanned, **Then** one component is emitted and C56 lists the mechanism that is not its C55.
3. **Given** a library with both a pkg-config `.pc` file and a CMake config package, **When** scanned, **Then** one component is emitted and C56 lists the mechanism that is not its C55.
4. **Given** three readers producing one canonical PURL with three distinct mechanisms, **When** scanned, **Then** C56 lists the two that are not C55, sorted.
5. **Given** a conditional `FetchContent_Declare` under a git submodule of the same repository (spec 1538 R9), **When** scanned, **Then** the submodule keeps C55 `git-submodule`, records the fetch in C233 as today, and carries C56 `["cmake-fetchcontent-git"]`.

---

### User Story 2 - The CycloneDX document stays valid (Priority: P1)

A consumer validates waybill's CycloneDX output against the 1.6 schema. A merged component with C56 must not make the document invalid, as milestone 105's planned carrier would.

**Why this priority**: An invalid document is rejected outright by validating consumers. This would be a regression on every scan with a cross-reader merge.

**Independent Test**: Validate every fixture document from User Story 1 against the vendored CycloneDX 1.6 schema.

**Acceptance Scenarios**:

1. **Given** any component carrying C56, **When** its CycloneDX document is validated against the vendored 1.6 schema, **Then** it validates.
2. **Given** the same scan in all three formats, **When** compared with `waybill sbom parity-check`, **Then** C55 and C56 agree.

---

### User Story 3 - One key, one meaning (Priority: P2)

A consumer reading `waybill:also-detected-via` can rely on it always naming source mechanisms. The symbol-fingerprint collision, which names other libraries, has a key of its own.

**Why this priority**: A row that means two things cannot be parsed reliably, and the existing collision path would already break format parity whenever it fires.

**Independent Test**: Run the fingerprint matcher on a synthetic binary whose symbols match two bundled fingerprint records. Check that the new key carries the other library's name in all three formats and that C56 is absent.

**Acceptance Scenarios**:

1. **Given** a binary matching two fingerprint records, **When** scanned, **Then** each matched component carries the other library under the fingerprint-collision key, the formats agree on it, and neither carries C56.

---

### User Story 4 - Nothing else changes (Priority: P2)

Scans with no cross-reader merge, and merges where every member has the same mechanism, produce exactly today's output.

**Why this priority**: C56 is additive evidence. It must not reshape documents that have nothing to report.

**Independent Test**: Scan fixtures with no multi-mechanism merge, and compare with `main` byte for byte.

**Acceptance Scenarios**:

1. **Given** two members of one merge group with the same mechanism, **When** scanned, **Then** no C56 is emitted.
2. **Given** a scan with no merge group of more than one distinct C55, and no fingerprint collision, **When** scanned, **Then** the output is byte-identical to `main`.

---

### Edge Cases

- **The survivor has no C55 but a member does.** In the general deduplicator, the first-wins fold gives the survivor that member's C55, and C56 lists only the mechanisms that differ from it. Where a site does not fold annotations (the design-source reconciler), the survivor keeps no C55 and records no C56 (research R5).
- **Two losers share a mechanism.** It appears once in C56.
- **A member has no C55.** It contributes nothing to C56.
- **A main-module component is the survivor.** C56 applies as on any other component. The C56 CycloneDX extractor already walks the main module.
- **A split document (`--split`).** C56 travels with its component into whichever document that component is projected into.
- **Merges made outside the general deduplicator.** Spec 1538 R9, the design-source reconciler (milestone 191) and binary-to-source attribution (milestone 109) record C56 under the same rule (FR-004). An R9 merge keeps its C233 entry as well.
- **A component merged at two sites.** An R9 merge followed by a deduplicator merge gives one C56, the union of both.
- **A fingerprint collision on a component that also merged across readers.** It carries both keys, each with its own meaning.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When components merge into one (a merge group, at any site FR-004 names), the survivor MUST carry C56: a JSON array of the distinct C55 values of the group's members, excluding the C55 the survivor finally carries, sorted lexicographically. C56 MUST be absent when that array would be empty, and when the survivor carries no C55 (research R5).
- **FR-002**: A member without C55 MUST contribute nothing to C56. C56 MUST never contain the survivor's own C55. A member that already carries C56 from an earlier merge contributes those values as well.
- **FR-003**: The survivor's C55 MUST be chosen as today: the member with the highest evidence confidence, reader order breaking ties, and the existing first-wins fold for a survivor with no C55. No emitted C55 value may change. SC-001 counts how many would have changed under milestone 105's ranking, as the record of the alternative this spec declined.
- **FR-004**: C56 MUST be recorded wherever one component is merged into another and their C55 values differ, under FR-001's rule, at every merge site listed in research R3 (S1–S9). These include the general PURL deduplicator (which also performs binary-to-source attribution's merge, milestone 109), spec 1538 R9 (a conditional CMake fetch merged into a git submodule or a required component), the design-source reconciler (milestone 191), the CMake reader's other folds (R5, R4, vendored directories), and `cmake_search_pair`'s folds. A package search (`find_package`, `pkg_check_modules`) folded into the component it names is included: its mechanism is recorded in C56 like any other. A merge that already records the absorbed declaration elsewhere (R9 in C233) MUST record both: C233 the declaration, C56 the mechanism. A survivor merged at more than one site MUST carry one C56, the union, sorted.
- **FR-005**: The CycloneDX carrier MUST be a component property `waybill:also-detected-via` with the same JSON-array value as the SPDX annotation. **Principle V audit result** (research R1, against the vendored CycloneDX 1.6 schema): no native construct says that another reader's mechanism found this component. `evidence.identity[].methods[]` items allow only `technique`, `confidence` and `value`; `technique` is a closed enum of analysis kinds, not readers; `value` holds observed evidence, not a classification (#1442); `tools[]` holds bom-refs of BOM objects, which waybill's readers are not. SPDX 2.3 and SPDX 3.0.1 have no native carrier either, so the property is a parity bridge. The `waybill-source-mechanism` key on `evidence.identity[].methods[]` MUST NOT be emitted. The C56 catalog row and its CycloneDX extractor MUST change in the same change.
- **FR-006**: SPDX 2.3 and SPDX 3 MUST carry C56 as the annotation the C56 row already specifies, with the same value as CycloneDX.
- **FR-007**: The symbol-fingerprint collision MUST move to a key and catalog row of its own, with extractors in all three formats. The move is outright: the collision MUST NOT also be written under `waybill:also-detected-via`, in any release. **Principle V audit result** (`contracts/wire.md`): no format has a native carrier for "another library also matched these symbols". CycloneDX `evidence.identity[]` records a concluded identity, not a rival candidate, and SPDX 2.3 and 3.0.1 have no field for it, so the new row is a parity bridge in all three formats. C56 MUST mean only source mechanisms. The CHANGELOG MUST record the rename as a wire change. Catalog rows that mention co-emission with C56 (C58) MUST be updated to name the new row.
- **FR-008**: Milestone 105's precedence module MUST be removed, since FR-003 keeps the existing survivor rule and nothing else calls it. Any C55 string it defines that a reader still relies on MUST stay defined where that reader can reach it.
- **FR-009**: Statements that claim C56 behaviour the code does not have MUST be corrected: the Yocto comments in `scan_fs/package_db/mod.rs`, and the headers of `tests/dedup_precedence_determinism.rs` and `tests/cross_reader_dedup_ecosystem_107.rs`.
- **FR-010**: Which components merge MUST NOT change. Output keeps one component per canonical PURL, as today.
- **FR-011**: The three formats MUST agree on C55, C56 and the new fingerprint-collision row (`waybill sbom parity-check`). No two components may share a `bom-ref`. CycloneDX output MUST validate against the vendored 1.6 schema.
- **FR-012**: Output MUST be byte-identical to `main` for any scan in which no merge group, at any FR-004 site, has more than one distinct C55 and no fingerprint collision fires.
- **FR-013**: Repeat scans MUST produce identical output.
- **FR-014**: No new Cargo dependencies.

### Key Entities

- **Merge group**: The components emitted as one by a single merge: members sharing one canonical PURL in the general deduplicator, or a component absorbed into another by R9, the design-source reconciler or binary-to-source attribution, where the PURLs may differ (an R9 fetch with no version folded into a pinned submodule). Members, survivor and annotations are as today.
- **Source mechanism (C55)**: The string naming how a reader found a component, such as `git-submodule`, `cmake-fetchcontent-git` or `opkg-installed`. It is single-valued on a component.
- **Also-detected-via (C56)**: The sorted set of other mechanisms in the component's merge group, or absent.
- **Fingerprint collision**: The names of other libraries whose symbol fingerprints matched the same binary. It moves to its own row (FR-007).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A census, run before design, counts the following on the public corpus, the quality corpus and TheRock (CI only, `kusari-sandbox/waybill-fixture-therock-sdk`):
  - merge groups that contain more than one distinct C55, split by mechanism pair;
  - how many emitted C55 values would have changed under milestone 105's ranking, which FR-003 declines (recorded, not applied);
  - whether the fingerprint collision fires on any target;
  - for FR-004, how many merges at each other site (R9, the reconciler, binary attribution) join members with different C55.

  On the public corpus, the first three counts are read from SC-004's golden diff instead (every changed golden shows its C56). The counterfactual ranking count is measured on TheRock and the quality corpus only (research R6). The research records the counts. This spec quotes none in advance.
- **SC-002**: Each synthetic `waybill-fixture-*` fixture produces the expected C55 and C56 in all three formats:
  - a submodule plus a `FetchContent_Declare` of the same repository and commit;
  - an opkg-installed database plus an image manifest naming the same package;
  - a `.pc` file plus a CMake config package of one library;
  - a group of three mechanisms;
  - a group of two members with the same mechanism, which gets no C56;
  - a conditional fetch under a submodule of the same repository (R9), which gets C233 and C56;
  - the fingerprint collision, at two levels (research R7), because no fixture binary matching two records can be built without a C toolchain: the entry mapping writes C242 and no C56, and a component carrying C242 agrees across the three emitters.
- **SC-003**: Every fixture document passes parity-check agreement, validates against the CycloneDX 1.6 schema and has unique `bom-ref`s. Ten repeat scans of each are byte-identical.
- **SC-004**: The public corpus is regenerated (`regen_goldens`), and every golden that changes is traced to an FR. Targets with no multi-mechanism merge group are byte-identical.
- **SC-005**: The quality corpus and TheRock's source tree (measure-source against `main`) are traced component by component against a `main` control. Component counts are unchanged, and every changed component is explained by an FR.
- **SC-006**: Scan time on TheRock's source tree is at most 1.05× `main`'s, measured as the median of at least 3 runs.

## Assumptions

- The counts in Background are lower bounds traced from the code and have not been measured: 21 ranked mechanisms, at least 25 emitted C55 values. SC-001 replaces them with measurements.
- The opkg/manifest and `.pc`/CMake fixture pairs are expected to produce the same canonical PURL, on the strength of the existing comments and spec 1531 SC-003. If a pair does not merge, the fixture records that, and the pair is replaced by one that does.
- Renaming the fingerprint-collision key is a wire change for any consumer of milestone 108's output (FR-007). No alias is kept; SC-001 records whether the collision fires on any target.
- Out of scope:
  - changing which components merge, including the PURL canonicalization milestone 105 assumed and spec 1530 found was never specified;
  - recording same-mechanism duplicates, which C233 covers for CMake declarations.
