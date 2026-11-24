# Feature Specification: `root-only` keeps the packages the root builds, and records what it dropped

**Feature Branch**: `1652-root-only-haskell`
**Created**: 2026-11-24
**Status**: Draft
**Input**: #1676. `--project-discovery=root-only` drops every nested Haskell package, including ones the root depends on, while the document still reports them as read and can claim completeness. Refs #1045, #1673, #1675.

## Background (measured)

**Reproduced at `main` `317fbae`** on a synthetic tree: a root `package.json`, plus two nested hpack packages, `waybill-fixture-a` and `waybill-fixture-b`, where `b` depends on `a`.

| `--project-discovery` | `pkg:hackage` components | `waybill:graph-completeness` | C200 `manifests-read` |
|---|---|---|---|
| `all` (default) | 4 | `partial` | 2 |
| `root-only` | **0** | **`complete`** | 2 |

**#1676's measurements** at `33c445f6`:
- **A root `package.yaml` depending on `waybill-fixture-a`:** `root-only` keeps the root and `base`, but drops `waybill-fixture-a` and the root's edge to it.
- **A private monorepo:** 85 `package.yaml` were read, and zero Haskell components were emitted.

**How `root-only` decides, from the code** (`generate/project_discovery/filter.rs`, milestone 220):
1. **Roots:** every main module; the root-level ones are in scope.
2. **Projection:** each in-scope root is projected through its dependency edges by milestone 215's `project_for_root`, which `--split` also uses, and which demotes sibling main modules.
3. **Follow-up:** the result is extended by directory, using `waybill:workspace-member`.
4. **Filter:** everything not reached is dropped.

**`waybill:workspace-member` is not set per reader.** #1676 says the Haskell reader never sets it. In fact milestone 176 stamps it on every component, as the directory of the component's own manifest.
- **A cargo workspace member is kept** because its only evidence is the root `Cargo.lock`, so its directory is the root's.
- **A nested hpack or cabal package names its own subdirectory,** so nothing ties it to the root, even when a root `cabal.project` or `stack.yaml` lists it.

The plan establishes each reader's exact behaviour before relying on it.

**Measured in the plan (research R1):** the projection is not the cause. A root Haskell package is never treated as root-level, because its manifest path (`package.yaml`, `*.cabal`) is kept as a file name. So every Haskell corpus target falls back to a full scan. The projection already follows edges across main modules.

**What reaches the document today:** C140 `waybill:project-discovery-mode` records the mode. The count of dropped projects (`nested_projects_ignored`) appears only in a log line. Completeness is computed on what remains, so a document emptied of its Haskell side can read `complete`.

**The corpus never exercises this.** Today the four hpack corpus repositories give identical output under `root-only`, because none has a root main module, so `root-only` falls back to full scope (`corpus_harness_195/manifest.rs`, #1677). No corpus target is scanned with `--project-discovery`.

## Clarifications

### Session 2026-11-24

- Q: Does a root `cabal.project` or `stack.yaml` with `packages:`, and no root package, set `root-only`'s scope? → A: Yes. It is a root declaration: its listed packages become in-scope roots, and unlisted nested packages are dropped and recorded. #1045's fall-back applies only when there is neither a root main module nor a root declaration.
- Q: Does FR-002 (reachability across main modules) apply to `--project-discovery=strict`? → A: Yes, FR-002 applies to `strict`: a nested main module reached through dependency edges is one of the root's own dependencies and rides along. FR-003 does not apply to `strict`: declared members drop, as cargo members do, and a root declaration does not set `strict`'s scope.
- Q: What does the drop record name? → A: The count, the reason, and the sorted main-module PURLs of the dropped projects, as one document-level record.
- Q: How does C200 show a manifest whose package was dropped by scope? → A: The manifest stays under `manifests-read`, and C200 gains a sorted `manifests-dropped-by-scope` list, present only when non-empty.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A package the root builds stays in the root's SBOM (Priority: P1)

An operator scans a monorepo with `--project-discovery=root-only` to get the SBOM of the root project alone. A nested package that the root project depends on, directly or through other packages, is part of that build, so it appears, with the root's edge to it.

**Why this priority**: today such a package is dropped along with the edge. The root's SBOM then omits code that goes into the root's build, which is the defect `root-only` must never introduce.

**Independent Test**: a root `package.yaml` depending on a nested `waybill-fixture-a`, which depends on a nested `waybill-fixture-c`. Under `root-only`, both nested packages and both edges are present.

**Acceptance Scenarios**:

1. **Given** a root main module that depends on a nested main module, **When** scanned with `root-only`, **Then** the nested main module, its own dependencies, and the root's edge to it are emitted.
2. **Given** the same through a chain (the root depends on `a`, which depends on `c`), **When** scanned, **Then** `a` and `c` are both kept.
3. **Given** a nested main module that no in-scope root reaches, **When** scanned with `root-only`, **Then** it is dropped, as today.
4. **Given** the same in another ecosystem (an npm root depending on a nested npm package by path), **When** scanned, **Then** the same rule applies. The rule is about the dependency graph, not Haskell.

---

### User Story 2 - A package the root project declares is part of it (Priority: P1)

A Haskell monorepo declares its packages in a root `cabal.project` (`packages:`) or a root `stack.yaml` (`packages:`), the way a cargo workspace declares its members. Under `root-only`, those packages are members of the root project and are kept, as cargo workspace members are.

**Why this priority**: it is the Haskell monorepo's own statement of what the project is. Without it, a monorepo whose packages don't depend on each other loses all of them, as the private monorepo did.

**Acceptance Scenarios**:

1. **Given** a root `cabal.project` listing `local-packages/*`, and a root manifest of another ecosystem, **When** scanned with `root-only`, **Then** every listed package is kept with its dependencies.
4. **Given** a root `cabal.project` listing `local-packages/*` and no root package of any ecosystem, **When** scanned with `root-only`, **Then** the listed packages are the in-scope roots: they are kept with their dependencies, a nested package outside the list is dropped and recorded, and the scan does not fall back to full scope.
2. **Given** a root `stack.yaml` listing two package directories, **When** scanned, **Then** both are kept.
3. **Given** a nested package that the root `cabal.project` does **not** list, and that no in-scope root depends on, **When** scanned, **Then** it is dropped.

---

### User Story 3 - The document says what `root-only` dropped (Priority: P1)

A consumer reading a `root-only` SBOM can tell how many projects were left out and why, and is never told the graph is complete when manifests that were read were discarded.

**Why this priority**: a silent drop with a `complete` verdict is a confident wrong answer. That is worse than an incomplete one, and it is what the reproduction shows.

**Acceptance Scenarios**:

1. **Given** a `root-only` scan that drops nested projects, **When** the document is read, **Then** it states how many were dropped, that the reason was project-discovery scope, and the main-module PURL of each dropped project, sorted, in all three formats.
2. **Given** that scan, **When** the completeness verdict is read, **Then** it is not `complete`, and its reason names the scope.
3. **Given** a manifest the Haskell reader read whose package `root-only` then dropped, **When** C200 is read, **Then** it stays under C200's `manifests-read`, and also appears in C200's sorted `manifests-dropped-by-scope` list. A scan that drops no hpack manifest has no such list, so its C200 value is unchanged.
4. **Given** a `root-only` scan that drops nothing (every nested project is reached or a member), **When** the document is read, **Then** no drop is recorded, and completeness is computed as today.

---

### User Story 4 - Nothing else changes (Priority: P2)

**Acceptance Scenarios**:

1. **Given** any scan under the default `--project-discovery=all`, **When** compared with `main`, **Then** it is byte-identical.
2. **Given** a scan with neither a root-level main module of any ecosystem nor a root declaration (`cabal.project` or `stack.yaml` with `packages:`), **When** run with `root-only`, **Then** it falls back to full scope, as today (#1045).
3. **Given** `--split`, which shares the projection, **When** run on the same fixtures, **Then** its output is unchanged.
4. **Given** `--project-discovery=strict`, **When** run, **Then** its documented behaviour holds: workspace members drop, and the root project's own dependencies ride along. A nested main module the root depends on is one of those dependencies and is kept (FR-002). A package the root only declares is a member and drops (FR-003 does not apply). With no root main module, `strict` falls back to full scope as today, because a root declaration does not set its scope.

### Edge Cases

- **A nested package depended on only by another nested package that is itself dropped** is dropped too. Reachability starts from in-scope roots.
- **A dependency cycle between nested packages** terminates, and keeps both.
- **The root depends on a nested package through a versionless, declared reference** (`waybill-fixture-a` with no version) while the nested main module carries a version. The plan measures whether the edge targets the main module or a separate declared component, and the rule covers both.
- **`cabal.project` `packages:` entries** can be globs (`local-packages/*`), directories, or `.cabal` file paths. The plan matches the forms the existing #1032 reader already parses, and reuses it.
- **A root `cabal.project` without `packages:`** declares nothing. Under #1032 it governs nothing, and the same applies here.
- **A Nix-built monorepo** (`callCabal2nix` per package) may declare its packages only in Nix. That is out of scope; #1676 flags it as inference.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (measure first)**: Before the design is fixed, the work MUST establish three things from the code and from scans of the synthetic fixtures:
  - why `root-only` drops a nested main module the root depends on: the projection's sibling demotion, the edge's target, or both;
  - how each reader's components get `waybill:workspace-member`, and what makes a cargo member share the root's directory;
  - for the corpus targets with nested main modules, how many components `root-only` keeps before and after the change. They are scanned locally with `root-only`, once as they are, which under the Clarification uses a root `cabal.project` or `stack.yaml` as the scope, and, where a target has neither, with a root manifest added so that `root-only` does not fall back.

  The counts are recorded.
- **FR-002 (reachability crosses main modules)**: Under `root-only` and `strict`, a main module reachable from an in-scope root main module through dependency edges MUST be kept with its own dependencies and the edges between them, in any ecosystem. `--split`'s output MUST NOT change.
- **FR-003 (declared Haskell members)**: Under `root-only` only (not `strict`, which drops members), a package declared by the root's `cabal.project` `packages:`, or by the root's `stack.yaml` `packages:`, MUST be treated as a member of the root project, as a cargo workspace member is. The declaration forms are the ones the existing `cabal.project` reader parses (#1032). A root declaration sets the scope on its own: when there is no root main module, its declared packages are the in-scope roots, and a nested package it does not list is dropped and recorded (FR-004).
- **FR-004 (record the drop)**: When `root-only` or `strict` drops one or more projects, the document MUST carry one document-level record stating how many, that project-discovery scope was the reason, and the sorted main-module PURLs of the dropped projects, identically in CycloneDX, SPDX 2.3 and SPDX 3. Use a standards-native construct first (Principle V); otherwise an existing row, extended compatibly; only failing both, a new row with its parity extractor.
- **FR-005 (completeness)**: When `root-only` or `strict` drops projects whose manifests were read, `waybill:graph-completeness` MUST NOT be `complete`, and its reason MUST name the scope.
- **FR-006 (C200 coherence)**: A manifest whose package was dropped by project-discovery scope MUST stay in C200's `manifests-read` and MUST also be listed, sorted, in a new C200 field, `manifests-dropped-by-scope`. The field is omitted when empty, so every existing C200 value is byte-identical. The C200 parity extractor and catalog row are extended in the same change. C200 covers hpack manifests (`package.yaml`) only. A dropped package read from a `.cabal` file has no per-manifest record, and is named in the drop record (FR-004).
- **FR-007 (unchanged otherwise)**: Default `all` scans MUST be byte-identical. `root-only`'s fall-back to full scope (#1045) MUST be kept for scans with neither a root main module nor a root declaration, and `--split` unchanged.
- **FR-008 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **In-scope root**: a root-level main module under `root-only`.
- **Reached main module**: a main module reachable from an in-scope root through dependency edges.
- **Root declaration**: a root `cabal.project` or `stack.yaml` with a `packages:` list. It sets `root-only`'s scope even with no root package.
- **Declared member**: a package listed by a root declaration.
- **Drop record**: one document-level record of how many projects `root-only` or `strict` left out, why, and the sorted main-module PURL of each.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (fixtures)**: On synthetic `waybill-fixture-*` trees, each acceptance scenario of User Stories 1 to 3 holds in all three formats, and `waybill sbom parity-check` reports no gap. The trees are:
  - a root `package.json` with a nested hpack pair;
  - a root `package.yaml` depending on a nested package, and on a chain;
  - a root `cabal.project` listing local packages, with and without a root manifest of another ecosystem;
  - a root `stack.yaml` listing packages;
  - an npm root depending on a nested npm package.
- **SC-002 (the reproduction)**: The synthetic tree from the Background, under `root-only`, gives the root's dependencies and a recorded drop, and is not `complete`. The nested pair is dropped, because the `package.json` root neither depends on nor declares it. With a root `cabal.project` listing them, they are kept.
- **SC-003 (positive control)**: A `root-only` scan that drops a project records the drop. The same scan with the drop rule removed (mutation) does not record it, and the test fails.
- **SC-004 (nothing else)**: Every corpus golden is byte-identical; no corpus target uses `--project-discovery`. `--split` output on the fixtures is unchanged. Default-mode output is byte-identical to `main` on the fixtures.
- **SC-005 (the counts)**: FR-001's before-and-after counts are recorded for every corpus target with nested main modules.
- **SC-006 (mutations)**: Each rule is mutated, and each mutation is killed:
  - reachability across main modules, under `root-only` and `strict` separately;
  - declared members, from `cabal.project` and `stack.yaml` separately;
  - the drop record;
  - completeness;
  - C200 coherence.

## Assumptions

- **`root-only`'s purpose** is "the SBOM of the root project": what it builds and declares, as milestone 220 describes. Dropping independent nested projects stays correct.
- **`strict` keeps its documented semantics** (Clarifications): a nested main module reached through edges is one of the root's own dependencies, and a declared package is a member.
- **#1676's private monorepo** is the motivation, not a measurement surface here. Its figures (85 read, 0 emitted) are cited, not re-measured.
- **A Nix-only declaration of a monorepo's packages** is out of scope.
