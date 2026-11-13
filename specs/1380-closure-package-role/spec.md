# Feature Specification: A closure member is a `package` only when it is one

**Feature Branch**: `1380-closure-package-role`
**Created**: 2026-11-13
**Status**: Draft
**Issue**: #1374

> **Numbering note.** This is spec **1380**, and it resolves issue **#1374**. Spec 1374 (`1374-exclude-members-from-coverage`) resolved issue #1380.
**Input**: User description: "#1374 — The Nix closure member role `package` (C206 waybill:closure-member-role) is assigned by a version-shaped token anywhere in the name, so on nix-closure-moat all 12 `package` members are source trees, a patch file or an env (clang-src-21.1.8, clang-at-least-16-LLVMgold-path.patch, python3-3.13.13-env). Decide, from measurements on at least three closures (moat, slacklinker, shellcheck), whether the role has a real population (bootstrap packages like mes-libc-mini-0.27.1) and whether it is separable by name (suffix-only version token; exclusions for .patch, -src-<v>, -source, -env), or whether it folds into `source`. The role is a frozen four-value enum that a consumer can observe, and the change regenerates corpus goldens. Dependency edges must not change, because is_edge_endpoint() is true for both roles."

## Context

C206 `waybill:closure-member-role` names what a closure member that declares no `pname` **is**. It is a frozen four-value enum: `source`, `package`, `scaffolding` and `fragment`. A member is a `package` when nothing earlier claims it and its name contains a version-shaped token anywhere.

**The issue's numbers are out of date.** #1374 was measured before later milestones widened the closure. Re-measured on 2026-11-13 from the committed goldens (`measurements/package-role-census-2026-11-13.txt`):

| closure | `package` members | genuine packages | version checks | test programs | source trees, patch, vendor | env |
|---|---|---|---|---|---|---|
| moat | 73 | 23 | 39 | 7 | 4 | 0 |
| shellcheck | 73 | 23 | 39 | 7 | 4 | 0 |
| slacklinker | 93 | 29 | 40 | 7 | 12 | 5 |
| openai | 4 | 3 | 0 | 0 | 1 | 0 |
| slack-web | 4 | 3 | 0 | 0 | 1 | 0 |

The "genuine packages" column is the census's `other` group, read by hand. Its examples, and the other groups' members:

- **Genuine packages.** The `mes`/`tinycc` minimal-bootstrap chain (`mes-libc-mini-0.27.1`, `tinycc-boot0-libs-…`), `glibc-iconv`, `libidn2`, `llvm-binutils`, `auditable-cargo` and the `rustc` wrappers.
- **Version checks.** `*-get-version-<v>`.
- **Test programs.** `*-simple-program-<v>`.
- **Source trees.** `ghc-9.10.3-source`, `clang-src-21.1.8`, `cargo-auditable-0.7.2-vendor` and `clang-at-least-16-LLVMgold-path.patch`.
- **Environments.** `python3-3.13.12-env`.

**So the role has a real population, and most of what carries it today is something else.** On moat, 50 of 73 are not packages. Every group carries dependency edges: on moat, 39 version checks have 39 in-edges and 134 out-edges. Moving a member to a role the graph walks through would therefore change the graph.

## Clarifications

### Session 2026-11-13

- Q: Which C206 role do version-check (`*-get-version-*`) and test-program (`*-simple-program-*`) derivations carry? → A: A new fifth C206 value, `check`. It is a graph endpoint, so dependency edges are unchanged. It extends the frozen enum, so the catalog row, the parity extractors and C184's `member-roles` counts change with it.
- Q: Which role do environments (`python3-3.13.12-env`) carry? → A: `package`. A `buildEnv` is built, version-bearing and made of packages.
- Q: What makes a member a `check`? → A: Only the two observed groups: version checks (`*-get-version-*`) and test programs (`*-simple-program-*`). They are identified from derivation evidence where a derivation records any, and by these name shapes otherwise. A broader test detector is out of scope: it has no measured population.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer filtering `package` gets packages (Priority: P1)

A consumer reads C206 to find the built packages among a closure's members. Today, filtering on `package` returns mostly version checks, test programs and source trees. After this change it returns built software only.

**Why this priority**: The role's whole purpose is this filter, and today the filter's answer is mostly wrong. The adjacent CPE work (#1367, #1371) will want to key on `role == package`. A wrong population then becomes wrong CPEs.

**Independent Test**: On each closure golden, list the members carrying `package`, and check every one against the census's hand-read classification.

**Acceptance Scenarios**:

1. **Given** the moat closure, **When** it is scanned, **Then** `mes-libc-mini-0.27.1`, the `tinycc-*-libs` and `tinycc-*-chain` members, and `glibc-iconv-2.42` carry `package`.
2. **Given** the moat closure, **When** it is scanned, **Then** no `*-get-version-*`, `*-simple-program-*`, `-source` or `-src-<v>` member carries `package`.
3. **Given** the slacklinker closure, **When** it is scanned, **Then** `clang-at-least-16-LLVMgold-path.patch`, `cargo-auditable-0.7.2-vendor` and `clang-src-21.1.8` do not carry `package`.

---

### User Story 2 - A source tree is recorded as source (Priority: P1)

A member that is a tree of source code is labelled `source`, whether it was fetched or produced by unpacking or filtering a fetch. That covers a patch file, a vendored dependency tree, and `<name>-src-<v>` or `<name>-source`.

**Why this priority**: It is the issue's central example. `source` is already a graph endpoint, so the graph does not change.

**Independent Test**: On the slacklinker golden, every source tree the census lists carries `source`. The dependency edges are byte-identical to before.

**Acceptance Scenarios**:

1. **Given** `clang-src-21.1.8`, **When** it is classified, **Then** it is `source`.
2. **Given** a fetched tarball that already carries `source`, **When** it is classified, **Then** nothing about it changes, including its content-hash identity.
3. **Given** a source tree that was not fetched, **When** it is classified as `source`, **Then** its identity stays its recipe hash. A fetch's content-hash identity is not claimed for it.

---

### User Story 3 - Checks and test programs are not packages (Priority: P2)

Version-check derivations (`*-get-version-<v>`) and test programs (`*-simple-program-<v>`) are build verification, not software the build ships or consumes as a package. They carry a new role, `check`.

**Why this priority**: These are the largest misfit group (46 of 73 on moat). A consumer is unlikely to filter on them, but must stop getting them under `package`.

**Independent Test**: On the moat golden, all 46 carry `check`, and the dependency edges are identical to before.

**Acceptance Scenarios**:

1. **Given** `bash-get-version-5.3`, **When** it is classified, **Then** it carries `check`.
2. **Given** `gcc-simple-program-15.2.0`, **When** it is classified, **Then** it carries `check`.
3. **Given** a consumer that knows only the four earlier values, **When** it reads a `check` member, **Then** the documented catalog row tells it what the value means. The component itself is unchanged except for this annotation value.

### Edge Cases

- **A real package whose name also matches a misfit pattern.** For example, a package genuinely named `<x>-source`. The rule is checked against every census member on five closures. A collision is recorded rather than silently resolved.
- **A version token in the middle of the name.** `clang-at-least-16-LLVMgold-path.patch` is a patch whatever number it contains.
- **An environment** (`python3-3.13.12-env`). It is built from packages and is version-bearing, so it carries `package` (FR-002).
- **A darwin closure.** The goldens are Linux closures. The darwin bootstrap has no `mes`/`tinycc` chain, so the genuine-package population there may be near zero. That is a real state, not a failure.
- **A member a nameless rung already claims** (content hash, then `text`). It is unaffected: only the third rung changes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A closure member carries `package` only when it is a built, version-bearing package. On every census closure, no member of the census's version-check, test-program, source-tree, patch or vendor groups carries `package`.
- **FR-002**: The members the census reads as genuine packages keep `package` on every census closure: the `mes`/`tinycc` bootstrap chain, `glibc-iconv`, `libidn2`, `llvm-binutils`, `auditable-cargo`, the `rustc` wrappers, and environments (`-env`).
- **FR-003**: A member that is a tree of source code, a patch or a vendored dependency tree carries `source`. A member that was not fetched keeps its recipe-hash identity. Only fixed-output members have a content-hash identity, as before.
- **FR-004**: Version-check (`*-get-version-<v>`) and test-program (`*-simple-program-<v>`) derivations carry a new fifth C206 value, `check`. These two groups are its whole scope. A derivation that is some other kind of test keeps whatever role the other rules give it. `check` is a dependency-graph endpoint, as `source` and `package` are. The catalog row, the per-format parity extractors and C184's `member-roles` counts include it.
- **FR-005**: The classification uses derivation evidence where a derivation records any. A name-shape rule is admitted only for a group that has no such evidence. Each rule's membership is checked against the census on all five closure goldens, with the counts recorded. A rule that matches a member the census reads as genuine is rejected.
- **FR-006**: The dependency graph does not change. The members and edges in every closure golden's `dependencies[]` stay identical: every reclassified member moves between graph-endpoint roles only.
- **FR-007**: C184's `member-roles` counts change consistently with the per-component values in all three formats. The parity row for C206 stays `SymmetricEqual`.
- **FR-008**: The catalog row for C206 and the reference docs describe each role by what it is, and state the rule that assigns it. The rule is not to be stated as name shape where evidence is used.
- **FR-009**: Corpus goldens change only in the C206 values of the members this spec reclassifies, and in C184's counts. Non-closure targets are byte-identical.

### Key Entities

- **Closure member**: a derivation in the closure that declares no `pname`. It is classified once, carries one C206 role, and has one identity basis (content hash or recipe hash).
- **Member role (C206)**: the closed enum a consumer reads. Its values are `source`, `package`, `check`, `scaffolding` and `fragment`. `source`, `package` and `check` are dependency-graph endpoints, and the graph walks through the other two.
- **Census**: the per-closure grouping in `measurements/`, which every rule is checked against.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the moat and shellcheck goldens, `package` goes from 73 members to the census's 23 genuine packages. On slacklinker it goes from 93 to 34: the 29 genuine packages and the 5 environments.
- **SC-002**: Zero members from the census's misfit groups carry `package` on any of the five closure goldens.
- **SC-003**: Every member of the genuine-package group keeps `package`: 23 of 23 on moat.
- **SC-004**: The dependency edges in all five closure goldens are identical before and after.
- **SC-006**: On the moat and shellcheck goldens, `check` has 46 members. On slacklinker it has 47.
- **SC-005**: Non-closure corpus targets are byte-identical.

## Assumptions

- **"Source" widens to source trees that were not fetched.** That fits the role's name, and the role's catalog text is updated to say so. The identity basis keeps the difference (FR-003).
- **The census is the oracle.** Its grouping comes from reading names, and it is checked by hand against what each derivation is. The rules this spec admits are measured against it on all five closures, not just the three the issue named.
- **Out of scope.** Detecting tests in general (`passthru.tests` or similar): `check` covers only the two measured groups. Also out of scope: whether any of these members should be emitted at all (#1034's depth question, now settled by runtime scope), and CPEs for them (#1367, #1371).
