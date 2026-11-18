# Feature Specification: The same build gives the same answer, scan after scan

**Feature Branch**: `1542-deterministic-binary-merge`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1600: on the built ROCm SDK, the same waybill build answers differently from scan to scan about one component, pkg:generic/gotcha … Trace before designing: where the top-level gotcha comes from …, what its ResolvedComponent parent_purl is in each outcome, and why the CycloneDX emitter renders it top-level when completeness counts it nested. Then decide: whether a file component merged by name and hash must carry the union of every identification found in either file; how a merge becomes independent of walk order; and whether SPDX 2.3's PURL-keyed packages need the same identity rule as spec 1541's component key (purl#container). Also measure whether anything else in the SDK or the public corpus varies between scans of one build … Acceptance: on TheRock's SDK, at least 5 interleaved scans of the branch emit documents identical after masking … Synthetic waybill-fixture-* fixtures, including two same-name, same-hash binaries in different directories, scanned repeatedly to show order independence. Zero new Cargo dependencies." (full text in the conversation that created this branch)

The spec number (1542) is the next in sequence. It is unrelated to issue #1542, the C/C++ coverage tracker.

## Background

### The promise this breaks

A scan of a fixed input is expected to describe it the same way every time. Repeatable output is what lets anyone compare two SBOMs and attribute the difference to the software rather than the scanner, and it is what lets waybill's own gates (golden documents, the invariant matrix) mean anything. Spec 1541 added a second promise that rests on the first. The document's graph-completeness count, the orphan classifier, invariant I7 and the three formats give one answer about which components are unreachable (its I-A, SC-004).

### What the SDK shows (measured)

ROCm TheRock's built SDK, scanned in CI in `kusari-sandbox/waybill-fixture-therock-sdk`. The `measure-full` workflow scans each side four times (a warm-up and three timed runs), and each scan logs its graph-completeness orphan count.

| run | branch commit | branch scans | `main` scans |
|---|---|---|---|
| 44189121691 | `2dd9c4f4` | 0, 0, 1, 0 | 645 ×4 |
| 44193745935 | `2aecdf6e` (spec 1541, as landed) | 1, 1, 1, 0 | 645 ×4 |

The varying component is `pkg:generic/gotcha`:

- **When the count is 1:** it is unreachable in every view and carries `waybill:orphan-reason` = `unowned-binary`. Completeness, the classifier, I7 and the formats agree.
- **When the count is 0:** completeness and the classifier count it as reached through a container. The CycloneDX document renders it at the top level, and nothing depends on it. I7 finds it unreachable and unexplained, so I-A fails.

The surrounding shape is observed in the documents:

- **One merged file component.** `lib/rocprofiler-sdk/libgotcha.so` and `lib/rocprofiler-systems/libgotcha.so` are one component, `pkg:generic/libgotcha.so?file-sha256=10e333b8…`, which nests only `pkg:generic/timemory`. *Corrected by the FR-001 trace (`measurements/trace.md`):* they share a file name, **not** a content hash. They end two symlink chains in different directories, and the sdk one's content is `56af308b…`.
- **Three `gotcha` components.** Two are nested in `libgotcha.so.3.0.0` and `libgotcha.so.3` (hash `56af308b…`). The third is top-level, and its source files name all three paths, because milestone 148 unions source files across components with one PURL.
- **SPDX 2.3 has one.** It keys packages by PURL and folds the three into one package reached by `CONTAINS`. The formats therefore disagree on the component set, and since spec 1541 on its C45 marker.

### What is suspected but not traced

The binary walker is parallel (milestone 772). The issue (#1600) inferred that which of the two same-name, same-hash files is merged first varies between scans, and with it which identification keeps a container. This was inference, and the FR-001 trace (`measurements/trace.md`) corrects it: the files share a name, not a hash, and the variation is in reachability over a fixed document. Nobody has seen the top-level `gotcha`'s `parent_purl` in either outcome, or why the CycloneDX emitter renders it top-level when completeness counts it nested. Nobody has checked whether anything else in a document varies between scans either.

## Clarifications

### Session 2026-11-17

- Q: Variation the determinism audit finds other than #1600's: fixed here or filed separately? → A: Fixed here if it is in the binary walk, analysis or merge path; otherwise filed as its own issue and recorded (FR-006).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A scan of a fixed input says the same thing every time (Priority: P1)

A consumer scans the same build twice and compares the two SBOMs. Every difference is in the fields that are meant to differ (serial number, timestamps). Nothing about which components exist, how they nest, which are reachable, or what explains them changes between scans.

**Why this priority**: an SBOM whose content depends on thread scheduling cannot be diffed, cached, signed and re-verified, or gated against a golden. This is the defect itself.

**Independent Test**: scan a synthetic tree holding two binaries with one file name and content hash in different directories, each carrying an identification, many times. Compare the documents after masking.

**Acceptance Scenarios**:

1. **Given** a tree holding two binaries with one name and different contents (the ends of two symlink chains) in different directories, **When** it is scanned repeatedly, **Then** every document is identical after masking serial number and timestamps, in all three formats.
2. **Given** TheRock's built SDK, **When** the branch is scanned at least five times, interleaved with `main`, **Then** every branch document is identical after masking, and `gotcha`'s outcome is the same in every scan.
3. **Given** any public corpus target, **When** it is scanned repeatedly, **Then** its documents are identical after masking. Otherwise the variation is traced: fixed here if it is in the binary path, filed as its own issue if not (FR-006).

---

### User Story 2 - No identification loses its file (Priority: P1)

Two files with one name and different contents are two components. Every identification binary analysis found in a file is reported under that file's component.

**Why this priority**: today one file's identification is detached and reported at the top level with no container. That is the mechanism behind the varying answer, and the document is wrong about where that identification lives even in the scans where reachability happens to agree.

**Independent Test**: two binaries with one name and different contents, each carrying an identification. After the scan, both file components exist, every identification is nested under its own file's component, and none is top-level.

**Acceptance Scenarios**:

1. **Given** two binaries with one name and different contents, in different directories, **When** scanned, **Then** each is its own file component, every identification is nested under the file it came from, and none is emitted at the top level.
2. **Given** two binaries with one name and the **same** contents, **When** scanned, **Then** they are one file component carrying both paths, with every identification nested under it, as today.
3. **Given** the SDK, **When** scanned, **Then** `gotcha` is not emitted as a detached top-level component, and the two `libgotcha.so` are two file components.

---

### User Story 3 - One identity in every format (Priority: P2)

CycloneDX, SPDX 2.3 and SPDX 3 report the same set of components for the same scan, so they agree on each one's reachability and markers.

**Why this priority**: it completes spec 1541's FR-009 for the case it could not reach. It is P2 because, once User Story 2 removes the detached copy, the disagreement on the SDK may disappear without a format change; the trace decides.

**Independent Test**: one PURL nested in two different binaries. The three formats report the same components, and agree on C45, C104 and C105.

**Acceptance Scenarios**:

1. **Given** one PURL nested in two different binaries, **When** scanned, **Then** the three formats agree on the component set, its reachability and its markers.

---

### Edge Cases

- **Two same-name, same-hash files.** One component carrying both paths, as today, independent of order.
- **Same name, different hash.** Not merged (different PURLs, `file-sha256` differs). Unchanged.
- **Same hash, different name.** Whatever waybill does today is kept, provided it is independent of walk order.
- **A file merged into a component a package owns** (spec 1536). Ownership and nesting follow the package, as today, and are independent of order.
- **One PURL nested in many containers.** Spec 1541's component key (`<purl>#<container>`) already distinguishes them. This feature does not change it.
- **A scan with no binaries.** Unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001: Trace before designing.** Before any design is fixed, the plan MUST establish from the code and a measured run, not inference:
  - which file and which identification mechanism produce the top-level `gotcha`;
  - its `parent_purl` in each of the two outcomes;
  - why the CycloneDX emitter renders it top-level when completeness counts it nested;
  - what in the walk or the merge depends on order.

  The findings decide FR-002 to FR-004.
- **FR-002: Order independence.** For a fixed input and build, the emitted documents MUST be identical, apart from the serial number and timestamps, regardless of the order in which files are walked, analysed or merged.
- **FR-003: No identification loses its file.** Components that each name one file's bytes MUST merge only when the bytes match: same name and same content hash. Every identification binary analysis found in a file MUST be reported nested under that file's component, so none is left without a container. *(Amended by the FR-001 trace: the SDK's two `libgotcha.so` share a name, not a hash.)*
- **FR-004: One identity across formats.** The three formats MUST report the same component set for one scan. If the trace shows SPDX 2.3's PURL-keyed packages still fold distinct components after FR-003, SPDX 2.3 adopts spec 1541's component key for nested components.
- **FR-005: One reachability answer.** On every input, including the SDK in every scan, graph-completeness, the orphan classifier, invariant I7 and the three formats MUST agree on which components are unreachable and on their markers (spec 1541 I-A).
- **FR-006: Determinism audit.** The plan MUST measure whether anything else varies between repeated scans of one build: on TheRock's SDK and on every public corpus target, with documents compared after masking serial number and timestamps. Variation in the binary walk, analysis or merge path (the path #1600's lies in) is fixed in this feature. Variation anywhere else is filed as its own issue, and recorded in the measurements with the issue number.
- **FR-007: Nothing else moves.** Outside the components this feature changes, every document is unchanged. Every golden change is traced to FR-002 to FR-004.
- **FR-008: No new dependencies.** Zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names and synthetic binaries.

### Key Entities

- **File component**: the component standing for one file's bytes (`pkg:generic/<name>?file-sha256=<hash>`). Several files with one name and one hash are one file component, carrying all their paths. One name with different hashes means different file components.
- **Identification**: a component binary analysis derives from a file's contents (an embedded version string, a symbol fingerprint, a package note, a linkage), nested under that file's component.
- **Masked document**: an emitted document with its serial number and timestamps replaced by fixed values, so two scans can be compared byte for byte.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (trace first):** Before FR-002 to FR-004 are fixed in the plan, a measured run accounts for the top-level `gotcha`, its container in each outcome, and the order dependence (FR-001).
- **SC-002 (the SDK is stable):** On TheRock's built SDK, at least 5 interleaved scans of the branch emit documents identical after masking, in all three formats.
- **SC-003 (one answer on the SDK):** In every one of those scans, graph-completeness, the classifier, I7 and the three formats agree on every component's reachability and markers. That is I-A on the SDK, where spec 1541 left one component that varied.
- **SC-004 (nothing detached):** No identification in the SDK document is top-level when the file it came from is a merged file component.
- **SC-005 (synthetic repeatability):** The two-same-name-binaries fixture, scanned at least 20 times, gives one masked document in each format, and one completeness count. The trace saw the old code vary in 4 of 12 scans of it.
- **SC-006 (corpora):** The determinism audit (FR-006) is recorded for every public corpus target. The public corpus invariant gate stays green, and every golden change is traced. Every quality-corpus movement is traced against a `main` control.
- **SC-007 (cost):** The median built-SDK scan is at most 1.1× `main`'s, by the interleaved `measure-full` workflow, and the full-tree source scan likewise by `measure-source`.
- **SC-008 (tests have teeth):** Each rule (order independence, the identification union, format identity if changed) is pinned by a test that fails when the rule is removed. Repeatability tests scan enough times to have seen the old variation: across spec 1541's eight branch scans of the SDK, each outcome appeared four times.

## Assumptions

- **Scope** is determinism of emitted documents and the binary merge. The CycloneDX emitter and SPDX 2.3 identity change only where FR-001 shows they must.
- **TheRock is measured in CI only**, through the fixture repository's `measure-full` and `measure-source` workflows. Its documents are read locally; TheRock is never cloned or scanned locally.
- **Masking** covers the serial number, document creation timestamps, and any field already masked by the corpus golden comparison. Anything else that differs counts as variation.
- **The variation rate is unknown** beyond the eight observed branch scans, in which each outcome appeared four times (0,0,1,0 and 1,1,1,0). Repeatability tests therefore scan many times rather than a few, and the plan measures how often the old code varies on the synthetic fixture before relying on it.
- **Spec 1541's component key** (`<purl>#<container>`) for reachability is kept as it is.
