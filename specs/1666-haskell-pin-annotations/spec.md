# Feature Specification: Haskell pin annotations say what is true

**Feature Branch**: `1666-haskell-pin-annotations`
**Created**: 2026-11-27
**Status**: Draft
**Input**: #1682 and #1741. C172 `waybill:nixpkgs-version-disagreement` names a version source that is wrong, or makes claims the component contradicts. An unreachable `cabal.project.freeze` pin carries no `waybill:orphan-reason`.

## Background (measured 2026-11-27, `main` at 46506106)

**C172 in the committed corpus goldens**, CycloneDX only:

| Target | Form | Source type | Count |
|---|---|---|---|
| haskell-language-server | lockfile note | `hackage-main-module` | 5 |
| haskell-security-advisories | lockfile note | `hackage-main-module` | 4 |
| nix-closure-slack-web | lockfile note | `hackage-main-module` | 1 |
| haskell-language-server | `"emitted": null` (#1457) | `hackage-cabal-design`, no version | 6 |
| nix-closure-slack-web | `"emitted": null` | `hackage-cabal-design`, version `2.11.14` | 1 (`hspec-discover`) |
| nix-closure-slacklinker | `"emitted": null` | `hackage-cabal-design`, version `2.3.0.0` | 1 (`slack-web`, `vcs_url`) |

- **The lockfile note is always wrong.** All 10 components that carry it are the project's own packages, and their versions come from their own manifests. #1682 also measured 23 such components on hs-opentelemetry (`25f5b2d`), which isn't a nixpkgs-enabled corpus target. The note is a fixed string, written for every Haskell non-boot component whose version differs from nixpkgs (`nix/haskell_packages/mod.rs:1079-1096`).
- **`"emitted": null` contradicts 2 of the 8.** Those two components carry a version the closure supplied, yet the note says the version "was not emitted". The twin-merge test asserts that C172 survives the merge ("C172 stays true"). The disagreement does still hold, because nixpkgs' version violates the project's declared range. The `emitted` field and the note are what went stale.
- **#1741 reproduces on `main`.** The fixture is a `cabal.project` listing `app`, an `app` library depending only on `base`, and a freeze file pinning `waybill-fixture-frozen ==4.0.0`. The scan emits `pkg:hackage/waybill-fixture-frozen@4.0.0` unreachable, with no orphan reason.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A disagreement names its real source (Priority: P1)

An operator comparing a Nix build with a cabal or Stack build reads C172 to learn which two sources disagree about a version, and why the emitted one won.

**Why this priority**: The note is wrong in every measured case.

**Independent Test**: A fixture where a Stack lock pins a version different from nixpkgs, and one where a freeze file does. Each C172 names its own kind of source.

**Acceptance Scenarios**:

1. **Given** a dependency pinned by a `stack.yaml.lock` at a version different from nixpkgs, **When** scanned with nixpkgs enrichment, **Then** C172 says the version came from the Stack lock file.
2. **Given** the same pinned by `cabal.project.freeze`, **When** scanned, **Then** C172 says it came from the freeze file.
3. **Given** any C172, **When** read, **Then** its note never names a source that did not produce the emitted version.

---

### User Story 2 - The project's own packages are not a disagreement (Priority: P1)

The same operator doesn't see a disagreement on the project's own packages. For those packages the comparison is between the working tree and the project's last nixpkgs release, which is expected to differ.

**Why this priority**: All 10 lockfile-note components in the corpus are this case, so it is the bulk of the noise.

**Independent Test**: A main module whose manifest version differs from nixpkgs' entry under the same name carries no C172.

**Acceptance Scenarios**:

1. **Given** a main module or workspace member whose version differs from nixpkgs, **When** scanned, **Then** it carries no C172.
2. **Given** the corpus, **When** measured on the branch, **Then** the 10 lockfile-note main-module components carry no C172. The document-scope disagreement count matches what remains.

---

### User Story 3 - A disagreement agrees with its component after the closure merge (Priority: P2)

When the closure supplies a version for a component whose nixpkgs version violated the declared range, C172 doesn't say the version "was not emitted".

**Why this priority**: The annotation contradicts the component it sits on. It affects 2 corpus components.

**Independent Test**: A design twin, whose declared range nixpkgs' version violates, merged into a closure member. C172 is removed or rewritten per FR-005.

**Acceptance Scenarios**:

1. **Given** hspec-discover in `nix-closure-slack-web`, built at 2.11.14, which is nixpkgs' own version, **When** scanned on the branch, **Then** it carries no C172, and C221 still records the range violation.
2. **Given** slack-web in `nix-closure-slacklinker`, built at 2.3.0.0 while nixpkgs has 2.2.0.0, **When** scanned, **Then** C172's `emitted` is `2.3.0.0` and the note names the closure.
3. **Given** a component the closure did not supply (the 6 in haskell-language-server), **When** scanned, **Then** its C172 is unchanged.

---

### User Story 4 - An unused freeze pin is explained (Priority: P2)

A `cabal.project.freeze` pin that no declared package reaches carries an orphan reason, as its Stack counterpart does.

**Why this priority**: The unexplained-orphan invariant (I7) can't see this shape today, because no corpus target has a freeze file.

**Independent Test**: The #1741 fixture yields `edgeless-lock-pin` on the frozen pin.

**Acceptance Scenarios**:

1. **Given** the #1741 fixture, **When** scanned, **Then** `waybill-fixture-frozen@4.0.0` is unreachable with orphan reason `edgeless-lock-pin`.
2. **Given** a freeze pin a declared package does reach, **When** scanned, **Then** it is reachable and carries no orphan reason.

### Edge Cases

- **A component pinned by both a lock and a freeze file**: since spec 1665 it is one component listing both. The note names both kinds.
- **A version from the closure that equals nixpkgs'**: there's no disagreement, so no C172.
- **A workspace member that is not a root main module**: skipped, like any main module.
- **A boot library**: already skipped; unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before changing any code, measure every Haskell corpus target (`hpack-*`, `haskell-*`, `nix-closure-*`) and the #1741 fixture on `main`. The corpus numbers come from the goldens CI produced with nixpkgs enrichment (research R1). #1682's 23 hs-opentelemetry components are cited as prior measurement, not re-measured, because that target isn't nixpkgs-enabled and an offline scan emits no C172. For each component that carries C172, record its value, source type and version source. For each unreachable component, record its orphan reason. Measure the branch the same way. Commit the probe and its output, and explain every difference.
- **FR-002**: C172's note names the source that produced the emitted version: the freeze file, the Stack lock file, or the closure. The note is derived from that source, not from a fixed string.
- **FR-003**: Main modules and workspace members carry no C172. A project's working tree being ahead of, or behind, its own last nixpkgs release is not two sources disagreeing about what was built.
- **FR-004**: The document-scope disagreement count (the enrichment summary) counts only the components that still carry C172.
- **FR-005**: After the closure merge, a C172 in the `"emitted": null` form whose component now carries a version is recomputed:
  - if the built version equals C172's `nixpkgs` version, C172 is removed, because nothing disagrees and C221 `waybill:requirement-unsatisfied` records the range violation;
  - otherwise `emitted` becomes the built version and the note says the closure built that version.
  
  Research R4 measured one corpus component of each kind.
- **FR-006**: An unreachable `hackage-freeze` component's orphan reason is `edgeless-lock-pin`, the same as for `hackage-stack-lock`. Both kinds are a project-local pin that no declared package depends on, and one reason keeps a consumer from needing to know two names for one fact.
- **FR-007**: A freeze file is tested against I7: an integration test applies I7's rule (an unreachable component must carry a reason) to a freeze-file fixture, in all three formats (research R7).
- **FR-008**: C172's value has the same shape in CycloneDX, SPDX 2.3 and SPDX 3. If its shape changes, the catalog row and the extractor change in the same commit.
- **FR-009**: Positive control: `waybill-fixture-*` projects covering, in turn:
  - a lock-sourced disagreement;
  - a freeze-sourced disagreement;
  - a main module that differs from nixpkgs;
  - a closure-supplied version over a range nixpkgs violates;
  - the #1741 unused freeze pin.
  
  Recorded mutations kill the tests for the note derivation, the main-module skip, the post-closure rewrite and the freeze arm.
- **FR-010**: Corpus goldens change only where FR-001's measurement explains the change. No invariant declaration moves unless explained. Zero new Cargo dependencies.

### Key Entities

- **Version source**: what produced a component's emitted version, one of the freeze file, the Stack lock file or the closure. The package's own manifest is also a source, but it never appears in a note, because main modules carry no C172 (FR-003).
- **Disagreement**: an emitted version that differs from the pinned nixpkgs revision's version of the same package. It is recorded with its source.

## Success Criteria *(mandatory)*

- **SC-001**: 0 C172 notes in the corpus name a source that didn't produce the version. Today it is 10 of 10 lockfile notes. The corpus has no lock- or freeze-sourced disagreement, so the derived note is proved by unit tests, and mutation M1 must kill one of them.
- **SC-002**: 0 main modules carry C172. Today it is 10 in the corpus.
- **SC-003**: 0 C172 values contradict their component's version. Today it is 2.
- **SC-004**: 0 unreachable freeze pins lack an orphan reason, in the fixture and in I7.
- **SC-005**: Each of the four behaviours fails under at least one recorded mutation, and every corpus difference between `main` and the branch is explained.

## Assumptions

- After spec 1665, the lock and freeze components carry their source type and source path, so the version source can be read from the component itself.
- The closure-supplied case is recognisable after the twin merge, from the component's version and tier.

## Out of Scope

- Changing which version wins.
- Boot-library handling.
- #1675 (closed).
