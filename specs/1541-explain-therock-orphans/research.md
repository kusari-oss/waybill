# Research: every unreachable component on TheRock explains itself

**Evidence:** `measurements/source-trace.md` (the FR-001 trace of TheRock's source tree) and the code reading below. The SDK trace at current `main` is pending: `measure-full` run 44173368036, recorded by T001. The decisions here do not depend on its numbers, because the SDK's dominant population (nested components) is settled by clarification Q1.

## R1. One reachability answer (FR-002)

**Finding.** Four call sites compute reachability with `compute_graph_completeness`: the CycloneDX builder, SPDX 2.3, SPDX 3 and the orphan classifier. The three emitters first prepare their inputs:
- `effective_components`, which drops main modules an operator root replaces;
- `anchor_retained_mainmod_edges` and `anchor_resolve_components`;
- workspace-peer edges;
- the root selection from the operator's `--root-*` flags.

The classifier (`classify_orphans_pre_emit`) instead uses `RootComponentOverride::default()`, a target ref of `<name>@0.0.0`, raw relationships and peer edges. `source-trace.md` shows the consequence on TheRock: the classifier counts 4 npm components reachable that the document counts unreachable, so they get no code.

**Decision.**
- **One entry point.** Add `graph_completeness::for_document(…)`. It takes the components, relationships, root override, scan-target coordinate, target name, version and kind, does the emitters' preparation once, and returns the `GraphCompletenessResult`.
- **Callers.** The three emitters call it in place of their own preparation, and the classifier calls it with the **scan's real root override and version** (the same values `ScanArtifacts` carries).
- **Agreement by construction.** The four answers are then one computation.

**Alternative rejected.** Passing the operator override into the classifier but keeping its own preparation. It would leave the anchoring steps different, which is how #1566 arose (spec 1537 R6).

## R2. Containment counts as reachability (FR-003, clarification Q1)

**Finding.** Nesting is `ResolvedComponent.parent_purl`. CycloneDX renders it as `components[].components[]`, and SPDX 2.3 and 3 render it with `CONTAINS`. Two passes ignore it:
- **The completeness BFS** walks only dependency relationships.
- **The I7 check** (`corpus_harness_195/invariants/checks/orphan_explained.rs`) reads only top-level `components[]` (`cdx_components`), so nested components are not in its population at all.

**Decision.**
- **In `for_document`,** add an edge from `parent_purl` to each child before the BFS. A child of a reachable container is then reachable. A child of an unreachable container stays unreachable and is explained like any orphan.
- **In the I7 check,** walk nested components too, treating containment as reachability, so the harness and the emitter answer alike.
- **No dependency edge is emitted.** The wire is unchanged except where completeness and orphan markers follow from it.

## R3. Binaries found by binary analysis (FR-004, clarification Q2)

**Finding** (`source-trace.md`): 787 unexplained `pkg:generic` components from binary analysis (`waybill:sbom-tier` = `analyzed`).
- **572** have every source file under a test-data directory.
- **215** do not: 212 `.co` GPU code objects in `hip-kernel-provider/src`, 2 `.hsaco` and 1 `.winmd`.
- **One `pkg:deb` glibc** was identified inside kpack's `test_assets` executables.

**Decision.** Two new C45 codes, below every existing code:
- **`test-data-binary`.** Unreachable; derived from binaries (`analyzed` tier, or every source file a binary the scanner read); every source file under a test-data directory.
- **`unowned-binary`.** The same, not under one.

**The test-data directory list is closed:** `test`, `tests`, `testdata`, `test-data`, `test_data`, `testsuite`, `test-suite`, `unittests`, `Inputs`, `fixtures`, `test_assets`, `test-assets`. Each is a path segment, matched case-sensitively. On TheRock it measured `test` 542, `Inputs` 464, `unittests` 10, `tests` 2, `testsuite` 2, `fixtures` 1, plus kpack's `test_assets`; the others are their common spellings.

**Rationale.** A consumer filtering findings acts differently on a test fixture and on a prebuilt binary committed to the tree. "Unowned" states only what the scan knows: no package claims the binary.

## R4. Lockfile packages with no owner: a reader gap (FR-005)

**Finding.**
- **nccl4py.** The lock's root entry is `source = { editable = "." }` with no `version`, because the project declares a dynamic version. `uv_lock.rs` skips any entry without a version, so the root is dropped, and with it every edge to its `dependencies`, `optional-dependencies` (`cu12`, `cu13`) and `dev-dependencies` (read at rocm-systems `c1ab6df5` through the GitHub API).
- **tensilelite.** Its root (`tensile@5.0.0`) is emitted and reachable. 14 of its packages (dev tools such as `coverage`, `pytest-cov`, `pre-commit` and their dependencies, and two `numpy` versions) are not.

**Decision.** In the uv reader:
- **Emit an editable root with no `version`** as a versionless component with C151 (`waybill:unresolved-reason`, "dynamic version in pyproject.toml"), so it is attached as any project is (spec 1537) and its edges exist.
- **Wire the root's `dev-dependencies` edges** with development scope, and its `optional-dependencies` edges with optional scope, as the reader already marks those packages.
- **Wire every listed version of a forked dependency** (`numpy` 2.2.6 / 2.4.6 / 2.5.1 under markers).

Each of the 35 is attributed by T-trace after these changes, from the CI document. Any that remain unexplained get an existing code or are traced further.

## R5. The remaining small populations (FR-007)

**Expected to receive existing codes once R1 lands:**
- **cargo (8):** older versions in `mirage/Cargo.lock`, `dead-lockfile-entry`;
- **npm (4):** `hoisted-unused` or `dead-lockfile-entry`;
- **conan (2) and vcpkg (2):** `foreign-build-system`, when no component of their ecosystem is reachable.

**Traced one by one at implementation:** `pkg:generic/tensile` (tensilelite's `pyproject.toml` workspace root) and `pkg:generic/wkmi@1.0.0` (a CMake package-version file). A population that is a waybill defect is declared in `invariant-declarations.toml` with an issue.

## R6. #1214 (FR-008)

Feature 1218 explained #1214's corpus populations, apart from `maven-guice`'s, which stays declared under #1235. This feature re-opens none of them. The corpus run shows any it newly explains.

## R7. Cost

*Inference.* `for_document` moves work rather than adding it, and containment adds one edge per nested component. SC-007 is measured by interleaved `measure-source`.
