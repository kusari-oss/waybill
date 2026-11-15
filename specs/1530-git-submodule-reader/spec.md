# Feature Specification: Git submodules as components

**Feature Branch**: `1530-git-submodule-reader`
**Created**: 2026-11-15
**Status**: Draft
**Input**: Issue #1515 — git submodules are never emitted. waybill has no reader for `.gitmodules` or gitlink commits, so a C/C++ super-build that pins its dependencies as submodules yields none of them. Milestone 105 designed this reader as User Story 6; its tasks (T083–T085) were never done. Reuse that design and record what no longer holds.

## Clarifications

### Session 2026-11-15

- Q: When a submodule and another reader name the same library under different PURLs (a Conan `pkg:conan/abseil@20240722.0` against a submodule `pkg:github/abseil/abseil-cpp@<sha>`, or a CMake `GIT_TAG v1.2.3` against the submodule's commit of the same repository), what happens? → A: **They merge only on an identical canonical PURL.** Different PURLs stay two components. There is no guessed mapping between a package-manager name and a repository, or between a tag and a commit. The `dedup_collision` test is rewritten to assert this (FR-009).
- Q: What counts as "used" for C57 `waybill:build-reference`? → A: **The milestone-105 `find_package` name match, or a CMake path reference.** A path reference is `add_subdirectory(<path>)`, or an `ExternalProject_Add` / `FetchContent_Declare` `SOURCE_DIR` naming the submodule's path. C57's documented definition is widened to say so (FR-008).

## Background

A git submodule is the most exact pin a C/C++ project can give a dependency: the super-project's tree records the dependency's repository URL in `.gitmodules` and its exact commit as a *gitlink*, a tree entry of mode `160000`. Today waybill emits nothing for either. Issue #1515's reproduction (a project with one submodule, scanned both checked out and as a fresh clone with the submodule uninitialised) gives zero non-file components in both cases.

Milestone 105 (spec `specs/105-cpp-ecosystem-expansion/`, User Story 6, FR-008/008a/009, FR-015/016) designed this reader and shipped the surrounding pieces, which still wait on it:

- the `git-submodule` value of `waybill:source-mechanism` (catalog row C55);
- `waybill:also-detected-via` (C56) and the dedup precedence that ranks `git-submodule` as a filesystem-derived signal below manifest declarations;
- `waybill:build-reference` (C57), with parity extractors in all three formats, and the `find_package` target collector it classifies against;
- the `#[ignore]`d test `dedup_collision_emits_also_detected_via_after_us6` and its `dedup_collision` fixture.

### What no longer holds from milestone 105 US6

| US6 said | Why it no longer holds | This spec |
| --- | --- | --- |
| The commit is read from `.git/modules/<path>/HEAD`; "no subprocesses". | That file exists only once a submodule is initialised. On a fresh clone it does not, yet the pinned commit is already recorded in the super-project's tree. | The commit comes from the gitlink in the super-project's recorded tree (FR-003). |
| An uninitialised submodule gets `version: "unknown"` and `waybill:resolver-step: "uninitialized-submodule"`. | The gitlink gives the exact commit whether or not the submodule is initialised. | Initialised or not, the component carries its gitlink commit (FR-003). |
| One component per `.gitmodules` entry. | `.gitmodules` can list paths with nothing behind them. On one large C++ tree the `.gitmodules` files listed 62 paths but only 36 gitlinks existed: a monorepo's top-level `.gitmodules` repeated its projects' entries, and one vendored project carried a `.gitmodules` with no gitlink behind it (issue #1515). | One component per gitlink (FR-002). |
| Only the `.gitmodules` at the scan root. | Initialised submodules can carry their own submodules. | Initialised submodules are read recursively (FR-006). |
| Non-GitHub URLs give `pkg:git+https://<url>@<sha>`. | `git+https` is not a PURL type. | A generic PURL with a `vcs_url` qualifier (FR-004). The elixir reader already uses this exact form for git dependencies, and the cocoapods, composer and dart readers use the same qualifier on their own types. |
| The `dedup_collision` fixture expects `pkg:conan/abseil@…` and `pkg:github/abseil/abseil-cpp@…` to merge after an unspecified "canonicalization". Its own README notes the two PURLs "technically wouldn't dedup". | No such canonicalization was ever specified, and any name-to-repository mapping would be a guess. | Only identical canonical PURLs merge (FR-009). The fixture's test asserts two components. |
| C57 counts a submodule as used only when a `find_package` target matches its last path segment (FR-008a). | Super-builds consume submodules by path, through `add_subdirectory(<path>)` or `SOURCE_DIR`, which a name match cannot see. | A path reference also counts (FR-008). |

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Every pinned submodule becomes a component, even on a fresh clone (Priority: P1)

An engineer scans a C/C++ super-project that pins its third-party code as git submodules. Their CI clones the repository without initialising submodules, so the scan sees `.gitmodules` and the gitlinks but none of the submodule contents. They expect one component per submodule, identified by its repository and pinned to the exact commit the super-project records.

**Why this priority**: This is the whole of #1515. A submodule pin is exact provenance (repository plus commit), and today it is invisible. A fresh, uninitialised clone is the common CI case, which is exactly where the milestone-105 design would have reported `unknown`.

**Independent Test**: Build a super-project fixture with a local `file://` repository added as a submodule, clone it without initialising submodules, and scan both the original and the clone. Each must give one component pinned to the gitlink commit.

**Acceptance Scenarios**:

1. **Given** a super-project with one submodule whose URL is a GitHub repository, **When** it is scanned with the submodule checked out, **Then** exactly one component is emitted as `pkg:github/<owner>/<repo>@<commit>`, where `<commit>` is the gitlink's full commit, carrying `waybill:source-mechanism: git-submodule` and `waybill:build-reference`.
2. **Given** a fresh clone of that super-project with the submodule not initialised, **When** it is scanned, **Then** the same component is emitted with the same commit.
3. **Given** a submodule whose URL is not a GitHub repository, **When** it is scanned, **Then** the component is `pkg:generic/<name>@<commit>?vcs_url=git+<url>`.
4. **Given** a `.gitmodules` whose URL carries credentials (`https://user:token@host/…`), **When** it is scanned, **Then** no emitted string contains the credentials, and a warning names the `.gitmodules` file with the URL redacted.

---

### User Story 2 — Only real submodules are counted, including nested ones (Priority: P1)

An engineer scans a monorepo whose `.gitmodules` files list more paths than the repository actually pins: a top-level file repeats its projects' entries, and a vendored project carries a `.gitmodules` with nothing behind it. They expect exactly the submodules the repository pins, plus the submodules of any initialised submodule, each once.

**Why this priority**: Counting `.gitmodules` lines over-reports. On the tree measured for #1515 it would have reported 62 components for 36 pins, which is false provenance for 26 entries.

**Independent Test**: A fixture whose `.gitmodules` lists three paths, only two of which have gitlinks, plus one initialised submodule that itself pins a submodule. The scan gives exactly three components.

**Acceptance Scenarios**:

1. **Given** a `.gitmodules` entry whose path has no gitlink in the recorded tree, **When** it is scanned, **Then** no component is emitted for it, and the skip is counted in a diagnostic.
2. **Given** two `.gitmodules` files that both list the same path, **When** it is scanned, **Then** one component is emitted for that path.
3. **Given** an initialised submodule that pins its own submodule, **When** it is scanned, **Then** the nested submodule is also emitted, pinned to the commit the *inner* repository records.
4. **Given** an uninitialised submodule, **When** it is scanned, **Then** its own submodules are not enumerated, because their gitlinks are not present, and nothing is guessed.

---

### User Story 3 — A submodule another reader also finds does not double-report (Priority: P2)

An engineer's super-build pins a library as a submodule and also names it elsewhere: a CMake `FetchContent_Declare` of the same repository, a Conan requirement, or a manifest inside the submodule's own directory that another reader reads. They expect the evidence to agree, not to see the same library reported twice under unrelated identities.

**Why this priority**: Milestone 105 built the precedence and `waybill:also-detected-via` machinery for exactly this case, but left open how a submodule's identity relates to other readers' identities for the same library.

**Independent Test**: The `dedup_collision` fixture, made a real git repository with a gitlink. Its `#[ignore]` gate is removed, and the test asserts FR-009: two components, Conan and submodule, because their PURLs differ.

**Acceptance Scenarios**:

1. **Given** a submodule and a `FetchContent_Declare(… GIT_REPOSITORY <same repo> GIT_TAG <same commit>)`, **When** it is scanned, **Then** one component is emitted. The manifest declaration wins per milestone-105 precedence, and `git-submodule` appears in its `waybill:also-detected-via`.
2. **Given** a submodule and another reader's component for the same library under a different PURL (a Conan requirement, or a CMake fetch at a tag rather than the commit), **When** it is scanned, **Then** both components are emitted, neither absorbs the other, and neither carries the other's mechanism in `also-detected-via`.
3. **Given** the same tree scanned repeatedly, **When** the outputs are compared, **Then** they are byte-identical, including the `also-detected-via` order.

---

### Edge Cases

- **No repository metadata.** A source archive or exported tree has `.gitmodules` but no repository data. Nothing records a commit, so no submodule component is emitted. One warning names each `.gitmodules` file and its entry count.
- **Relative submodule URLs** (`url = ../dep.git`) resolve against the super-project's own remote URL when one is recorded. Otherwise the component keeps the relative URL in `vcs_url`, with the gitlink commit as its version.
- **SSH and scp-style URLs** (`git@github.com:owner/repo.git`, `ssh://git@github.com/owner/repo`) identify GitHub repositories the same way `https://` URLs do. A `.git` suffix is not part of the repository name.
- **A gitlink with no `.gitmodules` entry**, such as a submodule added and then removed from `.gitmodules`, has a commit but no URL. It is emitted as `pkg:generic/<path-basename>@<commit>` without a `vcs_url`, with a diagnostic.
- **The scan root is a subdirectory of a working tree.** Only gitlinks under the scan root are emitted.
- **The checked-out commit differs from the gitlink**, for example a submodule moved locally and not committed. The gitlink is what the super-project pins and is what is emitted. The difference is recorded as a diagnostic, not as a second component.
- **The `git` tool is unavailable** on the host. The reader emits nothing and warns once. It never falls back to guessing from `.gitmodules`.
- **Hostile `.gitmodules` content.** A path that escapes the scan root (`../`, absolute) or a URL that looks like a command-line option is skipped with a warning, and nothing from it reaches a subprocess argument.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan MUST read every `.gitmodules` file in the super-project and in each initialised submodule (FR-006), for each submodule's `path` and `url`.
- **FR-002**: A component MUST be emitted for each gitlink, a tree entry recording a submodule commit, under the scan root. It MUST NOT be emitted for a `.gitmodules` entry that has no gitlink. A path listed by several `.gitmodules` files MUST give one component.
- **FR-003**: The component's version MUST be the full commit recorded by the gitlink in the super-project's tree. It MUST be the same whether the submodule is initialised or not, and it MUST NOT be read from the submodule's own checkout.
- **FR-004**: The component's identity MUST be:
  - for a GitHub URL (https, ssh or scp form, with or without `.git`): `pkg:github/<owner>/<repo>@<commit>`;
  - for any other URL: `pkg:generic/<name>@<commit>?vcs_url=git+<url>`, where `<name>` is the URL's last path segment without `.git`;
  - for a gitlink with no URL: `pkg:generic/<path-basename>@<commit>`.
- **FR-005**: Every URL MUST pass through the milestone-075 credential sanitizer before it appears in a PURL, an annotation or a log line. When sanitizing changes a URL, a warning MUST name the `.gitmodules` file and give the redacted URL.
- **FR-006**: The reader MUST recurse into each *initialised* submodule and apply FR-001 to FR-005 to it, taking nested commits from that submodule's own recorded tree. It MUST NOT enumerate submodules of an uninitialised submodule.
- **FR-007**: Each component MUST carry `waybill:source-mechanism: git-submodule` (C55) and `waybill:build-reference` (C57). Its classification is defined by FR-008.
- **FR-008**: `waybill:build-reference` MUST be `declared-and-used` when either of these holds, and `declared-only` otherwise:
  - a `find_package` target name case-insensitively matches the submodule's last path segment, under milestone 105's FR-008a rule;
  - a CMake file under the scan root references the submodule's path, through `add_subdirectory(<path>)` or an `ExternalProject_Add` / `FetchContent_Declare` `SOURCE_DIR`, with the path resolved relative to the referencing file's directory.

  Classification MUST NOT depend on walk order, and the correlation MUST NOT itself emit components. C57's catalog definition MUST be updated to name both signals.
- **FR-009**: A submodule component MUST merge with another reader's component only when their canonical PURLs are identical. Milestone-105 FR-015 then applies unchanged: one component, manifest-mode declarations outrank `git-submodule`, and the losing mechanisms are recorded in `waybill:also-detected-via` (C56). Components with different PURLs MUST stay separate, even when they plausibly name the same library. That covers a package-manager name against a repository, and a tag against a commit.
- **FR-010**: A submodule component MUST be a dependency of the component that pins it: the document's root for a top-level submodule, and the enclosing submodule's component for a nested one.
- **FR-011**: Every skip MUST be reported. That covers an entry with no gitlink, a gitlink with no URL, a hostile path or URL, a tree with no repository metadata, and an unavailable `git`. Each gets a warning naming the file, and the counts appear in the reader's log summary.
- **FR-012**: Nothing from `.gitmodules` may be interpreted as a command-line option or as a path outside the scan root in any subprocess the reader runs.
- **FR-013**: The reader MUST work offline. It reads only local repository data and fetches nothing.
- **FR-014**: Output MUST be identical across CycloneDX 1.6, SPDX 2.3 and SPDX 3 for C55, C56 and C57, and byte-identical across repeated scans of the same tree.

### Key Entities

- **Gitlink**: a tree entry recording that a path is a submodule pinned at a commit. It is the authority for which submodules exist and at which commit.
- **Submodule declaration**: a `.gitmodules` entry giving a name, path and URL. It supplies the URL, never the commit.
- **Submodule component**: one per gitlink. Its identity comes from the declaration's URL and the gitlink's commit, and it carries C55 and C57.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The #1515 reproduction (one submodule, scanned checked out and as a fresh clone) gives exactly one submodule component in each scan, with the gitlink's commit. The released v0.16.0 gives zero.
- **SC-002**: On a fixture whose `.gitmodules` files list more paths than there are gitlinks, the component count equals the gitlink count exactly, and every listed path without a gitlink appears in a skip diagnostic.
- **SC-003**: On ROCm TheRock at commit `b1cae390cc745283dce25581415353f9bc906162`, the number of submodule components equals the number of gitlinks recorded at that commit. Both counts are captured in CI and never by cloning or scanning TheRock locally.
- **SC-004**: On a fixture whose URLs carry credentials, no emitted document contains any credential substring.
- **SC-005**: The `dedup_collision` test runs without `#[ignore]`:
  - it asserts two components (`pkg:conan/abseil@20240722.0` and the submodule's `pkg:github/abseil/abseil-cpp@<commit>`);
  - a second fixture, where a `FetchContent_Declare` pins the same repository at the gitlink's commit, gives one component with `git-submodule` in `also-detected-via`;
  - 10 repeated scans of each fixture are byte-identical.
- **SC-006**: C55, C56 and C57 values agree across all three output formats for every submodule component in the fixtures.
- **SC-008**: In a fixture with three submodules, the C57 classification is:
  - `declared-and-used` for the one consumed by `add_subdirectory(<path>)`;
  - `declared-and-used` for the one matched by `find_package`;
  - `declared-only` for the one referenced by neither.
- **SC-007**: No committed golden changes except those whose input tree contains gitlinks. Each of those changes only by gaining submodule components and their edges.

## Assumptions

- `git` is a runtime assumption already (milestone 053's `git describe` ladder). The reader may run it against local repository data only. Where it is unavailable, FR-011 applies, and nothing is guessed.
- Fixtures use synthetic `waybill-fixture-*` names and local `file://` repositories built at test time. No test clones a remote repository, and TheRock is never cloned or scanned locally (SC-003 is measured in CI).
- A submodule pin is source-tier evidence: it names an exact commit, not a requirement.
- Zero new Cargo dependencies.
- Correlating a submodule with the components *inside* its directory (for example the submodule's own `Cargo.toml` main module) is out of scope, except as FR-009 decides for components naming the same library.
- Which submodules a given build actually compiles is not knowable from source. That is #1544's build-tree work, and FR-008's classification is only a source-level signal.
- Notes for the cataloger interface (#1545): TheRock's own subproject and fetch conventions belong to a TheRock cataloger (#1539). This reader covers only git's own submodule mechanism, which every project shares.
