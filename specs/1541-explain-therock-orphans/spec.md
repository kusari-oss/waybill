# Feature Specification: Every unreachable component on TheRock explains itself

**Feature Branch**: `1541-explain-therock-orphans`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1593: on ROCm TheRock, unreachable components carry no orphan explanation, breaking invariant I7 … Decide, after tracing the cause before designing …: whether the classifier and graph-completeness must share one reachability answer …; whether a component attached by containment is reachable …, with the three formats agreeing; which reason codes cover binary-analysis components in a source tree … and lockfile packages in subprojects with no owning project, reusing existing C45 values where they fit, otherwise registering new ones in the catalog first; and how far #1214 is in scope … Measure TheRock in CI only via the fixture repo … Synthetic waybill-fixture-* fixtures; zero new Cargo dependencies." (full text in the conversation that created this branch)

The spec number (1541) is the next in sequence. It is unrelated to issue #1541.

## Background

### The invariant

Invariant **I7** (`docs/reference/sbom-invariants.md`): a component not reachable from the subject by any dependency edge carries a marker explaining why. The marker is one of:
- `waybill:orphan-reason` (C45);
- `waybill:unresolved-reason` (C151);
- an `unreferenced` closure role;
- the file tier.

Without one, a consumer cannot tell whether the component is part of the artifact that waybill failed to relate, or something else in the tree.

I7 is triaged in two directions, and both are in scope here:
- *a reader should have written an edge*, in which case the fix is the edge;
- *the component genuinely has no edge*, in which case the fix is a reason.

Feature 1218 found the split 18 to 16 on a population first read as entirely the second.

### What TheRock shows

Measured in the fixture repository `kusari-sandbox/waybill-fixture-therock-sdk`, never locally (#1593):

| scan | run | waybill | unreachable (with a PURL) | explained |
|---|---|---|---|---|
| source tree, submodules checked out | `measure-source` 44028592278 | `b8ba7e4` | 846 | 5 |
| built ROCm 7.14 SDK | `measure-full` 43977697282 | v0.17.0 | 645 | 0 |

**Source tree:**

| count | kind |
|---|---|
| 453 | `library`, `pkg:generic`, from binary analysis |
| 275 | `application`, `pkg:generic`, from binary analysis; many are test inputs, e.g. LLDB's `postmortem/netbsd-core/1lwp_SIGSEGV.aarch64` |
| 59 | `file`, `pkg:generic`, from binary analysis |
| 35 | `pkg:pypi` from `uv.lock` files in subprojects, e.g. `rccl/bindings/nccl4py/uv.lock`, `hipblaslt/tensilelite/uv.lock` |
| 8, 4, 2, 2, 1 | `pkg:cargo`, `pkg:npm`, `pkg:conan`, `pkg:vcpkg`, `pkg:deb` |

**SDK:** 644 of the 645 are libraries nested inside another component: bundled `third_party` code under a binary (C228), and binaries inside Python packages (C230). In CycloneDX, nesting is containment, not a dependency edge, so the graph pass counts them unreachable. SPDX expresses the same nesting with `CONTAINS`.

### What is already in place

- **Feature 1218 (#1214)** made the classifier ecosystem-agnostic, and added `build-toolchain`, `foreign-build-system` and `unreferenced-lock-node` to C45.
- **#1214's corpus populations** are explained, except `maven-guice`'s, declared as a known I7 failure under #1235.
- **TheRock's populations match none of the existing codes,** so they stay unexplained.

### What is suspected but not traced

#1593 states as inference that the classifier runs before emission with a placeholder root. The emitted document's root comes from the operator's `--root-name` (TheRock's scans pass `--root-name TheRock`). So the classifier may compute reachability from a different subject than the document's `waybill:graph-completeness`, and leave unclassified components that the document then reports unreachable.

This feature traces that before designing (FR-001).

## Clarifications

### Session 2026-11-17

- Q: How are components nested inside another component treated? → A: Containment counts as reachability in graph-completeness, the classifier and I7: a component nested in a reachable component is reachable. The same rule applies in all three formats.
- Q: One reason code or two for binary-analysis components in a source tree? → A: Two: one for binaries under a test-data directory, one for the rest.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every unreachable component on TheRock says why (Priority: P1)

A consumer of TheRock's SBOM, source or SDK, walks the dependency graph from the root, and finds components it cannot reach. Each one carries a marker saying why it is in the document without an edge. Or it is reachable, because the edge that was missing is now written.

**Why this priority**: 846 and 645 unexplained components make the graph's completeness signal useless on the tree this effort exists for (#1542).

**Independent Test**: The fixture repository's `measure-source` and `measure-full` runs at the branch. Count the unreachable components carrying no I7 marker: 0, or each remaining one is a declared waybill defect with its own issue.

**Acceptance Scenarios**:

1. **Given** TheRock's source tree, **When** scanned at the branch, **Then** every unreachable component carries an I7 marker.
2. **Given** TheRock's built SDK, **When** scanned at the branch, **Then** components nested in a reachable component are reachable (FR-003), and every remaining unreachable component carries an I7 marker.
3. **Given** any population on TheRock where tracing shows a reader should have written an edge, **When** scanned at the branch, **Then** the edge is written and the components are reachable, not explained.

---

### User Story 2 - One reachability answer, in every format (Priority: P1)

A consumer comparing the document's `waybill:graph-completeness` reason, the per-component markers, and the three formats gets one consistent answer about which components are unreachable.

**Why this priority**: an explanation computed against a different reachability set from the one the document reports is not an explanation. This is the spec 1537 principle (one rule for root edges in every format), applied to reachability.

**Independent Test**: In every contract fixture and on TheRock, check that these are the same set:
- the components the document counts as unreachable;
- the components the classifier saw as unreachable;
- the components I7 checks;

and that the three formats agree on it.

**Acceptance Scenarios**:

1. **Given** a scan with an operator root (`--root-name`), **When** emitted, **Then** the classifier's unreachable set equals the document's.
2. **Given** a component nested in a reachable component, **When** emitted in the three formats, **Then** all three count it reachable, and none gives it an orphan marker.

---

### User Story 3 - Measured on TheRock, the corpora unchanged except as traced (Priority: P2)

The maintainer sees, from CI, the before and after on TheRock source and SDK, the cost, and every corpus change traced.

**Why this priority**: the same discipline as specs 1538 to 1540. Changing a reachability rule can move every corpus target's completeness and orphan markers.

**Independent Test**: `measure-source` and `measure-full` at the branch against `main`; the public corpus and the quality corpus against `main` controls.

**Acceptance Scenarios**:

1. **Given** the public corpus invariant gate (`invariant-must-pass.toml`), **When** run at the branch, **Then** it stays green, and every golden change is traced to a rule in this spec.

### Edge Cases

- **A component both nested and depended on.** It is reachable through the dependency edge, whatever FR-003 decides for containment.
- **A nested component whose container is itself unreachable.** Containment does not make it reachable from the root. It is explained as any orphan is, by its own population's reason.
- **A binary in a source tree that a package also claims.** It is a part of that package, not an orphan, and is treated as the package's contents.
- **The operator overrides the root, or leaves it unset.** Both give the same reachability answer in the classifier and the document.
- **`maven-guice`'s #1235 population** stays a declared I7 failure, a waybill defect. Explaining it would close the cell reporting it.
- **A population the tracing shows is a reader's missing edge,** such as a lockfile whose own project component exists but is not attached. It gets the edge, not a reason.
- **A scan without `--root-name`.** It behaves as today, apart from what FR-001's tracing changes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001: Trace before designing.** Before any rule changes, the plan MUST establish from the code and a measured run, not inference, why the 846 and 645 carry no marker:
  - whether the classifier's reachable set differs from the document's, and why (the placeholder root, `--root-name`, or something else);
  - for each population above, whether it lacks an edge a reader should write, or genuinely has none.

  The findings decide FR-002 to FR-005, and are recorded with the counts they explain.
- **FR-002: One reachability answer.** The orphan classifier, the document's `waybill:graph-completeness` and the I7 check MUST compute unreachability from the same subject and the same edges, so the three populations are identical in every scan. That includes scans with an operator root.
- **FR-003: Containment.** Containment MUST count as reachability, in graph-completeness, the orphan classifier and the I7 check alike. A component nested inside another (CycloneDX `components[].components[]`; SPDX 2.3 and SPDX 3 `CONTAINS`) is reachable when its container is.
  - No dependency edge is added, and no reason is given: nesting already states how the component is in the document.
  - The three formats MUST give the same answer.
  - A component nested in an unreachable container stays unreachable, and is explained as any orphan is.
- **FR-004: Binary-analysis components in a source tree.** An unreachable component found by binary analysis in a source scan, owned by no package, MUST carry one of two new C45 reasons:
  - **one for binaries under a test-data directory:** any path segment in a closed list that the plan fixes from TheRock's measured paths. It starts from spec 1538's test-scope directories, `test`, `tests` and `unittests`, plus `testdata` and the like where measured;
  - **one for every other such binary.**

  Both codes are registered in the C45 row and its contract before emission, below every existing code in priority, so no component carrying an existing code changes. Their exact strings are fixed in the plan.
- **FR-005: Lockfile packages with no owner.** For packages from a lock file (such as `uv.lock`) in a subproject, unreachable from the root:
  - if the lock file's own project is in the document, the packages MUST be attached to it, as spec 1537 attaches a requirements file's packages to its declaring project;
  - otherwise they carry the existing code that fits (`unreferenced-lock-node`, `foreign-build-system` or `dead-lockfile-entry`, by their current definitions);
  - otherwise a new code is registered first.
- **FR-006: Existing markers unchanged.** Every component that carries a C45 code on `main` MUST carry the same code. New codes sit below every existing code, as feature 1218's did (its FR-004).
- **FR-007: Remaining small populations.** TheRock's cargo, npm, conan, vcpkg and deb orphans (17 in the source scan) MUST each be traced and either attached or explained. A population that is a waybill defect is declared in `invariant-declarations.toml` with its own issue, as #1235 is, not given a reason.
- **FR-008: #1214.** #1214's own corpus populations are not re-opened. If this feature's rules newly explain any of them, the corpus run shows it, and #1214 is closed only when every population it lists is explained or declared.
- **FR-009: Formats agree.** CycloneDX, SPDX 2.3 and SPDX 3 MUST agree on which components are unreachable and on their markers. The root's direct targets keep agreeing (spec 1537 I-1).
- **FR-010: No new dependencies.** Zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names.

### Key Entities

- **Subject**: the component reachability is computed from. The document's root, as the emitted document states it.
- **Unreachable set**: components with a PURL that no chain of dependency edges and containment connects to the subject (FR-003).
- **I7 marker**: C45, C151, an `unreferenced` closure role, or the file tier.
- **Population**: a group of unreachable components with one cause, as traced (FR-001).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (trace first):** Before FR-002 to FR-005 are fixed in the plan, a measured run accounts for every unreachable component on TheRock (source and SDK) by population and cause, and the totals equal the document's own count.
- **SC-002 (TheRock source):** At the branch, 0 unreachable components without an I7 marker. Any exception is a declared waybill defect with an issue number.
- **SC-003 (TheRock SDK):** The same, for the built SDK.
- **SC-004 (one answer):** In every contract fixture and both TheRock scans, the document's unreachable count, the classifier's unreachable set and the I7 population are equal, and the three formats agree.
- **SC-005 (no regressions in explanation):** Every component carrying a C45 code on `main`, on TheRock and across the public corpus, carries the same code at the branch.
- **SC-006 (corpora):** The public corpus invariant gate stays green, and every golden change is traced to a rule here. Every quality-corpus movement is traced against a `main` control.
- **SC-007 (cost):** The median full-tree TheRock source scan is at most 1.5× `main`'s, by the interleaved `measure-source` workflow.
- **SC-008 (tests have teeth):** Each rule (shared reachability, containment, each of the two new codes and the test-data path rule, lockfile attachment) has a contract test, and a mutation that removes or weakens it is caught.

## Assumptions

- **Scope** is reachability and orphan explanation in the emitted document. Readers change only where FR-001 shows a reader should have written an edge.
- **TheRock is measured in CI only,** through the fixture repository's `measure-source` (source tree with submodules) and `measure-full` (the built SDK) workflows. It is never cloned or scanned locally.
- **#1235** (`maven-guice`) stays a declared I7 failure.
- **The numbers above are from runs at `b8ba7e4` and v0.17.0.** The plan re-measures at current `main` before designing (SC-001).
