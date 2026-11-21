# Feature Specification: Name a CMake git fetch with an unresolved reference after its repository

**Feature Branch**: `1638-unresolved-github-fetch`
**Created**: 2026-11-20
**Status**: Draft
**Input**: #1595. A CMake git fetch whose `GIT_REPOSITORY` is a literal GitHub URL, but whose `GIT_TAG` the reader cannot resolve, is emitted as `pkg:generic/<declared name>`. The declared name is often a build target, not the package, so a source that is plainly `github.com/ROCm/aotriton` is reported under a target's name.

## Clarifications

### Session 2026-11-20

- Q: A required unresolved (versionless) fetch and a required resolved (versioned) fetch of one repository: two components, or does the versioned one absorb the versionless one? → A: Two components (FR-014).
- Planning amendment (research R2): spec 1539's wrapper recognition is unchanged. A built-in call whose declared name is a forwarded parameter does not gain an identity of its own from a GitHub repository whose reference is unresolved, so every wrapper `main` recognises stays one and keeps C234 (FR-001, FR-010).
- Planning amendment (research R3): FR-004's URL is recorded on both mechanisms, and its repository identity follows the resolved fetch's: present for `FetchContent_Declare`, absent for `ExternalProject_Add`, whose resolved non-GitHub git fetches have none today. Giving them one would change merges in scans with no unresolved fetch (FR-010), so it is a follow-up.

## Background

Spec 1538 (#1517) reads `FetchContent_Declare` and `ExternalProject_Add`. A `${VAR}` left after expansion means the declaring file has no literal value for it. The reader then says so with C151 `waybill:unresolved-reason` instead of guessing a version, and emits `pkg:generic/<declared name>`, with no version, in the design tier. Spec 1539 (#1518) applies the same rule to wrapper calls, and spec 1540 (#1594) to `download_project()`. All three routes produce their component in the same place.

**Measured on the quality corpus's python-pytorch** (`2b3ec348`, v2.14.0). Spec 1539's branch (quality-corpus run 44105386117, against `main` run 44107875421) adds two components through `cmake/External/aotriton.cmake`'s wrappers, both traced on a sparse checkout at the pin:

| component | how it is declared |
|---|---|
| `pkg:generic/aotriton_external` | `aotriton_build_from_source(OFF aotriton_external)`. Its body is `ExternalProject_Add(${project} GIT_REPOSITORY https://github.com/ROCm/aotriton.git … GIT_TAG ${__AOTRITON_CI_COMMIT})`. `__AOTRITON_CI_COMMIT` is set twice in the file (an environment override, else a literal commit), so it stays unresolved. |
| `pkg:generic/aotriton_runtime` | a URL built with `string(CONCAT …)`. It has no repository identity. |

**Traced in code at `main` `7f1a568a`:**
- **Order of checks.** The unresolved-variable check runs before the GitHub check, so a literal GitHub repository never names the component.
- **No identity.** The unresolved branch records no repository URL. Spec 1538 R9's merge identifies a fetch by its repository, so it never matches a conditional unresolved fetch to a submodule or required fetch of the same repository, which it does for a resolved one.
- **An unreported case** (traced, not measured on a corpus). When the declared name is itself an unexpanded variable, as `${project}` is before a wrapper binds it, and the reference is also unresolved, the component's identifier would contain the variable. The declaration is then dropped and counted only in C231 `unresolved.identity`. A GitHub repository already names such a component when its reference resolves.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A fetched GitHub repository is named after the repository (Priority: P1)

A consumer of a source SBOM looks up the third-party code a CMake build fetches. When the build fetches a GitHub repository at a reference that waybill cannot resolve statically, the consumer sees the repository: `pkg:github/rocm/aotriton`, without a version and with the reason the version is missing. They do not see a build-target name that no registry or advisory knows.

**Why this priority**: A wrong name is worse than a missing version. A consumer can tell that a version is absent, because C151 says so, but cannot tell that `aotriton_external` means `ROCm/aotriton`.

**Independent Test**: Scan a synthetic tree whose CMake file fetches a literal GitHub URL with `GIT_TAG ${UNSET}`. The component is `pkg:github/<owner>/<repo>`, lowercased, with no version, carrying C151, in the design tier.

**Acceptance Scenarios**:

1. **Given** a direct `FetchContent_Declare(x GIT_REPOSITORY https://github.com/O/R.git GIT_TAG ${UNSET})`, **When** scanned, **Then** the component is `pkg:github/o/r` with C151 and no version.
2. **Given** the same fetch through a wrapper function (spec 1539) and through `download_project()` (spec 1540), **When** scanned, **Then** each yields the same identity, and the wrapper chain (C234) is recorded as before.
3. **Given** the python-pytorch pin, **When** scanned, **Then** `aotriton_external` becomes `pkg:github/rocm/aotriton` with C151 and no version, and `aotriton_runtime` is unchanged.
4. **Given** a fetch whose declared name is an unexpanded variable, whose repository is a literal GitHub URL and whose reference is unresolved, **When** scanned, **Then** it is emitted as `pkg:github/<owner>/<repo>` and counted as emitted in C231, not as `unresolved.identity`.

---

### User Story 2 - A fallback fetch merges into the component that already supplies its repository (Priority: P1)

A super-project pins a dependency as a git submodule, and its CMake falls back to fetching the same repository when the submodule is absent, at a reference waybill cannot resolve. The consumer sees one component for that repository: the submodule's. The fallback is recorded on it (C233), as it already is when the fallback's reference resolves.

**Why this priority**: Without the repository identity, the fallback is a second, misnamed component for code that is already listed, which inflates counts and invents a dependency.

**Independent Test**: A synthetic tree with a git submodule of `https://github.com/waybill-org/waybill-fixture-dep.git`, and a conditional fetch of the same URL with `GIT_TAG ${UNSET}` in the same directory. One component for the repository, with the fetch in C233.

**Acceptance Scenarios**:

1. **Given** a submodule of repository R and a conditional unresolved fetch of R in the same or a descendant directory, **When** scanned, **Then** there is one component for R (the submodule's), its C233 lists the fetch's file, and C231 counts the fetch under `merged.existing-component`.
2. **Given** a `FetchContent_Declare` of a literal non-GitHub repository with an unresolved reference, conditional, under a required component of the same repository, **When** scanned, **Then** it merges the same way (FR-004). An `ExternalProject_Add` of such a repository stays separate, as its resolved form does (FR-004).
3. **Given** a conditional unresolved fetch whose repository no enclosing component supplies, **When** scanned, **Then** it is emitted on its own, as in User Story 1.

---

### User Story 3 - Nothing that is not literal changes (Priority: P2)

A maintainer of a repository whose fetches all resolve, or whose unresolved fetches use URLs or unresolved repositories, rescans and sees an identical document.

**Why this priority**: The change must be confined to the case it fixes. The public corpus is the guard.

**Independent Test**: Scan fixtures with a URL fetch with an unresolved version, and a git fetch whose `GIT_REPOSITORY` is itself unresolved. Output is byte-identical to `main`.

**Acceptance Scenarios**:

1. **Given** a URL fetch with an unresolved version, **When** scanned, **Then** it is `pkg:generic/<declared name>` with C151, as today.
2. **Given** `GIT_REPOSITORY ${REPO}` with `REPO` unresolved, **When** scanned, **Then** the output is unchanged.
3. **Given** a scan in which no fetch takes the unresolved branch, **When** scanned, **Then** the output is byte-identical to `main`.

### Edge Cases

- **Two unresolved fetches of one GitHub repository**, under different declared names, now share an identity. They become one component, its evidence listing both files, and are counted accordingly in C231.
- **A required unresolved fetch and a required resolved fetch of one repository** stay two components: one versionless, one versioned (FR-014). Merging would assert that the unresolved declaration fetches the resolved one's commit, which FR-002 forbids; two required declarations can fetch different commits.
- **A conditional resolved fetch beneath a required unresolved fetch** of the same repository. R9 already merges a conditional fetch into the nearest required supplier of its repository, so the resolved fallback is recorded, with its version, in C233 of a versionless component. This spec keeps R9's rule as it is.
- **A conditional unresolved fetch merged into a supplier** has no version to record in C233, whose entries are `{"file", "version"}`. It is recorded with `"version": ""`, as spec 1538's name-based merge (R5) already records an unresolved declaration (traced at `7f1a568a`: both merges write the lost declaration's version, which is empty for an unresolved fetch). C233's catalog row states this.
- **A GitHub URL that is not a repository root** (`/tree/…`, an archive or release-asset URL) under `GIT_REPOSITORY` keeps the resolved path's existing owner/repo rule. This spec applies the same rule, not a new one.
- **A repository URL with credentials.** A resolved CMake git fetch records its `GIT_REPOSITORY` in `waybill:download-url` verbatim; no credential stripping applies to CMake download URLs today (traced at `7f1a568a`). FR-004 records the URL in the same form. Stripping credentials from CMake download URLs is an existing gap, outside this spec.
- **The reference is literal but the repository is unresolved.** That case is unchanged (FR-006).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A git fetch whose `GIT_REPOSITORY` is a literal GitHub URL and whose reference (`GIT_TAG`) is unresolved MUST be emitted as `pkg:github/<owner>/<repo>`, with no version, lowercased per spec 1530 FR-015. The owner/repo rule is the one a resolved GitHub fetch already uses. It MUST keep C151 with its current reason, the design tier, its C55 source mechanism, its scope and conditions (C122, C232/C236) and its wrapper chain (C234).
- **FR-002**: No version MUST be taken from any candidate value of an unresolved variable, including a variable set more than once or overridden from the environment.
- **FR-003**: FR-001 MUST apply identically on every route into a fetch: direct `FetchContent_Declare`, direct `ExternalProject_Add`, spec 1539 wrapper calls, and spec 1540 `download_project()`.
- **FR-004**: An unresolved git fetch with a literal repository that is not on GitHub MUST keep `pkg:generic/<declared name>` and C151. It MUST also carry its repository URL, in the same form a resolved git fetch records it (`waybill:download-url`). Its repository identity MUST be the one a resolved fetch of the same mechanism has: a `FetchContent_Declare` gains one, so the spec 1538 R9 merge can use it; an `ExternalProject_Add` has none, as a resolved non-GitHub `ExternalProject_Add` has none today (research R3).
- **FR-005**: A fetch given by `URL` with an unresolved version MUST be unchanged. It has no repository identity.
- **FR-006**: A git fetch whose `GIT_REPOSITORY` is itself unresolved MUST be unchanged.
- **FR-007**: A conditional unresolved fetch whose repository an enclosing required component supplies (a git submodule, a resolved or unresolved required fetch, another reader's component) MUST merge into that component under spec 1538 R9's existing rule. That rule covers the supplier choice, C233 and C231's `merged.existing-component`. The survivor MUST keep its own identity, version, C55, scope and conditions.
- **FR-008**: A fetch whose declared name is an unexpanded variable and that meets FR-001 MUST be emitted, named after the repository, and counted in C231 as emitted rather than `unresolved.identity`. C231's accounting equation MUST still hold.
- **FR-009**: The three formats MUST agree on the changed components and their annotations: C55, C151, C122, C232, C233, C234, C236 (`waybill sbom parity-check`). No two components may share a `bom-ref`.
- **FR-010**: Output MUST be byte-identical to `main` for any scan in which no fetch takes the unresolved branch with a literal repository.
- **FR-011**: Repeat scans MUST produce identical output.
- **FR-012**: No new Cargo dependencies.
- **FR-013**: A declaration with no version recorded in C233 MUST appear with `"version": ""`, on both merge paths (R5 by name, R9 by repository). The C233 catalog row MUST say so.
- **FR-014**: A required unresolved fetch MUST NOT merge into a required resolved fetch, or any other required component, of the same repository. Both are emitted. Only a conditional fetch merges by repository (FR-007).

### Key Entities

- **Unresolved git fetch**: a fetch declaration with a `GIT_REPOSITORY` and a `GIT_TAG`, at least one of which still contains a variable after same-file expansion. This spec distinguishes the repository being literal from it being unresolved.
- **Repository identity**: the repository a component stands for, without its version (spec 1538 R9). Today an unresolved fetch has none. Under FR-001, one with a literal GitHub repository has one on either mechanism. Under FR-004, a `FetchContent_Declare` of another literal repository has one, and an `ExternalProject_Add` of one does not (research R3).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A census, run before design, counts the fetches that take the unresolved branch on TheRock (CI only, census-fetches in `kusari-sandbox/waybill-fixture-therock-sdk`) and on the three quality-corpus targets. It splits them into a literal GitHub repository, a literal other repository, an unresolved repository, and a URL. The research records the counts; this spec quotes none in advance.
- **SC-002**: On python-pytorch, `aotriton_external` becomes `pkg:github/rocm/aotriton` with C151 and no version, and `aotriton_runtime` is unchanged. The quality corpus's three targets are traced component by component against a `main` control, and every change is explained by FR-001 to FR-008.
- **SC-003**: TheRock's source tree (measure-source against `main`): every component that changes is traced to an FR. Unexplained orphans do not increase.
- **SC-004**: The public corpus is regenerated (`regen_goldens`), and every golden that changes is traced. Targets where no fetch takes the unresolved branch with a literal repository are byte-identical.
- **SC-005**: Parity-check agreement in all three formats, no duplicate `bom-ref`s, and identical repeat scans on two runners.
- **SC-006**: Scan time on TheRock's source tree is at most 1.05× `main`'s, measured as the median of at least 3 runs.

## Assumptions

- `pkg:github` without a version is a valid PURL, and is the form a GitHub source takes when its version is unknown. The resolved path already lowercases owner and repository (spec 1530 FR-015).
- C151's existing reason string ("URL or GIT_TAG uses a CMake variable with no literal set() in the same file") still describes these components accurately, so no new value is needed.
- This spec changes identity, not version resolution. Resolving references from environment overrides, cache variables or other files is out of scope (#1517).
- Whether a reference names a moving branch is #1598, not this spec.
- TheRock and the quality corpus are measured only in CI. Fixtures use synthetic `waybill-fixture-*` names.
