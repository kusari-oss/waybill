# Feature Specification: Git submodules as components

**Feature Branch**: `1530-git-submodule-reader`
**Created**: 2026-11-15
**Status**: Draft
**Input**: Issue #1515 — git submodules are never emitted. waybill has no reader for `.gitmodules` or gitlink commits, so a C/C++ super-build that pins its dependencies as submodules yields none of them. Milestone 105 designed this reader as User Story 6; its tasks (T083–T085) were never done. Reuse that design and record what no longer holds.

## Clarifications

### Session 2026-11-15

- Q: When a submodule and another reader name the same library under different PURLs (a Conan `pkg:conan/abseil@20240722.0` against a submodule `pkg:github/abseil/abseil-cpp@<sha>`, or a CMake `GIT_TAG v1.2.3` against the submodule's commit of the same repository), what happens? → A: **They merge only on an identical canonical PURL.** Different PURLs stay two components. There is no guessed mapping between a package-manager name and a repository, or between a tag and a commit. The `dedup_collision` test is rewritten to assert this (FR-009).
- Q: What counts as "used" for C57 `waybill:build-reference`? → A: **The milestone-105 `find_package` name match, or a CMake path reference.** A path reference is `add_subdirectory(<path>)`, or an `ExternalProject_Add` / `FetchContent_Declare` `SOURCE_DIR` naming the submodule's path. C57's documented definition is widened to say so (FR-008).
- Q: Which hosts map to a forge PURL type, not `pkg:generic`? → A: **The forge set waybill already uses** (`yocto/recipe.rs`): `github.com` → `pkg:github`, `gitlab.com` → `pkg:gitlab`, `bitbucket.org` → `pkg:bitbucket`, `codeberg.org` → `pkg:codeberg`. Every other host gives `pkg:generic/…?vcs_url=`. One repository then has one identity across readers, which FR-009's identical-PURL merge depends on (FR-004).
- Q: Where do skipped submodules get reported, given that a consumer never sees the log? → A: **In the document, through a new document-level catalog row.** It is emitted only when a `.gitmodules` file was seen, and records the count of submodules pinned and the count skipped by reason. Logs keep the per-file detail (FR-011).
- Q: Does a `declared-only` submodule (C57) get a narrower scope in the document? → A: **No.** Every submodule is an ordinary dependency of whatever pins it. C57 is the only "probably unused" signal, because a build can consume a submodule through paths the CMake reader cannot see. Which submodules are built is #1544's question (FR-010).

### Session 2026-11-15 (analysis remediation)

- Q: How are forge PURLs cased? purl-spec's `github` and `bitbucket` definitions require the owner and repository to be lowercased. The Nix reader does this, while the CMake and Yocto readers keep the declared case, and ROCm's organisation is `ROCm`. → A: **Lowercase `github` and `bitbucket` owner and repository, and make the CMake reader do the same in this feature,** so that FR-009's identical-PURL merge works for mixed-case repositories. `gitlab` and `codeberg` have no published definition (checked 2026-11-15) and keep the declared case, as the Nix reader does. Yocto is left as is and filed separately (FR-004, FR-015).

### Session 2026-11-15 (implementation)

- Q: Milestone 105's `waybill:also-detected-via` (C56) has never been emitted. `scan_fs::dedup::dedup` has no caller, and the CycloneDX builder passes no winner or losers. Measured: a submodule and a CMake fetch of one PURL already merge into one component, with C55 `cmake-fetchcontent-git` and the submodule's C57 kept. Does #1515 wire C56? → A: **No. It is deferred to #1558.** #1515 asserts the merge and C57. A merged submodule is recognised for root anchoring by C57, which only this reader emits (FR-009, FR-010).

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
| Non-GitHub URLs give `pkg:git+https://<url>@<sha>`. | `git+https` is not a PURL type. | The forge hosts waybill already maps (GitHub, GitLab, Bitbucket, Codeberg) get their forge type. Other hosts get a generic PURL with a `vcs_url` qualifier (FR-004). The elixir reader already uses that exact form for git dependencies, and the cocoapods, composer and dart readers use the same qualifier on their own types. |
| The `dedup_collision` fixture expects `pkg:conan/abseil@…` and `pkg:github/abseil/abseil-cpp@…` to merge after an unspecified "canonicalization". Its own README notes the two PURLs "technically wouldn't dedup". | No such canonicalization was ever specified, and any name-to-repository mapping would be a guess. | Only identical canonical PURLs merge (FR-009). The fixture's test asserts two components. |
| C57 counts a submodule as used only when a `find_package` target matches its last path segment (FR-008a). | Super-builds consume submodules by path, through `add_subdirectory(<path>)` or `SOURCE_DIR`, which a name match cannot see. | A path reference also counts (FR-008). |

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Every pinned submodule becomes a component, even on a fresh clone (Priority: P1)

An engineer scans a C/C++ super-project that pins its third-party code as git submodules. Their CI clones the repository without initialising submodules, so the scan sees `.gitmodules` and the gitlinks but none of the submodule contents. They expect one component per submodule, identified by its repository and pinned to the exact commit the super-project records.

**Why this priority**: This is the whole of #1515. A submodule pin is exact provenance (repository plus commit), and today it is invisible. A fresh, uninitialised clone is the common CI case, which is exactly where the milestone-105 design would have reported `unknown`.

**Independent Test**: Build a super-project fixture with a local `file://` repository added as a submodule, clone it without initialising submodules, and scan both the original and the clone. Each must give one component pinned to the gitlink commit.

**Acceptance Scenarios**:

1. **Given** a super-project with one submodule whose URL is a GitHub repository, **When** it is scanned with the submodule checked out, **Then** exactly one component is emitted as `pkg:github/<owner>/<repo>@<commit>`, where `<commit>` is the gitlink's full commit, carrying `waybill:source-mechanism: git-submodule` and `waybill:build-reference`. A `gitlab.com`, `bitbucket.org` or `codeberg.org` URL gives `pkg:gitlab/…`, `pkg:bitbucket/…` or `pkg:codeberg/…` the same way.
2. **Given** a fresh clone of that super-project with the submodule not initialised, **When** it is scanned, **Then** the same component is emitted with the same commit.
3. **Given** a submodule whose URL is on any other host, **When** it is scanned, **Then** the component is `pkg:generic/<name>@<commit>?vcs_url=git+<url>`.
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

- **No repository metadata.** A source archive or exported tree has `.gitmodules` but no repository data. Nothing records a commit, so no submodule component is emitted. The document's FR-011 annotation counts the entries under `no-repository-data`, and a warning names each `.gitmodules` file.
- **Relative submodule URLs** (`url = ../dep.git`) resolve against the super-project's own remote URL when one is recorded. Otherwise the component keeps the relative URL in `vcs_url`, with the gitlink commit as its version.
- **SSH and scp-style URLs** (`git@github.com:owner/repo.git`, `ssh://git@gitlab.com/owner/repo`) identify forge repositories the same way `https://` URLs do. A `.git` suffix is not part of the repository name.
- **Nested GitLab groups** (`gitlab.com/group/subgroup/project`): only `<owner>/<repo>` is recognised, as in the Yocto reader, so a path with more segments falls back to `pkg:generic/…?vcs_url=` instead of truncating the path.
- **A gitlink with no `.gitmodules` entry**, such as a submodule added and then removed from `.gitmodules`, has a commit but no URL. It is emitted as `pkg:generic/<path-basename>@<commit>` without a `vcs_url`, with a diagnostic.
- **The scan root is a subdirectory of a working tree** whose `.gitmodules` lies above the scan root. Nothing is emitted, and no coverage record is written. The reader never reads a repository above the scan root, as the golang reader already avoids a parent `.git`. *(Amended during planning: research R14.)*
- **The checked-out commit differs from the gitlink**, for example a submodule moved locally and not committed. The gitlink is what the super-project pins and is what is emitted. The difference is recorded as a diagnostic, not as a second component.
- **The `git` tool is unavailable** on the host. The reader emits no submodule components, counts the entries under `git-unavailable` in the FR-011 annotation, and warns once. It never falls back to guessing from `.gitmodules`.
- **Git refuses the repository** ("dubious ownership", `safe.directory`). The reader emits nothing for it, counts the entries under `git-refused`, and warns with the remedy. It never overrides `safe.directory` itself, because that check exists to stop a foreign repository's config from running code. *(Amended during planning: research R2.)*
- **A hostile repository config** (for example `core.fsmonitor` naming a program). It MUST NOT run. The reader uses only commands measured not to load the index, and also disables `core.fsmonitor` and hooks on the command line. *(Research R1.)*
- **Hostile `.gitmodules` content.** A path that escapes the scan root (`../`, absolute) or a URL that looks like a command-line option is skipped with a warning, and nothing from it reaches a subprocess argument.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan MUST read every `.gitmodules` file under the scan root for each submodule's `path` and `url`. That includes those of the super-project and of each initialised submodule (FR-006), and any `.gitmodules` inside a repository below its top level, such as a vendored project or a monorepo's own projects. A non-top-level `.gitmodules` is joined against its enclosing repository's gitlinks, under its own directory as a path prefix. Its entries that match an already-declared gitlink deduplicate with that declaration. Its entries that match none count as `no-gitlink`. Only a `.gitmodules` with no enclosing repository at or below the scan root counts as `no-repository-data`.
- **FR-002**: A component MUST be emitted for each gitlink, a tree entry recording a submodule commit, under the scan root. It MUST NOT be emitted for a `.gitmodules` entry that has no gitlink. A path listed by several `.gitmodules` files MUST give one component.
- **FR-003**: The component's version MUST be the full commit recorded by the gitlink in the super-project's tree. It MUST be the same whether the submodule is initialised or not, and it MUST NOT be read from the submodule's own checkout.
- **FR-004**: The component's identity MUST be:
  - for a URL on a forge host waybill already maps, in https, ssh or scp form, with or without `.git`, and with exactly an `<owner>/<repo>` path: `pkg:<forge>/<owner>/<repo>@<commit>`. The hosts are `github.com` → `github`, `gitlab.com` → `gitlab`, `bitbucket.org` → `bitbucket` and `codeberg.org` → `codeberg`, the same mapping as the Yocto reader;
  - for `github` and `bitbucket`, `<owner>` and `<repo>` MUST be lowercased, as their purl-spec type definitions require (`ROCm/Foo` gives `pkg:github/rocm/foo`). `gitlab` and `codeberg` have no published type definition, so the declared case is kept, as the Nix reader keeps it;
  - for any other URL: `pkg:generic/<name>@<commit>?vcs_url=git+<url>`, where `<name>` is the URL's last path segment without `.git`;
  - for a gitlink with no URL: `pkg:generic/<path-basename>@<commit>`.
- **FR-005**: Every URL MUST pass through the milestone-075 credential sanitizer before it appears in a PURL, an annotation or a log line. When sanitizing changes a URL, a warning MUST name the `.gitmodules` file and give the redacted URL.
- **FR-006**: The reader MUST recurse into each *initialised* submodule and apply FR-001 to FR-005 to it, taking nested commits from that submodule's own recorded tree. It MUST NOT enumerate submodules of an uninitialised submodule.
- **FR-007**: Each component MUST carry `waybill:source-mechanism: git-submodule` (C55) and `waybill:build-reference` (C57). Its classification is defined by FR-008.
- **FR-008**: `waybill:build-reference` MUST be `declared-and-used` when either of these holds, and `declared-only` otherwise:
  - a `find_package` target name case-insensitively matches the submodule's last path segment, under milestone 105's FR-008a rule;
  - a CMake file under the scan root references the submodule's path, or a path inside it, through `add_subdirectory(<path>)` or an `ExternalProject_Add` / `FetchContent_Declare` `SOURCE_DIR`, with the path resolved relative to the referencing file's directory.

  Classification MUST NOT depend on walk order, and the correlation MUST NOT itself emit components. C57's catalog definition MUST be updated to name both signals.
- **FR-009**: A submodule component MUST merge with another reader's component only when their canonical PURLs are identical. The existing PURL deduplicator then emits one component, which keeps the submodule's C57. Recording the losing mechanisms in `waybill:also-detected-via` (C56) is milestone 105's design but has never been wired, and is deferred to #1558. Components with different PURLs MUST stay separate, even when they plausibly name the same library. That covers a package-manager name against a repository, and a tag against a commit.
- **FR-010**: A submodule component MUST be an ordinary dependency of the component that pins it: the document's root for a top-level submodule, and the enclosing submodule's component for a nested one. Its scope MUST NOT be narrowed by its C57 value: no optional lifecycle scope, no excluded scope, no removed edge. C57 is the only signal that a submodule may be unused. A top-level submodule that FR-009 merged into another reader's component MUST keep its root edge. It is recognised by C57, which only this reader emits.
- **FR-011**: Every skip, and every component emitted with incomplete identity, MUST be reported in the document and in the log.
  - **In the document**: whenever a `.gitmodules` file was seen, the document MUST carry one document-level annotation, under a new catalog row with extractors in all three formats. It records the count of submodules pinned and the count skipped for each reason: `no-gitlink`, `no-url`, `no-repository-data`, `git-unavailable`, `git-refused` and `unsafe-path-or-url`. `git-refused` was amended in during planning (research R2). A reason with a zero count is listed with zero, so an absent reason cannot be mistaken for one that was not checked.
  - **`no-url`**: unlike the other reasons, it counts gitlinks that *are* emitted (as `pkg:generic/<path-basename>@<commit>`) because no declaration gives a URL. They are therefore counted in `pinned` as well. Every other reason counts entries that produced no component.
  - **In the log**: each skip also gets a warning naming the file. `git-unavailable` warns once per scan.
  - **When it is absent**: a document with no `.gitmodules` file in its tree carries no such annotation.
- **FR-012**: Nothing from `.gitmodules` may be interpreted as a command-line option or as a path outside the scan root in any subprocess the reader runs. Nothing a scanned repository configures may run either. The reader MUST NOT run a `git` command that loads the index, since R1 measured `ls-files` and `status` running `core.fsmonitor` from repository config. It MUST pass `-c core.fsmonitor=false` and an empty hooks path on every invocation.
- **FR-013**: The reader MUST work offline. It reads only local repository data and fetches nothing.
- **FR-014**: Output MUST be identical across CycloneDX 1.6, SPDX 2.3 and SPDX 3 for C55 and C57, and byte-identical across repeated scans of the same tree.
- **FR-015**: The CMake reader's `pkg:github` and `pkg:bitbucket` PURLs, from a `FetchContent_Declare` `GIT_REPOSITORY`, MUST lowercase owner and repository in the same way, so a submodule and a CMake fetch of the same repository at the same commit share one canonical PURL (FR-009). No other CMake output changes.

### Key Entities

- **Gitlink**: a tree entry recording that a path is a submodule pinned at a commit. It is the authority for which submodules exist and at which commit.
- **Submodule declaration**: a `.gitmodules` entry giving a name, path and URL. It supplies the URL, never the commit.
- **Submodule component**: one per gitlink. Its identity comes from the declaration's URL and the gitlink's commit, and it carries C55 and C57.
- **Submodule coverage record**: the document-level FR-011 annotation. It holds the count of submodules pinned and the count skipped for each reason, and is present only when a `.gitmodules` file was seen.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The #1515 reproduction (one submodule, scanned checked out and as a fresh clone) gives exactly one submodule component in each scan, with the gitlink's commit. The released v0.16.0 gives zero.
- **SC-002**: On a fixture whose `.gitmodules` files list more paths than there are gitlinks, the component count equals the gitlink count exactly, and every listed path without a gitlink appears in a skip diagnostic.
- **SC-003**: On ROCm TheRock at commit `b1cae390cc745283dce25581415353f9bc906162`, the number of submodule components equals the number of gitlinks recorded at that commit. Both counts are captured in CI and never by cloning or scanning TheRock locally.
- **SC-004**: On a fixture whose URLs carry credentials, no emitted document contains any credential substring.
- **SC-005**: The `dedup_collision` test runs without `#[ignore]`:
  - it asserts two components (`pkg:conan/abseil@20240722.0` and the submodule's `pkg:github/abseil/abseil-cpp@<commit>`);
  - a second fixture, where a `FetchContent_Declare` pins the same repository at the gitlink's commit, gives one component that carries the submodule's C57 and is a root dependency (C56 deferred to #1558);
  - 10 repeated scans of each fixture are byte-identical.
- **SC-006**: C55 and C57 values agree across all three output formats for every submodule component in the fixtures. C56 is out of scope (#1558).
- **SC-007**: No committed golden changes except those whose input tree contains a `.gitmodules` file. Each of those changes only by gaining submodule components, their edges and the coverage record. Research R13 measured the public corpus: no target pins a submodule, and only `haskell-language-server` has a (comment-only) `.gitmodules`. So exactly its three goldens are predicted to change, each gaining a coverage record of `pinned: 0`. *(Amended during planning: research R13.)*
- **SC-008**: In a fixture with three submodules, the C57 classification is:
  - `declared-and-used` for the one consumed by `add_subdirectory(<path>)`;
  - `declared-and-used` for the one matched by `find_package`;
  - `declared-only` for the one referenced by neither.
- **SC-009**: Scanning a fixture's `.gitmodules` with its repository data removed gives zero submodule components. The document's coverage record shows the entry count under `no-repository-data` and zero pinned, identically in all three formats. The same tree with its repository data present shows every entry pinned.
- **SC-010**: With a hostile `core.fsmonitor` configured in the scanned repository, a scan runs no program the repository names. A test asserts that the payload's marker file is absent.
- **SC-011**: A submodule of `https://github.com/ROCm/waybill-fixture-Mixed.git` at commit `<C>`, plus a `FetchContent_Declare(… GIT_REPOSITORY https://github.com/ROCm/waybill-fixture-Mixed.git GIT_TAG <C>)`, gives exactly one component, `pkg:github/rocm/waybill-fixture-mixed@<C>`, which carries the submodule's C57 and is a root dependency. The pre-existing `golden/*/cmake.*` goldens are unchanged, because their GitHub PURL is already lowercase.

## Assumptions

- `git` is a runtime assumption already (milestone 053's `git describe` ladder). The reader may run it against local repository data only. Where it is unavailable, FR-011 applies, and nothing is guessed.
- Fixtures use synthetic `waybill-fixture-*` names and local `file://` repositories built at test time. No test clones a remote repository, and TheRock is never cloned or scanned locally (SC-003 is measured in CI).
- A submodule pin is source-tier evidence: it names an exact commit, not a requirement.
- Zero new Cargo dependencies.
- Correlating a submodule with the components *inside* its directory (for example the submodule's own `Cargo.toml` main module) is out of scope, except as FR-009 decides for components naming the same library.
- Which submodules a given build actually compiles is not knowable from source. That is #1544's build-tree work, and FR-008's classification is only a source-level signal.
- Notes for the cataloger interface (#1545): TheRock's own subproject and fetch conventions belong to a TheRock cataloger (#1539). This reader covers only git's own submodule mechanism, which every project shares.
