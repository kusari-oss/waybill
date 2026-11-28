# Feature Specification: A cabal.project.freeze governs the packages its cabal.project lists

**Feature Branch**: `1668-freeze-governs-members`
**Created**: 2026-11-28
**Status**: Draft
**Input**: #1744 (refs #1741). A root `cabal.project.freeze` does not govern a member in a subdirectory.

## Background

The test project:
- `cabal.project` lists `packages: app`.
- `app/waybill-fixture-app.cabal` depends on `waybill-fixture-frozen`.
- The root `cabal.project.freeze` pins `waybill-fixture-frozen ==4.0.0`.

A scan of this project emits two unconnected components:
- the member's declared dependency, without a version;
- the freeze pin `waybill-fixture-frozen@4.0.0`. Spec 1666 labels it C45 `waybill:orphan-reason = edgeless-lock-pin`, and that label is false here: a declared package does use the pin.

With `packages: .`, the same project emits one reachable component.

**Cause (observed in code, to be confirmed by measurement):**
- A parsed freeze file records its exact pins under its own directory only.
- A package looks up pins by its own directory, so a member under `app/` never sees the root pin.
- Spec 1663 closed the same gap for `stack.yaml.lock`: it records the lock's pins under every package its sibling `stack.yaml` lists. Freeze files got no equivalent.
- The `cabal.project` membership reading added by #1032 and spec 1662 already turns `packages:` entries (directories, globs, `./` prefixes, trailing slashes, `.cabal` paths) into package directories. Nothing applies that reading to freeze files.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A member's frozen dependency is one pinned component (Priority: P1)

An operator scans a cabal project whose packages are in subdirectories and whose root freeze file pins their dependencies. The operator expects each frozen dependency once, at its pinned version, reached from the package that declares it. A versionless twin should not appear, and no orphan reason should be needed.

**Why this priority**: today the pinned version is disconnected from its user, which defeats the purpose of a freeze file, and the orphan label says something false (#1741 was filed for that label).

**Independent Test**: scan the #1744 fixture and check:
- there is exactly one `waybill-fixture-frozen` component, at `4.0.0`;
- the member has an edge to it;
- it has no C45;
- its `waybill:source-files` names `cabal.project.freeze`;
- CycloneDX, SPDX 2.3 and SPDX 3 agree.

**Acceptance Scenarios**:

1. **Given** `packages: app` and a root freeze file pinning the member's dependency, **When** scanned, **Then** one component at the pinned version is reached from the member, and no versionless twin exists.
2. **Given** `packages:` written as a glob (`libs/*`), with a `./` prefix, with a trailing slash, or as an explicit `.cabal` path, **When** scanned, **Then** every listed member is governed exactly as in scenario 1.
3. **Given** a freeze pin that no listed package depends on, **When** scanned, **Then** it stays unreachable and keeps C45 `edgeless-lock-pin` (negative control).
4. **Given** a governed member that also depends on `waybill-fixture-loose`, which the freeze file does not pin, **When** scanned, **Then** `waybill-fixture-loose` is emitted as today: one versionless design-tier component reached from the member. The member's main module changes only in `waybill:sbom-tier`, from `design` to `source`. That matches what the `packages: .` control already emits on `main`.
5. **Given** a package directory that `cabal.project` does not list, **When** scanned, **Then** the freeze file does not govern it. Its declared dependency stays as today.

### User Story 2 - The corpus changes only where measurement explains it (Priority: P2)

A maintainer wants every golden change traced to this rule.

**Independent Test**: before and after measurement over the corpus goldens, covering:
- every component carrying C45 `edgeless-lock-pin`;
- every versionless `pkg:hackage` component whose name a freeze file in that target pins.

Every difference is explained in `measurements/branch.md`.

**Acceptance Scenarios**:

1. **Given** the committed goldens on `main`, **When** regenerated on the branch, **Then** every changed line is one this rule predicts, and each one is listed.

### Edge Cases

- A freeze file beside a `cabal.project` in a subdirectory (not the scan root) governs what that `cabal.project` lists, resolved against its own directory.
- A freeze file with no sibling `cabal.project` keeps today's behaviour: it governs its own directory only.
- A `packages:` entry that matches no discovered package directory adds nothing.
- An entry that is an unparseable glob is ignored, as membership already ignores it.
- A pin that both a stack lock and a freeze file supply for the same directory: the union of pinned names, unchanged from how `pins_by_dir` merges today.
- A `RangeConstraint` freeze entry still does not suppress a declaration (only exact pins do), unchanged.
- An entry outside the project directory (`../x`) is not governed. Membership is limited to discovered directories under the scan root.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A parsed `cabal.project.freeze` MUST govern its own directory and every package directory that its sibling `cabal.project` lists in `packages:` or `optional-packages:`.
- **FR-002**: The listed directories MUST be found by the same `packages:` reading that #1032 and spec 1662 use. That reading covers directories, globs with literal separators, `./` and trailing-slash forms, and `.cabal` paths naming their directory. A second parser MUST NOT be written.
- **FR-003**: The "directories a lock governs" step MUST be one helper used by both freeze files and `stack.yaml.lock`. Each lock kind supplies its own membership source: `cabal.project` for a freeze file, `stack.yaml` for a Stack lock. Rationale: the two copies are how #1744 arose, since spec 1663 fixed one and not the other.
- **FR-004**: A governed member's dependency that the freeze file pins exactly MUST resolve to the single pinned component, reached from the member, with no C45 orphan reason.
- **FR-005**: That component's `waybill:source-files` MUST include the freeze file (spec 1665's rule, applied as before).
- **FR-006**: A freeze pin that no governed package depends on MUST keep C45 `edgeless-lock-pin`.
- **FR-007**: A package directory not listed by the sibling `cabal.project` MUST be unaffected by this change.
- **FR-008**: The result MUST be parity-equal across CycloneDX, SPDX 2.3 and SPDX 3.
- **FR-009**: The change MUST add zero Cargo dependencies.
- **FR-011**: A governed member's dependency that the freeze file does not pin MUST be emitted exactly as before. Suppression stays per dependency (#938). The member's main module MUST differ from `main` only in `waybill:sbom-tier` = `source`, the value an in-directory freeze file already gives (measured: control fixture (f), `measurements/main.md`).
- **FR-010**: Measurement MUST precede code. On `main`, measure:
  - the #1744 fixture and its four `packages:` variants;
  - the corpus population named in User Story 2.

  Repeat the same measurement on the branch, and explain every difference.

### Key Entities

- **Lock file**: a `cabal.project.freeze` or a `stack.yaml.lock`. It supplies a set of exactly pinned names.
- **Governed directories**: the lock's own directory, plus the package directories its sibling project file lists.
- **Member**: a package directory that the project file lists.

## Success Criteria *(mandatory)*

- **SC-001**: The #1744 fixture yields exactly 1 `waybill-fixture-frozen` component, at `4.0.0`, with 1 edge from the member and 0 C45 values. On `main` it yields 2 components and 1 `edgeless-lock-pin`.
- **SC-002**: Each of the 4 `packages:` variants gives the same result as SC-001.
- **SC-003**: The negative-control pin keeps `edgeless-lock-pin` in all three formats.
- **SC-004**: Each mutation of the governed-directory expansion is killed by at least one test, and an unmutated control passes. The mutations are:
  - freeze governs only its own directory;
  - drop glob members;
  - drop `.cabal`-path members;
  - the lock's own directory is not governed.
- **SC-005**: Corpus goldens change only on lines that the measurement attributes to this rule.
- **SC-006**: One pre-PR gate passes and one PR is opened.

## Assumptions

- "Sibling `cabal.project`" means a file named `cabal.project` in the same directory as the freeze file. Cabal names the freeze file after the project file, and `cabal.project.local` adds no packages.
- The corpus may contain no affected target. Then the fixture is the evidence, and the goldens are unchanged (to be measured).
- Spec 1666's C45 classification runs after resolution, so a pin that becomes reachable drops its orphan reason with no change to that pass.
