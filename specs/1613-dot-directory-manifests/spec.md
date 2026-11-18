# Feature Specification: Read dependency manifests in CI and docs dot-directories

**Feature Branch**: `1613-dot-directory-manifests`
**Created**: 2026-11-18
**Status**: Draft
**Input**: User description: "#1610: the walker skips every dot-directory, so dependency manifests under CI and docs directories are never read. Measured with census/dotdirs.py … On ROCm TheRock b1cae390 with submodules … .github holds 930 files … .ci holds 4 pip requirements files … .sphinx holds 1 … Decisions to make: (1) which dot-directories are descended … (2) whether the rule is the same for every walker … (3) how components declared only under a CI or docs directory are labelled … (4) whether skipped dot-directories that hold manifests are recorded … Out of scope: a GitHub Actions reader (#435, #821) and Dockerfile base images (#1537) … Acceptance: on TheRock's source tree, the packages from the 8 requirements files appear with the decided scope and derivation …" (full text in the conversation that created this branch)

The spec number (1613) is the next in sequence. It is unrelated to PR #1613.

## Background

### What is skipped (measured)

All waybill walkers skip every directory whose name starts with `.`. One helper (`project_roots::should_skip_default_descent`) decides this for about 15 walkers:
- the shared walker;
- the file tier;
- per-reader walkers in pip, npm, nuget, gradle, kotlin_dsl, swift and yocto.

The figures below come from `census/dotdirs.py`, which lists every file under a dot-directory (other than `.git`) by the outermost dot-directory and by manifest type, recognised by file name. It runs in `kusari-sandbox/waybill-fixture-therock-sdk`; TheRock is scanned in CI only.

**ROCm TheRock `b1cae390`, with submodules** (`census-dotdirs` run 44370134340):

| Dot-directory | Files | Manifests by type |
|---|---|---|
| `.github` | 930 | 513 Actions workflows, 30 composite actions, 4 Dockerfiles, 4 CMake files, 3 pip requirements files |
| `.ci` | 46 | 4 pip requirements files, 1 Dockerfile |
| `.sphinx` | 4 | 1 pip requirements file |
| `.upstream-tests` | 2,719 | 8 CMake files |
| `._upstream` | 65 | 1 CMake file |
| `.gitlab-ci` | 183 | none |
| `.jenkins` | 55 | none |
| `.claude` | 121 | none |

`.upstream-tests` is an upstream test suite, and is already in spec 1533's test-directory list.

The eight requirements files are:
- `compiler/amd-llvm/.ci/requirements.txt`, `all_requirements.txt`, `metrics/requirements.txt` and `metrics/requirements.lock.txt`;
- `base/rocm-cmake/test/docsphinx/docs/.sphinx/requirements.txt`;
- `rocm-libraries/.github/requirements-hipdnn-superbuild-ci.txt`;
- two more under `.github`.

The `.ci` files pin about a dozen packages declared nowhere else in the tree, for example `google-cloud-storage`, `google-auth`, `google-api-core` and `junitparser`.

**The 17 cached public-corpus trees:** the only dot-directory content is `.github`: 57 workflows in 16 trees, and 19 actions in 2 trees. No waybill reader reads either today. Descending into `.github` should therefore change no corpus output; that is a prediction.

**waybill's own repository:** `.venv` (1,942 files) and Cargo's `.fingerprint` (6,699 files) are the caches the skip exists for.

### What already exists for labelling

The pip reader classifies a requirements file's scope by its path (`classify_requirements_scope`).

| Signal | Matches | Scope |
|---|---|---|
| Immediate parent directory | `docs`/`doc`/`documentation` | `docs` |
| | `test`/`tests` | `test` |
| | `ci`/`.ci` | `ci` |
| File name | `requirements-<scope>*.txt` or `<scope>-requirements*.txt` | `dev`, `test`, `docs` or `ci` |

A classified file's packages are emitted with the Optional lifecycle scope, plus C155 `waybill:python-req-file-scope` = the scope. The classifier already names `.ci`, but the walker never reaches it.

Under that rule, of the eight files only those whose immediate parent is `.ci`, or whose name carries a scope, would be labelled:
- `.ci/metrics/requirements.txt` (parent `metrics`) would not be;
- `.sphinx/requirements.txt` would not be;
- `requirements-hipdnn-superbuild-ci.txt` would not be (the scope word is not adjacent to `requirements`).

### Out of scope

- **A GitHub Actions reader** (#435, #821).
- **Dockerfile base images** (#1537).

This feature only makes their inputs reachable: once it lands, the 513 workflows, 30 actions and 5 Dockerfiles on TheRock are within the walk.

## Clarifications

### Session 2026-11-18

- Q: Should the document record what was descended or still skipped? → A: Nothing beyond the labels on each component. No document-scope count.
- Q (raised in planning): the pip selector reads only names starting with `requirements`, so `.ci/all_requirements.txt`, the only file declaring most of TheRock's CI-only packages, would stay unread even once `.ci` is walked. Include the fix here? → A: Yes. Widen the selector to the names the scope classifier can label (research R3).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Packages declared for CI and docs builds appear, labelled as such (Priority: P1)

A consumer scans a repository whose CI or documentation builds install Python packages from requirements files under `.ci/`, `.github/` or `.sphinx/`. Those packages appear in the document, and they are labelled as CI or docs dependencies, not as the product's runtime dependencies.

**Why this priority**: today they are silently absent. On TheRock, about a dozen packages exist only in `.ci/` requirements files. CI tooling is part of the build's supply chain, but a consumer must be able to tell it apart from what ships.

**Independent Test**: a synthetic repository with:
- `.ci/requirements.txt` and `.ci/metrics/requirements.txt`;
- `.github/requirements-tool-ci.txt`;
- `docs/.sphinx/requirements.txt`.

Each declares one `waybill-fixture-*` package. The document shows all four, each with a non-runtime scope and a label naming CI or docs.

**Acceptance Scenarios**:

1. **Given** a requirements file directly under `.ci/`, **When** the repository is scanned, **Then** its packages appear with the `ci` label (C155) and a non-runtime scope.
2. **Given** a requirements file deeper inside a CI dot-directory (`.ci/metrics/requirements.txt`, or a file anywhere under `.github/`), **When** scanned, **Then** its packages carry the `ci` label too. The CI directory's influence does not stop at the immediate parent.
3. **Given** a requirements file under `.sphinx/`, **When** scanned, **Then** its packages carry the `docs` label.
4. **Given** a package declared both under `.ci/` and in the product's own `requirements.txt`, **When** scanned, **Then** it appears once, with the runtime scope. The product's declaration wins, as it does today for a package that both a runtime and a dev requirements file declare.

---

### User Story 2 - Caches, virtualenvs, VCS metadata and test suites stay skipped (Priority: P1)

A consumer scans a repository holding `.venv/`, tool caches, editor directories and upstream test suites under dot-directories. None of their content appears in the document, and the scan does not slow down walking them.

**Why this priority**: the skip exists for these. Descending into a virtualenv or a package cache would flood the document with installed copies and spend time on thousands of irrelevant files.

**Independent Test**: a synthetic repository with a requirements file under:
- `.venv/`;
- `.tox/`;
- `.upstream-tests/`;
- `.cache/`.

None of their packages appear. A repository with no dot-directories produces a document byte-identical to the one produced before this feature.

**Acceptance Scenarios**:

1. **Given** manifests under `.venv/`, `.tox/`, `.cache/`, `.idea/`, `.vscode/`, `.git/` or `.upstream-tests/`, **When** scanned, **Then** none of their packages appear.
2. **Given** a repository with no listed dot-directory and no `<scope>-requirements*.txt` file, **When** scanned before and after this feature, **Then** the documents are identical.
3. **Given** the public corpus, **When** scanned, **Then** every golden is unchanged, or each change is traced and explained.

---

### User Story 3 - The file tier does not start reporting CI files (Priority: P2)

A consumer who scans with a file inventory does not see workflow files, CI scripts and docs configuration as file-tier components merely because their directory is now walked for manifests.

**Why this priority**: the file tier reports files no package claims. Opening CI directories to it would add hundreds of workflow and script files that are not shipped content (513 workflows on TheRock alone).

**Independent Test**: the same synthetic repository, scanned with the file tier on, shows no file-tier component for any file under a CI or docs dot-directory.

**Acceptance Scenarios**:

1. **Given** workflow files and scripts under `.github/`, **When** scanned, **Then** no file-tier component is emitted for them.

### Edge Cases

- **A dot-directory that is not CI or docs but holds a manifest** (for example `.devcontainer/` is CI-adjacent; an unknown `.tools/` is not on the list). Only listed names are descended; an unlisted directory stays skipped.
- **A virtualenv or cache nested inside a CI directory** (`.github/.venv/`, `.ci/node_modules/`). The existing skips still apply below a descended directory: descending `.ci` does not descend everything under it.
- **A `package.json` under `.github/actions/<name>/`** (a JavaScript action). It is read by the npm reader and labelled `ci`, like a requirements file. Its `node_modules/`, if committed, stays skipped.
- **A CI directory nested deep in a submodule** (`compiler/amd-llvm/.ci/`). Recognised by name wherever it occurs, not only at the scan root.
- **A requirements file that is also a lockfile** (`requirements.lock.txt`). Read as today's pip reader reads it; only reachability changes.
- **The same requirements file reachable through two paths** (a symlink into `.ci/`). Read once, as the walker's existing visited set ensures.
- **A dot-directory name that differs only in case** (`.CI`, `.GitHub`). Matched case-insensitively, as the pip classifier matches `Doc`/`docs`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The manifest readers that discover files through the shared walker MUST descend into dot-directories whose name is on a fixed list of CI and docs directories. The list is:
  - `.github`;
  - `.ci`;
  - `.gitlab`;
  - `.circleci`;
  - `.buildkite`;
  - `.devcontainer`;
  - `.sphinx`.
  
  Names are matched case-insensitively. Every other dot-directory MUST stay skipped. Readers that walk the tree themselves (nuget, gradle, kotlin_dsl, swift, yocto) are unchanged: none of their manifests occur under listed directories on TheRock or the public corpus (research R2).
- **FR-002**: Below a descended dot-directory, the existing skips MUST still apply: other dot-directories not on the list, `node_modules`, `vendor`, virtualenvs and build outputs.
- **FR-003**: The file tier MUST NOT descend into the listed directories. Files under them MUST NOT become file-tier components.
- **FR-004**: The pip requirements scope classifier MUST label a requirements file found anywhere under a listed CI directory (`.github`, `.ci`, `.gitlab`, `.circleci`, `.buildkite`, `.devcontainer`) as `ci`, and one under `.sphinx` as `docs`. Its packages get the Optional scope and C155, as files the classifier already labels do today.
  - Precedence: an explicit `test` or `docs` signal from the immediate parent directory or the file name keeps today's precedence over the ancestor signal.
- **FR-004a**: The pip requirements selector MUST also read:
  - files named `<scope>-requirements*.txt`, where `<scope>` is one the classifier labels (`dev`, `test`, `docs`, `ci`);
  - any file named `*requirements*.txt` under a listed CI or docs directory.
  
  Files of the second kind are labelled by FR-004. Other names that are not `requirements*.txt` (for example `tox-requirements.txt`) stay unread, because nothing would label them and they would read as runtime. The existing #1522 check against non-pip content applies to every file read.
- **FR-005**: For readers that have no file-location scope of their own (for example npm, for a `package.json` under `.github/actions/`), components declared only under a listed directory MUST get a non-runtime lifecycle scope. They MUST carry a derivation naming the directory class (`ci` or `docs`), carried in an existing catalog row if one fits. A new row needs registration first (FR-009).
- **FR-006**: A package declared both under a listed directory and in a non-CI location MUST appear once, with the scope the non-CI declaration gives. This uses the existing scope-merge rules.
- **FR-007**: A repository with no listed dot-directory and no file named `<scope>-requirements*.txt` MUST produce a document byte-identical to the one produced without this feature. A repository that has such a file changes only by the packages that file declares, each labelled with its scope (FR-004a).
- **FR-008**: The document MUST record CI and docs declarations only through the labels on each component (C155, and FR-005's derivation). No document-scope record is added for directories descended or skipped. Those directories are skipped on purpose, and counting their contents would mean walking the caches the skip avoids.
- **FR-009**: Every new annotation MUST be registered as a catalog row, with parity extractors for all three formats, before it is emitted. The formats MUST agree on it.
- **FR-010**: The walker-audit gate MUST stay green. Any new walk function is reflected in its allowlist.
- **FR-011**: Documents MUST be identical across repeated scans (spec 1542's repeat check).
- **FR-012**: Tests MUST use synthetic `waybill-fixture-*` fixtures:
  - requirements files under `.ci/`, `.ci/metrics/`, `.github/` and `.sphinx/`;
  - requirements files under `.venv/`, `.tox/`, `.cache/` and `.upstream-tests/`;
  - a `package.json` under `.github/actions/<name>/`, with a committed `node_modules/`;
  - a package declared both under `.ci/` and at the root;
  - a repository with no dot-directories.
- **FR-013**: No new Cargo dependencies.

### Key Entities

- **Listed dot-directory**: a directory whose name is on FR-001's list. It is descended by manifest readers and is not walked by the file tier. Its class is `ci`, or `docs` for `.sphinx`.
- **CI or docs declaration**: a manifest found under a listed dot-directory. Its components carry the class's label and a non-runtime scope, unless declared elsewhere too.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock's source tree, the packages declared in all 8 requirements files under `.ci`, `.github` and `.sphinx` appear in the document, each with a `ci` or `docs` label and a non-runtime scope. This includes the dozen or so declared nowhere else, most of which only `.ci/all_requirements.txt` declares.
- **SC-001a**: On TheRock's source tree, the packages of the two `dev-requirements.txt` files (composablekernel, MIOpen) appear, labelled `dev`. `tox-requirements.txt` stays unread, and is recorded as a known gap.
- **SC-002**: On TheRock's source tree, no package declared only under `.upstream-tests`, `._upstream`, `.gitlab-ci`, `.jenkins` or `.claude` appears, and no file under any dot-directory becomes a file-tier component.
- **SC-003**: Every public-corpus golden is unchanged, or each change is traced. The prediction is none, because the corpus's dot-directory content is workflows and actions that no reader reads yet.
- **SC-004**: The three formats agree on every new or extended annotation on TheRock's source tree, and repeated scans are identical.
- **SC-005**: TheRock's source scan time, interleaved against `main` on the same runner class, is no more than 1.05× `main`'s median. The listed directories hold about 1,000 files against the tree's 335,551.
- **SC-006**: The quality corpus is traced against a `main` control, and every change is explained.

## Assumptions

- **A fixed list rather than a deny-list.** A deny-list of caches would silently start reading any new dot-directory, which is the failure the skip guards against. The list can grow as new CI systems are measured.
- **Two labels for CI tooling, by ecosystem.** A CI-only pip package is `Optional` with C155 = `ci`, the pip reader's existing treatment of `requirements-ci.txt` since milestone 670. A CI-only component from any other reader is `Development` with C62 = `ci-directory`. Both are non-runtime, so a consumer that excludes non-runtime scopes excludes both. A consumer selecting by label checks C155 for pip and C62 otherwise. Aligning pip with `Development` would change milestone 670's documents and is out of scope.
- **The file tier stays out.** It reports unclaimed files as shipped content. CI scripts and workflows are not shipped content, and including them would contradict that.
- **CI tooling is a non-runtime dependency.** It belongs in the document for supply-chain completeness, labelled so a consumer filtering for shipped content can exclude it. This matches how the pip reader already treats `requirements-ci.txt`.
- **Out of scope: GitHub Actions and Dockerfiles.** No reader for workflows or Dockerfiles is added here; #435, #821 and #1537 build on the reachability this provides.
