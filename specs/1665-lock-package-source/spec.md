# Feature Specification: Haskell lock-file packages record their lock file

**Feature Branch**: `1665-lock-package-source`
**Created**: 2026-11-27
**Status**: Draft
**Input**: #1736. Components built from `stack.yaml.lock` package entries and from `cabal.project.freeze` pins record an empty source path, so `waybill:source-files` names no file.

## Background (measured)

Spec 1663 made each Stack snapshot component record its lock file. It did not touch package components. The issue measured the gap after spec 1663:
- hs-opentelemetry (`25f5b2d`) has 6 packages pinned by its root `stack.yaml.lock`, among them `co-log@0.6.1.2` and `proto-lens@0.7.1.6`. All 6 are emitted with no source file.
- lootbox (`2cd7b1e`) has 16 such packages, likewise with no source file.

A consumer can't tell which lock file pinned a package.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every lock-pinned package names its lock file (Priority: P1)

An operator reads the SBOM of a Stack or Cabal project. For any package whose version a lock file pinned, they can see which file pinned it.

**Why this priority**: This is the defect. Provenance is the reason the field exists.

**Independent Test**: Scan a fixture with a `stack.yaml.lock` and a `cabal.project.freeze`. Every component they pin lists its file in `waybill:source-files`.

**Acceptance Scenarios**:

1. **Given** a `stack.yaml.lock` pinning two packages, **When** scanned, **Then** both components list that lock file, rootfs-relative.
2. **Given** a `cabal.project.freeze` exact pin, **When** scanned, **Then** the component lists the freeze file.
3. **Given** the same package at the same version pinned by two lock files, **When** scanned, **Then** there is one component, and it lists both files, sorted.
4. **Given** the same package name pinned at different versions by two lock files, **When** scanned, **Then** there are two components with different PURLs, and each lists only the file or files that pinned its version.

---

### User Story 2 - Nothing else moves without a reason (Priority: P1)

The same operator gets no other change: no orphan reason changes silently, no component merges differently, and no source file is lost or wrongly added.

**Why this priority**: Orphan classification and deduplication both read source files, so recording a new one could change them.

**Independent Test**: A fixture pin that no declared package reaches is still classified `edgeless-lock-pin`. A pin merged with a design-tier twin of the same PURL keeps the lock file and gains no wrong file.

**Acceptance Scenarios**:

1. **Given** a lock pin that no declared package names, **When** scanned, **Then** it is unreachable and its orphan reason is still `edgeless-lock-pin`.
2. **Given** a lock pin whose PURL also comes from another source (a design-tier declaration, or the same pin from another reader), **When** the components merge, **Then** the merged component lists the lock file plus exactly the other source's files.
3. **Given** every Haskell corpus target, **When** scanned on `main` and on the branch, **Then** every difference in source files, reachability or orphan reason is listed and explained.

### Edge Cases

- **An unparseable lock file** contributes no components, so it has nothing to record.
- **A lock file reached through the root `stack.yaml`'s `packages:`** (spec 1663's governed directories) still records the root lock, which is the file that pinned the version.
- **A `cabal.project.freeze` range constraint** (not an exact pin) emits no component today, and that does not change.
- **A path outside the scan root:** the recorded path is rootfs-relative, like every other C18 value.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before any code changes, measure every Haskell corpus target (`hpack-*`, `haskell-*`, `nix-closure-*`). List each component whose source type is `hackage-stack-lock` or `hackage-freeze`, with its source files, its reachability from the root, and its orphan reason, on `main`. Then measure the branch the same way. Every difference is explained, and the probe and its output are committed.
- **FR-002**: Each component a `stack.yaml.lock` package entry produces records that lock file as its source.
- **FR-003**: Each component a `cabal.project.freeze` exact pin produces records that freeze file as its source.
- **FR-004**: When more than one lock or freeze file pins the same PURL, the result is one component that lists every such file, sorted and unique.
- **FR-005**: When the same name is pinned at different versions, each version is its own component, and each lists only the files that pinned that version. No file is attributed to a version it did not pin.
- **FR-006**: An orphan reason that changes between `main` and the branch is listed and explained. A pin no declared package reaches is still `edgeless-lock-pin`.
- **FR-007**: When a lock-pinned component merges with another component of the same PURL, the merged component's source files are the union of both inputs' files: the lock file is not lost, and no unrelated file is added. This is the existing behaviour of the resolver's same-PURL source-file union, already covered by its tests (`canonicalize_source_files_by_purl_*_md148` in `resolve/deduplicator.rs`, including `..._three_entries_full_union_md148`). This spec relies on it and does not change it (research R5). The corpus measurement checks that no lock component loses its file.
- **FR-008**: CycloneDX, SPDX 2.3 and SPDX 3 carry identical `waybill:source-files` values for these components (C18 parity).
- **FR-009**: Positive control: a `waybill-fixture-*` project with:
  - a `stack.yaml.lock` pinning two packages, one reached by a declared package and one not;
  - a second lock pinning one of those packages at the same version;
  - a third lock pinning the same name at a different version;
  - a `cabal.project.freeze` exact pin.
  
  Every component records the right file or files. The recording, the multi-lock merge, the different-version split and the freeze path each have a test that a recorded mutation makes fail.
- **FR-010**: Corpus goldens change only where FR-001's measurement explains the change. No invariant declaration moves unless explained. Zero new Cargo dependencies.

### Key Entities

- **Lock pin**: a package name and exact version, together with the lock or freeze file that pinned it.
- **Lock-pinned component**: one PURL, with the sorted set of lock or freeze files that pinned exactly that version.

## Success Criteria *(mandatory)*

- **SC-001**: In hs-opentelemetry, all 6 lock-pinned packages list `stack.yaml.lock`. In lootbox, all 16 list their lock file. 0 lock-pinned components across the Haskell corpus have empty source files.
- **SC-002**: Every difference between the corpus measurement on `main` and on the branch is explained, and 0 are unexplained.
- **SC-003**: The positive control's components each list exactly the expected files: 1 file for a single-lock pin, 2 for the shared pin, 1 for each version of the split pin, and 1 for the freeze pin.
- **SC-004**: Every test of the four behaviours fails under at least one recorded mutation.

## Assumptions

- Recording a source file does not change a component's identity (its PURL), so components are not split or merged differently. FR-001 measures this.
- The hackage versions and PURLs of lock-pinned packages are already correct (spec 1663). This spec changes only their provenance.

## Out of Scope

- #1675, #1682.
- Snapshot components (done in spec 1663).
- Design-tier (`hackage-cabal-design`) components' source files, which already name their manifest.
