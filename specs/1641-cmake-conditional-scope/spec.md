# Feature Specification: A CMake condition that holds by default does not make a fetch optional (#1639)

**Feature Branch**: `1641-cmake-conditional-scope`
**Created**: 2026-11-21
**Status**: Draft
**Input**: #1639. A CMake fetch inside any `if()` is marked optional and excluded, including conditions that are always true in the build being described. Refs #1591, #1605, #1516, #1519, #1623, #1624.

## Clarifications

### Session 2026-11-21

- Q: What does a fetch whose conditions evaluate to unknown get (FR-004)? → A: It is kept in scope, with C232 as the record of what gates it. This is spec 1269's safe direction: over-reporting an optional feature is the safer error. SC-001's census reports how many components this affects.
- Q: Do spec 1614's package searches follow the same rule (FR-007)? → A: Yes. They use the same evaluation, with C236 as the record. It is the same defect, and #1623 fixed its installed-tree form.

## Background

Spec 1538 research R2 made a conditional CMake fetch `Optional`. This is carried as CycloneDX `scope: excluded` and SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`, with C122 `waybill:optional-derivation` naming the shape. The shapes are tried in a precedence order, and the first match is the reason given:
1. `cmake-find-package-args`
2. `cmake-not-target`
3. `cmake-find-package-fallback`
4. `cmake-try-find-package-always`
5. `cmake-dependency-provider`
6. `cmake-function-body`
7. `cmake-conditional`: any other enclosing `if()`, `elseif()` or `else()`

C232 `waybill:cmake-fetch-conditions` records every condition.

The first six shapes are fallbacks a super-build does not take (#1519). They are deliberate, and this spec does not change them. `BUILD_TESTING` making a fetch Test (spec 1538 R7) is also unchanged.

The seventh is the catch-all, and it cannot tell an optional feature from a condition that holds in the build being described. Spec 1614's package searches use the same catch-all: a library searched for under any condition is Optional, its conditions recorded in C236 `waybill:cmake-find-conditions`. #1623 already exempts a record that ships a library file in an installed tree.

**Measured in #1639** at `main` `786091a8`. Each fixture is the same `ExternalProject_Add` under a different enclosing condition:

| Enclosing condition | scope | lifecycle-scope |
|---|---|---|
| none | required | – |
| `if(CMAKE_SYSTEM_NAME STREQUAL "Linux")` | excluded | optional |
| `if(NOT CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)` (super-build guard) | excluded | optional |
| `option(X "" ON)` + `if(X)` | excluded | optional |
| `option(X "" OFF)` + `if(X)` | excluded | optional |

Only the last row is optional in the default configuration.

**The cost.** In one multi-project tree that bundles its system libraries, this excluded 103 components, 35 of them versioned (zlib, bzip2, expat, xz, zstd, ncurses and 14 more). A consumer that drops excluded components, which is what the scope is for, loses the libraries that ship. Spec 1269 names reporting what ships as not shipping the dangerous direction.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A bundled library under a condition that holds stays in scope (Priority: P1)

A team scans a source tree that fetches its third-party libraries under platform checks, a super-build guard, or options that default to ON. They give the SBOM to a vulnerability scanner that ignores excluded components. Every one of those libraries is scanned.

**Why this priority**: This is the defect. The excluded libraries are the ones that ship.

**Independent Test**: For each condition in the Background's table, scan a fixture with one fetch under it. Under a platform check, the super-build guard or a default-ON option, the component is required, and C232 still records the condition. Under a default-OFF option it stays optional.

**Acceptance Scenarios**:

1. **Given** a fetch under `if(CMAKE_SYSTEM_NAME STREQUAL "Linux")`, **When** scanned, **Then** the component is in scope in every format, and C232 lists the condition.
2. **Given** a fetch under the super-build guard, **When** scanned, **Then** the component is in scope.
3. **Given** `option(X "" ON)` and a fetch under `if(X)`, **When** scanned, **Then** the component is in scope.
4. **Given** `option(X "" OFF)` and a fetch under `if(X)`, **When** scanned, **Then** the component is optional with C122 `cmake-conditional`, as today.

---

### User Story 2 - Combined and branching conditions are read as CMake reads them (Priority: P1)

Real trees combine conditions, branch with `elseif` and `else`, and nest `if()`s. The scope reflects whether the fetch runs in the default configuration on some supported platform, not merely whether a condition exists.

**Why this priority**: Most bundled fetches in large trees sit under more than one condition. A rule that reads only simple guards would leave them wrong.

**Independent Test**: Fixtures for a compound condition, an `else()` branch and nesting each give the scope the evaluation rule (FR-003) predicts.

**Acceptance Scenarios**:

1. **Given** a fetch under `if(WIN32 AND X)` with `X` defaulting OFF, **When** scanned, **Then** it is optional.
2. **Given** a fetch in the `else()` of `if(WIN32)`, **When** scanned, **Then** it is in scope: the `else` of a platform check is another platform.
3. **Given** a fetch in the `else()` of `if(X)` with `X` defaulting ON, **When** scanned, **Then** it is optional: that branch runs only when `X` is OFF.
4. **Given** a fetch under an `if()` nested inside another `if()`, **When** scanned, **Then** it is in scope only if every enclosing condition holds by default.

---

### User Story 3 - Nothing else changes (Priority: P2)

Fallback detection, test scoping, merging and the coverage record behave as before, except where FR-001 changes a scope.

**Why this priority**: The six fallback shapes and R9's merging were each measured on TheRock and corrected there. A scope fix must not undo them.

**Independent Test**: The fallback-shape fixtures, the `BUILD_TESTING` fixtures and an R9 merge into a submodule produce the same output as `main`. So does a scan with no catch-all conditional.

**Acceptance Scenarios**:

1. **Given** a fetch under `if(NOT TARGET x)`, **When** scanned, **Then** it is optional with `cmake-not-target`, as today.
2. **Given** a conditional fetch of a repository that an enclosing submodule supplies, **When** scanned, **Then** it merges into the submodule's component as spec 1538 R9 specifies.
3. **Given** a tree with no catch-all conditional fetch or search, **When** scanned, **Then** the output is byte-identical to `main`.

### Edge Cases

- **An option with no visible default:** no `option()`, `cmake_dependent_option()` or `set(… CACHE BOOL …)` is found for it. Its value is unknown, so the fetch is in scope (FR-004).
- **An option defined twice with different defaults:** both definitions are in reach (FR-002). Its value is unknown.
- **`cmake_dependent_option(X "" ON "DEP" OFF)`:** X is ON by default only when DEP holds by default. Otherwise it takes the fallback value.
- **A plain `set(X ON)`** in reach is a literal and evaluates like a default. A `set(X ${Y})` is unknown.
- **A platform predicate combined with an option:** `if(WIN32 AND X)` is decided by `X`, because a platform predicate holds on some supported platform.
- **`elseif`:** its branch holds when every earlier branch does not, and its own condition holds.
- **Interaction with `BUILD_TESTING`:** a fetch under `BUILD_TESTING` is Test (spec 1538 R7) whatever else encloses it. Test outranks the new rule, as it outranks Optional today.
- **Expressions the rule cannot read:** a generator expression, `DEFINED`, `EXISTS`, `VERSION_GREATER`, `MATCHES` and similar are unknown (FR-004). The one exception is the super-build guard's string comparison.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A CMake fetch whose only reason for being optional today is `cmake-conditional` MUST be in scope when its enclosing conditions hold in the default configuration on some supported platform (FR-003). Otherwise it stays Optional with C122 `cmake-conditional`. The six specific fallback shapes and `BUILD_TESTING` MUST be unaffected.
- **FR-002**: An option's default MUST be read from an `option()`, `cmake_dependent_option()`, `set(… CACHE BOOL …)` or plain literal `set()`. Only definitions in reach of the fetch count: the same file before the fetch, an enclosing directory's `CMakeLists.txt`, or a file `include()`d into either before the fetch. Definitions with different values make the option unknown.
- **FR-003**: Each enclosing condition MUST evaluate to one of: *holds by default*, *does not hold by default*, or *unknown*.
  - **Atoms:**
    - a platform predicate (`WIN32`, `UNIX`, `APPLE`, `LINUX`, `MSVC`, `CYGWIN`, `MINGW`, `ANDROID`, `IOS`, `CMAKE_SYSTEM_NAME`, `CMAKE_SYSTEM_PROCESSOR`, `CMAKE_HOST_*`, `CMAKE_CXX_COMPILER_ID`, `CMAKE_C_COMPILER_ID`, alone or compared with a literal) holds, and so does its negation;
    - the super-build guard, in either operand order, holds;
    - an option or literal evaluates to its default (FR-002);
    - anything else is unknown.
  - **Operators:** `NOT`, `AND`, `OR` and parentheses combine values with three-valued logic.
  - **Branches:** an `else()` holds when the matching `if` (and any `elseif`) does not, except that the else of a platform-only condition holds. An `elseif` holds when the earlier branches do not and its own condition does.
  - **The chain:** a fetch's scope is decided by the conjunction of every enclosing branch.
- **FR-004**: A fetch whose conditions evaluate to unknown MUST be kept in scope. C232 is the record of what gates it (Clarifications). Only a chain that does not hold by default leaves a fetch Optional with C122 `cmake-conditional`.
- **FR-005**: C232 MUST still record every enclosing condition of every fetch, in scope or not. A kept-in-scope fetch carries no C122. No new catalog row is added: C232 already says what gates the fetch, and the native scope says whether it ships.
- **FR-006**: Spec 1538 R9 MUST keep merging a fetch on the presence of a condition, independent of its new scope. C231's `conditional` count keeps its definition, the number of Optional fetch components, so it falls by the number FR-001 brings into scope. C231 V2 (`conditional + test <= components`) still holds.
- **FR-007**: Spec 1614's package searches MUST follow the same evaluation (FR-002 to FR-004) for their catch-all conditional (a search under any condition that is not a test directory or `BUILD_TESTING`). C236 remains the record of every condition. A library is Optional only when every search for it is in a chain that does not hold by default, and in scope when any search holds or is unknown.
- **FR-008**: The three formats MUST agree (`waybill sbom parity-check`). CycloneDX MUST validate against the vendored 1.6 schema, and no two components may share a `bom-ref`.
- **FR-009**: A scan with no catch-all conditional fetch or search MUST produce output byte-identical to `main`. Repeat scans MUST produce identical output.
- **FR-010**: The C122 and C232 rows of `docs/reference/sbom-format-mapping.md` and the consumer guide MUST state when a conditional fetch is in scope.
- **FR-011**: No new Cargo dependencies.

### Key Entities

- **Enclosing condition:** one `if()`, `elseif()` or `else()` reaching a fetch or search, as C232 and C236 record it.
- **Default value:** an option's or variable's value in reach when nothing overrides it on the command line (FR-002).
- **Evaluation:** *holds by default*, *does not hold by default*, or *unknown*, for one condition and for the chain (FR-003).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Before any design choice is fixed, a census at a log-only commit counts the catch-all conditional fetches and searches on TheRock, the quality corpus and the public corpus. It splits them by category (platform, super-build guard, option ON, option OFF, literal, unknown, compound) and counts how many change scope under each choice for FR-004, and how many of those are versioned. No figure is predicted before the census runs.
- **SC-002**: Every row of the Background's table, and each User Story 2 scenario, has a fixture that gives the stated scope in all three formats. On `main`, each fixture that this spec changes gives Optional (the control).
- **SC-003**: The six fallback-shape fixtures, the `BUILD_TESTING` fixtures and an R9 merge give the same output as on `main`.
- **SC-004**: The public corpus regenerated on the branch (`regen_goldens`) differs from the committed goldens only in components whose scope FR-001 or FR-007 changes, each traced. TheRock's source tree and the quality corpus, against a `main` control, have equal component counts, and every changed component is traced to an FR.
- **SC-005**: Scan time on TheRock's source tree is at most 1.05× `main`'s, median of at least 3 runs.
- **SC-006**: Removing any one evaluation rule (platform, super-build guard, option default, literal, operator, `else`, `elseif`, nesting) makes at least one test fail.
- **SC-007**: Every fixture scanned 10 times gives byte-identical output, parity-check agrees, and the CycloneDX output validates.

## Assumptions

- **"Supported platform"** means a platform the tree declares any condition for. waybill describes a source tree, not one build of it, so a library fetched only on Windows is still a library the tree ships for Windows.
- **Command-line overrides** (`-DX=OFF`) are unknowable from the tree. The default is what the SBOM describes, as for every other reader.
- **A census before design** follows spec 1639's pattern: a log-only commit, a CI workflow in the TheRock fixture repository, and quality-corpus targets at their pins.
- **Out of scope:** the six specific fallback shapes, Conan's `conan-conditional-requires`, and installed trees (#1623).
