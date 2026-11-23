# Feature Specification: a binary match that merges into another reader's component stays visible on it (#1665)

**Feature Branch**: `1646-merged-binary-evidence`
**Created**: 2026-11-23
**Status**: Draft
**Input**: #1665. A curated embedded-version match that merges into an installed-library record (pkg-config, CMake package version) leaves no trace of itself on the merged component. Refs #1543, #1662, #1641.

## Background

**Measured on TheRock** (`measure-full` run 45082092571 at `cd88e02b`, whose code is `main` `e1148818`'s; `census-versions` run 45082095568). The census reports 7 curated libraries matched from their own bytes: bzip2 1.0.8, expat 2.7.5, gmp 6.3.0, liblzma 5.8.1, llvm 23.0.0git, ncurses 6.6.20251230 and zlib 1.3.2. Their final components:

| Component | Merged into a record? | `waybill:evidence-kind` (C4) | `waybill:confidence` (C16) | C56 `also-detected-via` | `evidence.identity` methods (CycloneDX) |
|---|---|---|---|---|---|
| bzip2@1.0.8 | no | `embedded-version-string` | `heuristic` | — | `manifest-analysis` 0.85 |
| expat@2.7.5 | pkg-config | — | — | `cmake-package-version` | `manifest-analysis` 0.85 |
| gmp@6.3.0 | pkg-config | — | — | — | `manifest-analysis` 0.85 |
| liblzma@5.8.1 | pkg-config | — | — | `cmake-package-version` | `manifest-analysis` 0.85 |
| llvm@23.0.0git | CMake | — | — | `cmake-find-package` | `manifest-analysis` 0.85 |
| ncurses@6.6.20251230 | pkg-config | — | — | — | `manifest-analysis` 0.85 |
| zlib@1.3.2 | pkg-config | — | — | `cmake-find-package` | `manifest-analysis` 0.85 |

So 6 of the 7 lose every trace that the library's own bytes named it. The library file is still listed among the component's source files, but nothing says which reader read it. Three more facts:
- **The loss is asymmetric.** zlib keeps `waybill:fingerprint-symbols-matched=10/10`: symbol-fingerprint corroboration survives a merge, and the version-string match does not.
- **Mechanism, read from code at `e1148818`** (`resolve/deduplicator.rs`): the deduplicator keeps one winner by confidence, and at equal confidence the readers' order decides, so the record wins. It unions the loser's free-form annotations, which is why the fingerprint count survives. The loser's typed evidence kind and confidence go with it. C56 records only the losers' *source mechanisms*, and a curated entry has none.
- **The native field is wrong even without a merge.** Every component read from a package database or a binary is given one technique, `PackageDatabase` (`waybill-common/src/resolution.rs`), which CycloneDX emits as `manifest-analysis` (`generate/cyclonedx/evidence.rs`). bzip2's version was read from its binary's string section, which CycloneDX 1.6 calls `binary-analysis`. SPDX 2.3 and SPDX 3 carry the same identity in an `evidence.identity` annotation (catalog row D1, presence-only parity), with the internal technique name.

## Clarifications

### Session 2026-11-23

- Q: How does SPDX's D1 `evidence.identity` annotation (today one flat `{technique, confidence}` object) carry a second method? → A: The flat `technique` and `confidence` keys stay, holding the winner's method. A `methods` array listing every method is added only when a component has two or more. Single-method components are byte-identical to `main`.
- Q: Which binary-derived evidence kinds does FR-003 relabel as `binary-analysis`? → A: Only `embedded-version-string` and `symbol-fingerprint`. The census counts every other binary-derived kind (`dynamic-linkage`, `embedded-source-path`, Go build info, `cargo-auditable`). A follow-up issue decides those from the counts; embedded manifests such as Go build info are arguably `manifest-analysis`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A merged component says its library was also read from the binary (Priority: P1)

An operator scans an SDK whose libraries ship with pkg-config or CMake files. Each library merges into one component, which is correct (#1662). But the document no longer says that waybill read the library's version from its own bytes, only that a `.pc` file declared it. A consumer weighing the identity cannot tell a library corroborated by its binary from one whose binary was never read.

**Why this priority**: This is the defect (#1665). It affects 6 of TheRock's 7 curated libraries, and every future banner, anchor or symbol match that merges.

**Independent Test**: Synthetic `waybill-fixture-*` trees, one per merge shape. In each, the merged component records the binary match in every format.

**Acceptance Scenarios**:

1. **Given** a library whose banner agrees with a `.pc` file beside it, **When** scanned, **Then** the one merged component's CycloneDX `evidence.identity` holds two methods:
   - the record's `manifest-analysis`;
   - a `binary-analysis` method for the banner match.

   SPDX 2.3 and SPDX 3 carry the same two techniques in their D1 `evidence.identity` annotation.
2. **Given** the same library with a CMake package-version file instead of a `.pc` file, **When** scanned, **Then** the result is the same.
3. **Given** a library matched by both its banner and its exported symbols, with an agreeing `.pc` file, **When** scanned, **Then** the merged component records the binary match once and keeps its symbol count (`fingerprint-symbols-matched`), as on `main`.
4. **Given** any of these trees, **When** the three formats are emitted, **Then** `waybill sbom parity-check` reports no gap that `main` does not.

---

### User Story 2 - A version read from a binary is labelled binary analysis (Priority: P2)

A consumer reads CycloneDX's evidence technique to judge how a component was identified. Today a version read from a binary's bytes says `manifest-analysis`, the same as a `.pc` file or a dpkg database.

**Why this priority**: It is the same defect at the root: the binary match's own label is wrong even when nothing merges. It changes more components than the merge does, so it is counted before it is wired.

**Independent Test**: A banner-only tree and a symbol-fingerprint-only tree, each without a record.

**Acceptance Scenarios**:

1. **Given** a library matched only by its banner, **When** scanned, **Then** its CycloneDX identity method is `binary-analysis`. C4 stays `embedded-version-string` and C16 stays `heuristic`.
2. **Given** a library matched only by its exported symbols, **When** scanned, **Then** its identity method is `binary-analysis`.

---

### User Story 3 - Nothing else changes (Priority: P2)

**Why this priority**: The change is to evidence, not identity. No component, version, PURL or edge may change.

**Independent Test**: The public corpus, the quality corpus and TheRock, each traced against a `main` control.

**Acceptance Scenarios**:

1. **Given** any scan, **When** compared with `main`, **Then** the set of components, PURLs, versions and edges is identical, and every change is an evidence field FR-001 to FR-004 name.
2. **Given** a component that no binary match touched, **When** compared with `main`, **Then** it is byte-identical.

---

### Edge Cases

- **Two binary matches, one component.** A banner match and a symbol fingerprint for the same library on the same binary are already one entry on `main` (milestone 096). The merged component records one `binary-analysis` method, not two.
- **The binary match wins the merge.** At equal confidence the readers' order decides. Whichever side wins, the record's method and the binary's method both appear.
- **A binary match merges into another binary match**, for example the same library in two binaries. There is one `binary-analysis` method, not one per file.
- **A record merges into a record** (expat's `.pc` and CMake files). There is no binary match, so nothing changes. C56 already records the second mechanism.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census first, log-only)**: Before any output changes, a DEBUG census event SHALL be emitted for every merge in which an entry whose evidence came from a binary match (C4 `embedded-version-string` or `symbol-fingerprint`) is absorbed into, or absorbs, an entry from another reader. The event records:
  - the PURL;
  - each side's reader mechanism, evidence kind, technique and confidence;
  - which side won.

  The same commit SHALL also log every component whose technique FR-003 would change. Both run on TheRock and the quality corpus, and are recorded before anything is wired.
  - The public corpus's harness does not enable the census, so its changes are measured exactly by the wired commit's golden diff.
- **FR-002 (the binary match on a merged component)**: When a binary-match entry and an entry from another reader merge, the merged component's identity evidence SHALL list one method per distinct technique, in a stable order.
  - That is the winner's method plus `binary-analysis`.
  - In CycloneDX this is `evidence.identity[].methods[]`, a native field (Principle V).
  - In SPDX 2.3 and SPDX 3 it is the existing D1 `evidence.identity` annotation. Its flat `technique` and `confidence` keys keep the winner's method. A `methods` array listing every method, in the same order as CycloneDX's, is added only when there are two or more, so a single-method component's payload is unchanged. **No new annotation or catalog row.**
- **FR-003 (the technique of a binary match)**: A component whose identity came from a binary's own bytes (C4 `embedded-version-string` or `symbol-fingerprint`) SHALL report the technique `binary-analysis`, merged or not.
  - Every other evidence kind keeps its technique in this spec (clarification 2).
  - The census (FR-001) counts the components each other binary-derived kind would change (`dynamic-linkage`, `embedded-source-path`, Go build info, `cargo-auditable`). A follow-up issue is filed with those counts. Embedded manifests (Go build info, `cargo-auditable`) may rightly stay `manifest-analysis`; structural inferences may not.
- **FR-004 (confidence)**: The `binary-analysis` method's confidence SHALL be the confidence the unmerged binary match reports on `main` (0.85, the value of `PackageDatabase`). The merged component's own C16 and tier stay the winner's, as on `main`. No new numbers are invented. Whether two independent sources should raise confidence is out of scope (Assumptions).
- **FR-005 (C4, C16 and C56 unchanged in meaning)**:
  - C4 and C16 stay single-valued, the winner's.
  - C56 keeps listing reader *mechanisms* only. Evidence kinds are not mechanisms, and mixing the two vocabularies would make C56 ambiguous.

  The binary match is recorded by FR-002, the native field.
- **FR-006 (every merge shape)**: FR-002 SHALL apply to every merge site that can absorb a binary match. That includes:
  - spec 1531's PURL merge with pkg-config and CMake records (expat, gmp, liblzma, zlib);
  - spec 1645's banner-plus-record merge (ncurses, llvm);
  - spec 1644's anchored libraries (gmp, liblzma).

  The census lists the merge sites it observed.
- **FR-007 (formats agree)**: The three formats SHALL carry the same techniques for each component. D1 stays presence-only, and `waybill sbom parity-check` SHALL report no gap that `main` does not.
- **FR-008 (scope of change)**: Components, PURLs, versions, CPEs and edges SHALL be identical to `main`. Output differs only in identity evidence: FR-002's added method and FR-003's technique. Each changed component is counted.
- **FR-009 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **Binary-match entry**: an entry whose evidence kind is `embedded-version-string` or `symbol-fingerprint`.
- **Identity method**: a technique (CycloneDX's vocabulary) and a confidence; a component has one or more.
- **Merge**: the deduplicator's absorption of one entry into another with the same PURL; it has a winner and a loser.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock, each of the 6 merged curated libraries (expat, gmp, liblzma, llvm, ncurses, zlib) records the binary match in all three formats, and bzip2 is labelled `binary-analysis`. This is traced against a `main` control, and no component, PURL, version or edge changes.
- **SC-002**: The census (FR-001) is recorded for TheRock and the quality corpus before wiring. It gives the number of components whose technique changes, by evidence kind.
- **SC-003**: The synthetic trees cover banner plus `.pc`, banner plus CMake, banner alone, fingerprint alone, and fingerprint plus banner plus `.pc`. Each shows the expected methods in every format.
- **SC-004**: Each rule is guarded by a test that fails when the rule is removed, shown by mutation: the added method on merge, the technique of a banner match, the technique of a fingerprint match, and the one-method-per-technique rule.
- **SC-005**: `waybill sbom parity-check` shows no new gap on any acceptance tree or on TheRock.
- **SC-006**: The public corpus differs from its goldens only in identity evidence, with each changed component traced, or not at all. Its goldens are regenerated only if anything changed.
- **SC-007**: TheRock's scan time is within 1.05× `main`, measured as the median of at least 3 interleaved runs per arm with one warm-up each, and reported with min, median and max.

## Assumptions

- **CycloneDX 1.6's technique vocabulary** includes `binary-analysis` and `manifest-analysis`, and `methods[]` is an array. Waybill already validates its CycloneDX output against the 1.6 schema in its tests.
- **Confidence aggregation is out of scope.** Combining two independent sources into a higher confidence is a design question for the identity dataset (#1667), and has no measured basis today.
- **Other binary-derived evidence kinds keep their technique.** `embedded-source-path`, `dynamic-linkage`, `cargo-auditable` and Go build info are counted by the census, and their relabelling is a follow-up if the counts justify it.
- **TheRock is scanned in CI only**, through `kusari-sandbox/waybill-fixture-therock-sdk`, dispatched with `--ref main` and full SHAs, with every `refs.txt` verified.
- **Fixtures use synthetic `waybill-fixture-*` names**, except where a library's own banner or name is what is under test.
