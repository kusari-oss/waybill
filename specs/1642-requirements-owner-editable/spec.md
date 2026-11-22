# Feature Specification: A requirements file belongs to the project in its directory, not to a lockfile entry beside it (#1640)

**Feature Branch**: `1642-requirements-owner-editable`
**Created**: 2026-11-21
**Status**: Draft
**Input**: #1640. A requirements file's packages attach to a dev-group editable package from the `uv.lock` beside it, not to the project. Refs #1566, #1587, #1618.

## Clarifications

### Session 2026-11-21

- Q: Does FR-001 change the emitted source paths, or only the owner lookup? → A: The emitted ones. A merged pip main module lists its manifest's directory first and keeps the lockfile path after it, and `waybill:workspace-member` follows. The owner lookup and the annotations read the same value, so they cannot disagree.
- Q: Does FR-005's invariant cover every ecosystem, or only pip? → A: Every ecosystem. Any component that the root (by relationship or reader root anchor, which includes top-level git submodules) or any runtime main module depends on directly is protected. The census counts and traces the effect by ecosystem.
- Planning amendment (research R2): FR-001 applies to a main module merged with a **lockfile** entry. An **installed** entry (venv or `dist-info`) keeps its own path first. `waybill:layer-digest` is stamped from the first source path, so moving it would drop the layer digest on image scans. The census counts installed merges.
- Planning amendment (research R3): FR-003's step 1 holds by construction once FR-001 is in place. Every pip main module's first source path is then its manifest's directory, so the key is `(rank, non-runtime, PURL)`. The census verifies that no candidate's first path is a lockfile.
- Planning amendment (research R6): the public corpus's harness keeps no scan logs. Its SC-001 census is the regenerated diff at the wired commit, traced component by component. At the log-only commit it must show 0 differing files.
- FR-004 resolved by measurement (research R4, `measurements/probe.md`): a package that a scan-root requirements file declares reaches the root through a reader root anchor. The propagation's immunity search does not start from anchors, so a non-runtime path into the package wins. Measured: `excluded`, C62 `transitive`, in 3 of 3 runs. The control, with the file owned by the project so that the edge is a relationship, is `required` in 3 of 3.

## Background

Spec 1537 (#1566, via #1587) gives each package a pip requirements file declares an edge from the project that declares the file. The project is found by location (`scan_fs/declared_attach.rs`, `OwnerIndex`): the nearest enclosing directory that some main module or git submodule owns. Within one directory, a `pkg:pypi` main module outranks any other, and ties go to the lowest PURL.

**Measured at `main` `79b3350b`** on #1640's synthetic tree:

```
requirements.txt              waybill-fixture-shared>=1.0
tool/pyproject.toml           [project] waybill-fixture-tool 1.0.0; [dependency-groups] dev = ["waybill-fixture-devtool"]
tool/devtool/pyproject.toml   [project] waybill-fixture-devtool 0.1.0
tool/uv.lock                  waybill-fixture-tool (editable "."), waybill-fixture-devtool (editable "devtool", dev group)
tool/requirements.txt         waybill-fixture-shared>=1.0
```

| tree | edges into `pkg:pypi/waybill-fixture-shared` |
|---|---|
| without `tool/requirements.txt` | root |
| with it | root, **`pkg:pypi/waybill-fixture-devtool@0.1.0`** (`scope: excluded`, C122 `pep-735-dependency-groups:dev`) |
| with it, the dev package renamed `waybill-fixture-zdevtool` and nothing else changed | root, `pkg:pypi/waybill-fixture-tool@1.0.0` |

The edge's source should be `waybill-fixture-tool`. Which project owns `tool/` is decided by package-name order.

**Why both claim `tool/`.** Measured on the same tree, with and without `tool/uv.lock`:

| | `waybill-fixture-devtool@0.1.0` source-files | workspace-member |
|---|---|---|
| without `uv.lock` | `["tool/devtool"]` | `["tool/devtool"]` |
| with `uv.lock` | `["tool/uv.lock"]` | `["tool"]` |

Without a lockfile, the main module of `tool/devtool/pyproject.toml` carries its own directory. With one, the pip reader's main-module pass finds the lockfile entry of the same PURL and augments it (`pip/mod.rs`, the FR-011 "augment-existing" branch). It copies the main-module role and annotations onto that entry, but keeps the entry's source path, `tool/uv.lock`. The project's own directory is lost, and both editable packages now claim the lockfile's directory. #954 is the same class of defect: that branch copies only the fields it names, and it once dropped the manifest's license the same way.

**The propagation half, not reproduced.** #1640 also reports, in a large multi-project tree with this layout, five packages that the top-level `requirements.txt` declares directly. They were emitted `scope: excluded` with C62 `transitive`, although the root has a direct edge to each, so runtime requirements were reported as not shipping. Nine more were excluded although an in-scope component depends on them. On the minimal tree, spec 1269's pass logs "assigned nothing" (`immune=3`, `rooted_ecosystems=["pypi"]`), and `waybill-fixture-shared` stays `required`.

*Inference when the spec was written; since reproduced (research R4):* the propagation's immunity search starts from main modules (`component_root_seeds`, plus spec 1616's added seeds). Packages a scan-root requirements file declares reach the root through reader root anchors (`declared_root_anchors`, installed through `supplement::install_reader_root_anchors`). Those are not relationships the pass sees, and they are not seeds, so a root-declared package reached through a non-runtime path has no runtime path that protects it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A requirements file attaches to the project that declares it (Priority: P1)

A team scans a source tree in which a project directory holds a `pyproject.toml`, a `uv.lock` and a `requirements.txt`, and the lockfile also lists an editable development tool from a subdirectory. Every package in the requirements file is a dependency of the project in that directory, whatever the packages are called.

**Why this priority**: This is the defect. A dependency edge from a dev-only tool is false, and it gives scope propagation a non-runtime path into a runtime requirement.

**Independent Test**: Scan #1640's tree in both name orders (`devtool` and `zdevtool`). In both, `waybill-fixture-shared` has edges from the root and from `waybill-fixture-tool@1.0.0`, and none from the dev package.

**Acceptance Scenarios**:

1. **Given** #1640's tree, **When** scanned, **Then** `tool/requirements.txt`'s packages have an edge from `pkg:pypi/waybill-fixture-tool@1.0.0`, in every format.
2. **Given** the same tree with the dev package renamed so that its name sorts after the project's, **When** scanned, **Then** the output is the same, up to the renamed package.
3. **Given** the same tree, **When** scanned, **Then** `waybill-fixture-devtool`'s main module reports its own directory, `tool/devtool`, first in `source-files` and as its workspace-member, as it does with no `uv.lock`. The lockfile path follows it in `source-files`.
4. **Given** a requirements file in `tool/devtool/`, **When** scanned, **Then** its packages attach to `waybill-fixture-devtool`. A project owns its own directory, whatever its scope.

### User Story 2 - A package the root requires directly is never reported as not shipping (Priority: P1)

A team gives the SBOM to a consumer that drops excluded components. A package that the scan root's requirements file declares, or that a runtime main module depends on directly, is never excluded only because a development or optional path also reaches it.

**Why this priority**: This is the cost #1640 observed: five root-declared runtime requirements reported as not shipping. Spec 1269 names that the dangerous direction.

**Independent Test**: A fixture in which the root's requirements file declares a package that a non-runtime component also depends on, in a tree where `pypi` has an ecosystem root. The package is runtime in every format. On `main`, if the inference in the Background holds, it is excluded with C62 `transitive` (the control).

**Acceptance Scenarios**:

1. **Given** a package that the scan root's requirements file declares, and a development-scoped component that depends on it, **When** scanned, **Then** the package has no non-runtime scope from transitive derivation.
2. **Given** a package that a runtime main module depends on directly, and that a non-runtime component also depends on, **When** scanned, **Then** the package has no non-runtime scope from transitive derivation.
3. **Given** a non-pip component that a top-level git submodule or a runtime main module of another ecosystem depends on directly, and that a non-runtime component also depends on, **When** scanned, **Then** it has no non-runtime scope from transitive derivation.
4. **Given** a package reached *only* through non-runtime components, **When** scanned, **Then** spec 1269's propagation still assigns it a non-runtime scope, as on `main`.

### User Story 3 - Nothing else changes (Priority: P2)

A tree in which each directory has one owner produces the same document as `main`.

**Why this priority**: The ownership rule decides edges for every requirements file and, through spec 1538 and spec 1614, for CMake fetches and searches too. A change here must not move any edge it does not mean to.

**Independent Test**: The public corpus regenerated on the branch is byte-identical to the committed goldens except for components traced to an FR. The spec 1537 and spec 1618 fixtures are unchanged.

**Acceptance Scenarios**:

1. **Given** a tree with one project per directory, **When** scanned, **Then** the output is byte-identical to `main`.
2. **Given** a uv workspace whose members are declared in `[tool.uv.workspace]`, **When** scanned, **Then** each member keeps its main-module role, and owns the directory its `pyproject.toml` is in.

### Edge Cases

- **An editable entry with no manifest in the tree:** the lockfile names `source = { editable = "x" }`, but `x/pyproject.toml` is absent or unreadable. No pip main module is built for it, so it owns no directory, as on `main` for a lockfile entry without the main-module role.
- **A virtual source** (`source = { virtual = "." }`): treated like an editable source with the same path.
- **An editable path outside the scan root** (`editable = "../shared"`): it owns nothing inside the scan.
- **Two pip main modules in one directory** (for example a `pyproject.toml` and a `setup.py` naming different projects): the remaining tie (FR-003).
- **A dev-group editable package that is also a runtime dependency** of another project: it keeps the runtime scope the readers give it. FR-002 concerns only what owns a directory.
- **A requirements file in a directory no project owns:** unchanged. It goes to the nearest enclosing owner, or to the document's subject (spec 1537 R4).
- **CMake fetches and searches** use the same owner index (spec 1538 R6, spec 1614). They follow the corrected ownership too. Any change in their edges is traced (SC-004).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A pip main module's project directory MUST be the directory of the manifest that declares it, whichever reader's entry survives the merge of that main module with a same-PURL lockfile entry. An installed entry keeps its own path first (Clarifications, planning amendment). The merged entry MUST list that directory as its first source path, keep the lockfile's path after it, and derive `waybill:workspace-member` from the first (Clarifications). On #1640's tree, `waybill-fixture-devtool` has `source-files` `["tool/devtool", "tool/uv.lock"]` and workspace-member `["tool/devtool"]`, and owns `tool/devtool`. `waybill-fixture-tool` owns `tool`. This settles decision (a): the directory comes from the manifest, and a lockfile's `source.editable` path MUST agree with it. Where they disagree, the manifest wins and the disagreement is logged at debug level.
- **FR-002**: A main module MUST NOT own a directory other than its own (FR-001). A package that a dependency group alone lists keeps its main-module role, its scope and its ownership of its own directory. Decision (b), whether such a package is a main module at all, is settled by SC-001's census. The default is to keep the role, because removing it changes root selection, `waybill:workspace-member` and every C-row keyed on `component-role`, and FR-001 already removes the false ownership. If the census finds a case FR-001 does not fix, the plan revisits this.
- **FR-003**: When two or more owner candidates of the same rank remain for one directory, the owner MUST be chosen in this order:
  1. a candidate whose declaring manifest is in that directory (true of every candidate once FR-001 holds; Clarifications, planning amendment);
  2. a runtime candidate over a non-runtime one;
  3. the lowest PURL, as on `main`.

  A tie that reaches step 3 is logged with the candidates and the directory. This settles decision (c).
- **FR-004**: Decision (d): the condition that lets spec 1269's transitive pass give a non-runtime scope to a package that the root or a runtime main module depends on directly MUST be identified. Each candidate condition is either reproduced on a synthetic fixture (measured) or labelled as inference, with what would test it. The inference in the Background is the first candidate. *Resolved:* it is reproduced (Clarifications; research R4).
- **FR-005**: Decision (e): transitive derivation MUST NOT give a non-runtime scope to a component that the document root (through a reader root anchor or a relationship) or a runtime main module depends on directly, in every ecosystem (Clarifications). Root anchors include top-level git submodules as well as root-declared requirements packages. A reader's own declared scope is unaffected: a requirements file named `requirements-dev.txt` still gives Optional (spec 1537 FR-010). A test of this invariant MUST fail on `main` if FR-004 reproduces the condition.
- **FR-006**: The three formats MUST agree (`waybill sbom parity-check`). CycloneDX MUST validate against the vendored 1.6 schema, and no two components may share a `bom-ref`.
- **FR-007**: A scan in which every directory has one owner candidate, and in which FR-005 removes no scope, MUST produce output byte-identical to `main`. Repeat scans MUST produce identical output.
- **FR-008**: The documentation of requirements ownership (spec 1537's consumer-guide entry and the C62 row of `docs/reference/sbom-format-mapping.md`) MUST state the ownership rule and the FR-005 invariant.
- **FR-009**: No new Cargo dependencies.

### Key Entities

- **Owner candidate:** a main module or git submodule offered as the owner of one directory, with its rank (spec 1537 R3, spec 1618 R3).
- **Project directory:** the directory of the manifest that declares a main module (FR-001).
- **Root-declared package:** a package a requirements file declares, with no owning project below the scan root, anchored to the document root (spec 1537 R4).
- **Direct runtime dependent:** the document root or a runtime main module with a direct edge or anchor to a component (FR-005).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Before any design choice is fixed, a census at a log-only commit runs on the public corpus (`regen_goldens`), the quality corpus and TheRock. It counts:
  - the directories with more than one owner candidate at the same rank;
  - how many of those candidates are lockfile-merged main modules whose source path is not their manifest's directory, and how many are editable entries whose path is not `.`;
  - how many requirements packages, CMake fetches and searches change owner under FR-001 and FR-003;
  - how many main modules' `source-files` and `workspace-member` change under FR-001;
  - the main modules a dependency group alone lists, and what removing their role would change (FR-002);
  - the components that FR-005 would keep runtime, by ecosystem and by kind of direct dependent (root relationship, root anchor, runtime main module).

  No figure is predicted before the census runs. TheRock runs in CI only, dispatched `--ref main` with full SHAs, and `refs.txt` is verified.
- **SC-002**: #1640's tree, in both name orders, attaches `waybill-fixture-shared` to `waybill-fixture-tool@1.0.0` and not to the dev package, in all three formats. On `main`, the `devtool` order attaches it to the dev package (the control).
- **SC-003**: A dev-group-only editable package owns no requirements file outside its own directory, in every fixture.
- **SC-004**: The public corpus regenerated on the branch differs from the committed goldens only in components traced to an FR. TheRock and the quality corpus, against a `main` control, have equal component counts, and every changed component or edge is traced to an FR.
- **SC-005**: Scan time on TheRock is at most 1.05× `main`'s, median of at least 3 runs.
- **SC-006**: Removing FR-001's directory rule, any step of FR-003's order, or the FR-005 invariant makes at least one test fail.
- **SC-007**: Every fixture scanned 10 times gives byte-identical output, parity-check agrees, and the CycloneDX output validates.

## Assumptions

- **The manifest is the authority for a project's location.** A lockfile records where uv installed a project from, and should agree. When it does not, the tree's manifest says where the project is.
- **A census before design** follows spec 1641's pattern: a log-only commit, a CI workflow in the TheRock fixture repository, and quality-corpus targets at their pins.
- **The large tree in #1640** is TheRock's source tree. Its five root-declared packages are expected in SC-001's census. If they do not appear there, FR-004 records that.
- **Synthetic fixtures only**, named `waybill-fixture-*`.
- **Out of scope:** what makes a package a uv workspace member (`[tool.uv.workspace]`, milestone 106), poetry and Pipfile lockfiles (unless the census finds the same merge there), and #1642's unexplained orphans.
