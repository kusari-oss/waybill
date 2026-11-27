# Feature Specification: Nix-built Haskell packages count as project members

**Feature Branch**: `1664-nix-built-members`
**Created**: 2026-11-27
**Status**: Draft
**Input**: #1731. A Haskell package that only a Nix expression builds is not a project member, so it and its dependencies are dropped.

## Background (measured)

In mercury/hs-temporal-sdk at `d24313e37a009cc4121012e509e16a3e749811d3` (corpus target `hpack-hs-temporal-sdk`), scanned with `--offline` and default flags:

- `cabal.project` says `packages: */*.cabal`, which matches one directory level. `tools/tix-to-markdown/` is two levels down, and the repository has no `stack.yaml`.
- `nix/packages/tix-to-markdown.nix` builds the package. `nix/overlays/haskell/hs-temporal-sdk.nix`, `nix/packages/coverage.nix` and `nix/utils/haskell.nix` also name it.
- Before spec 1662, `main` emitted `pkg:hackage/tix-to-markdown@2025.10.1.0` and `pkg:hackage/hpc`. After spec 1662, neither is emitted. `hpc` goes because `tix-to-markdown` is its only dependent.

Project membership (#1032, extended by spec 1662) is currently the union of the governing `cabal.project` and a sibling `stack.yaml`. A Nix build is not one of the declarations it reads.

## Clarifications

### Session 2026-11-27

- Q: Must the default scan (no `--nix-closure`) restore tix-to-markdown and hpc? → A: Yes. Static recognition (a) is required; the closure tier (b) may only be added on top of it, never instead.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A package that Nix builds appears in the SBOM (Priority: P1)

An operator scans a Haskell repository in which one package is built only by a Nix expression that names its directory literally. The SBOM lists that package and its dependencies, and says the Nix file is why it was included.

**Why this priority**: Without this, the SBOM silently omits shipped code and its dependencies. That is the #1731 defect.

**Independent Test**: Scan the positive-control fixture. The Nix-built package is present, and its `waybill:source-files` names the Nix file.

**Acceptance Scenarios**:

1. **Given** a `cabal.project` glob that misses a two-level package, and a Nix file that builds that package from a literal relative path in a form the measurement found, **When** the repository is scanned, **Then** the package is a member, its dependencies are emitted, and the record says how it was admitted.
2. **Given** hs-temporal-sdk at the pinned commit, **When** it is scanned, **Then** `tix-to-markdown` and `hpc` are emitted again.

---

### User Story 2 - Nothing else becomes a member (Priority: P1)

The same operator gets no extra members from Nix references that do not build a package, such as fixtures, vendored trees or comments.

**Why this priority**: A false member puts components into the SBOM that nobody ships. That is worse than the omission this spec fixes.

**Independent Test**: In the positive control, one package is named only from a non-build attribute, and one only through an interpolated path. Neither is a member.

**Acceptance Scenarios**:

1. **Given** a directory named only in a comment or a non-build attribute, **When** scanned, **Then** it is not a member.
2. **Given** a directory reached only through an interpolated or computed path, **When** scanned, **Then** it is not a member, and the measurement reports the count of such references.
3. **Given** every Haskell corpus target, **When** scanned on the branch, **Then** every newly admitted directory is listed per target, each with its reason.

---

### User Story 3 - The choice of mechanism is evidenced (Priority: P2)

A maintainer can see from the committed measurement why this is answered from Nix source or from the `--nix-closure` tier, and what the rejected option would have cost.

**Independent Test**: `research.md` contains the step-1 table and the decision, and the probe that produced the table is committed so it can be rerun.

### Edge Cases

- A package that `cabal.project` or `stack.yaml` already admits is unchanged. The Nix rule only adds members.
- A path is resolved against the directory of the `.nix` file that contains it, as Nix does. For the root `flake.nix` that directory is the flake root, so there is no second resolution rule.
- A Nix-referenced path with no `.cabal` or `package.yaml` in it admits nothing.
- A package admitted by more than one Nix file records all of those files.
- A repository with no Haskell manifests: no behaviour change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before any code changes, measure every Haskell corpus target (`hpack-*`, `haskell-*`, `nix-closure-*`). List each `.cabal`/`package.yaml` directory the current membership rule rejects. Classify each by whether a repository Nix file references it, and in which form: `callCabal2nix`, `callCabal2nixWithOptions`, `callPackage`/`haskellPackages.callPackage` on a path, cabal2nix output, haskell.nix `project`/`cabalProject`, interpolated or computed, or none. Report the count in each class. The probe and its output are committed.
- **FR-002**: The default scan (no `--nix-closure`) MUST admit Nix-built members by (a) static recognition of literal path references in Nix source, added to the membership union. Whether to also add (b), membership derived from evaluated derivation sources in the `--nix-closure` tier, is decided from FR-001's data; record that decision and its measured cost in `research.md`. (b) never replaces (a).
- **FR-003**: Only the forms FR-001 observed are recognised. A path counts only when it is a literal, resolved against the directory of the `.nix` file that contains it. Interpolated and computed paths are never recognised.
- **FR-004**: A member admitted by Nix records the admitting Nix file(s) in `waybill:source-files` and states the admission route as a documented value in `sbom-format-mapping.md`.
- **FR-005**: Membership that `cabal.project` or `stack.yaml` grants is unchanged. The order of precedence among the three sources is documented.
- **FR-006**: A directory referenced by Nix only outside a recognised build form (a comment, or a path in a non-build attribute) is not a member.
- **FR-007**: No Nix evaluation happens at scan time unless `--nix-closure` is given.
- **FR-008**: Positive control: a `waybill-fixture-*` repository containing (i) a two-level package built by `callCabal2nix` on a literal relative path, (ii) a package referenced only from a non-build attribute, and (iii) one referenced only through an interpolated path. Only (i) is a member. Every recognised form and every exclusion has a test, and each test has been shown by mutation to fail.
- **FR-009**: Every corpus golden difference is explained by FR-001's table. No invariant declaration moves except those the measurement explains.
- **FR-010**: Zero new Cargo dependencies.

### Key Entities

- **Membership source**: one of `cabal.project`, `stack.yaml`, or Nix build reference, with the file that granted membership.
- **Nix build reference**: a recognised form, the literal path it names, the resolved package directory, and the Nix file and location it came from.

## Success Criteria *(mandatory)*

- **SC-001**: In `hpack-hs-temporal-sdk`, scanned with default flags (no `--nix-closure`), `pkg:hackage/tix-to-markdown@2025.10.1.0` and `pkg:hackage/hpc` are present again.
- **SC-002**: Across all Haskell corpus targets, every newly admitted directory appears in the FR-001 table with a recognised form, and there are 0 unexplained additions.
- **SC-003**: The positive control admits exactly 1 of its 3 candidate packages.
- **SC-004**: Every recognition and exclusion test fails under at least one recorded mutation.
- **SC-005**: Scans without `--nix-closure` invoke `nix` 0 times.

## Assumptions

- Literal-path references cover the observed cases. FR-001 confirms or refutes this; cases (a) cannot reach are counted and reported, and may motivate adding (b) under `--nix-closure`.
- The pinned corpus commits are unchanged.

## Out of Scope

- #1675, #1682, #1736.
- Evaluating Nix outside `--nix-closure`.
- Membership via other build systems (Bazel, Make).
