# Feature Specification: Scope propagation protects every runtime project, not one per ecosystem

**Feature Branch**: `1616-propagation-seeds`
**Created**: 2026-11-19
**Status**: Draft
**Input**: #1618. Spec 1269's scope propagation marks a project's declared runtime dependencies non-runtime when that project is not the one main module per ecosystem it protects.

## Background

Spec 1269 propagates a non-runtime scope (dev, test, optional, …) from declared non-runtime components to what they reach. It runs in two phases:
- **Phase A (immunity).** Everything a runtime path from the **seed set** reaches is protected: it can never receive a propagated scope.
- **Phase B (propagation).** Starting from each declared non-runtime component, every component reached that is not immune and has no scope of its own takes that scope, marked C62 `transitive`.

The seed set is the document's primary root plus **one "top" main module per ecosystem**. Spec 1269 chose this set deliberately: its contract (P3) has immunity computed from the same roots that the emitters' completeness reporting uses.

In a tree with several projects of one ecosystem, only one of them is a seed. The others' declared runtime dependencies are not protected by their own project. Spec 1269 names the result the dangerous direction: a dependency that ships is reported as not shipping.

**Measured on ROCm TheRock.** CI measure-source runs 44550926162 and 44550947405: waybill `fbb168ab` against `main` `52b96f1d`, TheRock `b1cae390` with submodules, scanned only in CI.
- **The cause.** TheRock has 64 main modules. Two required pip main modules are not the seeded one:
  - `pkg:pypi/llvm@0.0.0-unknown`, declaring `compiler/amd-llvm/llvm/utils/git/requirements*.txt`;
  - `rocprof-compute`, declaring `rocprofiler-compute/requirements.txt`.
- **Before spec 1615.** On `main` their pins were protected only by accident: runtime-labelled Sphinx docs packages also reached them (`rocm-docs-core → pygithub → requests → certifi`, `matplotlib → numpy`).
- **After spec 1615.** Spec 1615 correctly labelled those docs packages non-runtime. **11 packages** declared in these runtime files then came out non-runtime with C62 `transitive`: pygithub, pynacl ×2, numpy, certifi, cryptography ×2, pyjwt ×2, requests, typing-extensions. Spec 1615 records them as an exception to its FR-007 and SC-002, pending this feature.

**How many other components are affected is not yet measured.**
- **A graph walk over the emitted documents is not reliable.** It suggested about 250 on TheRock `main`. But it also flags a component in `nix-closure-slacklinker`, whose single main module is necessarily the seed, so it over-counts. It does not model spec 1269's edge exclusions (inferred cross-ecosystem edges, nix build-graph edges).
- **Candidate corpus targets from that walk:** go-opentelemetry (29 main modules), haskell-security-advisories, and the nix-closure targets moat, slack-web and slacklinker.
- **Unlikely movers:** python-flask, pytorch, ansible and mongo show no candidates.

The real population is measured by running the change (SC-002).

**Traced in the source at `main` `1647ee01`.**
- **The call.** The propagation runs in `scan_cmd` on the final component list. Its seeds come from `component_root_seeds`, after the root is selected.
- **Main modules at that point.** They still carry their main-module role. Milestone 149's demotion of main modules to libraries (`waybill:demoted-from-main-module`) happens later, when the documents are built, and only under an operator root override.
- **Ecosystems with no seed.** Spec 1269's FR-012 forbids propagating into an ecosystem that has no seed at all.

## Clarifications

### Session 2026-11-19

- Q: Do the wider seeds apply only to the propagation's immunity search, or to graph completeness too? → A: Only to the immunity search. Graph completeness and the rows it feeds are unchanged; the departure from spec 1269's P3 is recorded in FR-004.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every project's runtime dependencies stay runtime (Priority: P1)

An SBOM consumer scanning a monorepo sees a package that a project declares as a runtime requirement marked as a dev or docs dependency. This happens because a dev or docs tool elsewhere in the tree also uses that package. On TheRock, `pygithub`, `numpy` and `requests` are among 11 such packages.

**Why this priority**: The defect hides software that ships, which is the failure spec 1269 was built to prevent.

**Independent Test**: Scan a tree with two projects of one ecosystem. The non-top project declares `waybill-fixture-shipped` as a runtime dependency, and a dev-scoped component also depends on it. The package is required, with no C62 `transitive`.

**Acceptance Scenarios**:

1. **Given** projects `a` (the seeded one) and `b` of one ecosystem, `b` declaring `waybill-fixture-shipped` at runtime, and a dev-scoped `waybill-fixture-devtool` that depends on it, **When** the tree is scanned, **Then** `waybill-fixture-shipped` is required, without C62.
2. **Given** the same tree where `b`'s runtime dependency has its own runtime dependency, **When** it is scanned, **Then** that one is required too: the whole runtime closure of `b` is protected.
3. **Given** a package reached only from the dev-scoped component and from no project at runtime, **When** the tree is scanned, **Then** it still takes the dev scope with C62 `transitive`, as today.

---

### User Story 2 - A non-runtime project protects nothing (Priority: P1)

A docs site or test harness can be a project of its own, a main module that is itself non-runtime. Its dependencies must not become immune because of it: that would undo spec 1269 for exactly the trees it was written for.

**Why this priority**: Without this, the fix over-protects and reverses spec 1269's propagation, silently.

**Independent Test**: Scan a tree where a main module is itself dev-scoped. Its dependencies, reached by no runtime path, take the propagated scope as today.

**Acceptance Scenarios**:

1. **Given** a main module with a non-runtime scope that depends on `waybill-fixture-docdep`, **When** the tree is scanned, **Then** `waybill-fixture-docdep` keeps the non-runtime scope propagation gives it on `main`.

---

### User Story 3 - The record shows how much the wider protection did (Priority: P3)

An operator comparing two waybill versions on one project sees fewer C62 `transitive` components and needs to know that a rule changed, not the project.

**Why this priority**: Diagnostic. The scopes themselves are what consumers read.

**Independent Test**: Scan a tree where the wider protection keeps 2 packages runtime. The propagation's INFO summary reports 2 components protected only by the added seeds.

**Acceptance Scenarios**:

1. **Given** US1's tree, **When** it is scanned, **Then** the propagation summary log reports the number of components that only the added seeds protected.

---

### Edge Cases

- **One main module in an ecosystem.** It is already the seed, so the output is unchanged.
- **Main module reached from the primary root.** Already protected, so unchanged.
- **Cross-ecosystem.** A runtime dependency of a non-top main module of ecosystem X, reached by a dev component of ecosystem Y over a declared edge, is protected.
- **Excluded edges.** Inferred cross-ecosystem edges (milestone 218) and nix build-graph edges (milestone 1280) are not traversed from the new seeds either, so they confer no protection.
- **Ecosystem with no seed.** An ecosystem with no main module stays under spec 1269's FR-012, unchanged.
- **Operator root override.** With `--root-name`, main modules keep their role at propagation time (milestone 149 demotes later), so they are seeds as without the override.
- **Scope only grows.** No component may gain a non-runtime scope that it does not have on `main`. Every change is a loss of C62 `transitive`.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Spec 1269's immunity search MUST start from every main module that is itself runtime, in addition to its current seeds. "Runtime" means the main module has no non-runtime lifecycle scope of its own.
- **FR-002**: A main module with a non-runtime scope MUST NOT be a seed (US2).
- **FR-003**: The immunity search MUST traverse from the new seeds under the same rules as today, with the same excluded edge types and the same stop at components with a non-runtime scope of their own.
- **FR-004**: The wider seeds MUST apply only to the propagation's immunity search. Graph completeness keeps its own roots, and every catalog row it feeds (orphan reasons, the completeness record) is unchanged. This departs on purpose from spec 1269's contract P3, which shares one seed set with completeness: every newly protected component sits under a main module the scan found, while the document still declares one root per ecosystem. Whether completeness should also treat every main module as a root is a separate decision.
- **FR-005**: The change MUST only remove propagated scopes. A component's scope on `main` is either kept or, if it was propagated (C62 `transitive`), may become runtime; nothing may become newly non-runtime.
- **FR-006**: The propagation's INFO summary MUST report how many components only the added seeds protected. No new catalog row is added: a per-document count would change every document's bytes for a diagnostic figure, which is what spec 1276's T028 measured and rejected.
- **FR-007**: Spec 1615's exception to its FR-007 and SC-002 MUST be removed when this feature lands, with the 11 TheRock packages required again.
- **FR-008**: A tree with at most one main module per ecosystem MUST produce output byte-identical to `main`.
- **FR-009**: No new Cargo dependencies.

### Key Entities

- **Seed set**: the components the immunity search starts from. Today: the primary root and one main module per ecosystem. After this feature: also every other runtime main module.
- **Main module**: a component carrying the main-module role at propagation time, a project the scan found in the tree.
- **Propagated scope**: a non-runtime scope assigned by spec 1269, marked C62 `transitive`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock's source tree, the 11 packages are required, without C62 `transitive`.
- **SC-002**: In every corpus measured (TheRock, the public corpus with regenerated goldens, and the quality corpus against a `main` control), every component whose scope changes:
  - loses a C62 `transitive` scope and becomes runtime;
  - is traced to a runtime main module that is now a seed and reaches it over a traversable edge.

  The count is recorded per target. It is a measurement, not a prediction.
- **SC-003**: No component in any corpus gains a non-runtime scope, and no component is added or removed.
- **SC-004**: The three formats agree on C42 and C62 (`waybill sbom parity-check` shows no gap that `main` does not). There are no duplicate `bom-ref`s, and two scans at one commit on two runners are identical, timestamps and serials aside.
- **SC-005**: TheRock's source scan time stays within 1.05× `main`'s median, measured with the interleaved measure-source workflow.
- **SC-006**: Each rule (FR-001, FR-002, FR-003, FR-005) has a mutation that a test catches.

## Assumptions

- **Seed recognition.** At propagation time a main module carries the main-module role; milestone 149's demotion runs later, at document generation. Planning verifies this, and if it is wrong the seed recognition follows whatever marks a main module there.
- **The 11 packages and their cause** are measured. The wider population is not, and is labelled so.
- **TheRock is measured only in CI.** Fixtures use synthetic `waybill-fixture-*` names.
