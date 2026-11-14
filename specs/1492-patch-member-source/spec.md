# Feature Specification: Locally built patches are sources, not fragments

**Feature Branch**: `1492-patch-member-source`
**Created**: 2026-11-13
**Status**: Draft
**Input**: User description: "#1502 — Locally built patches with no version token (replaceVars outputs: dontUnpack=1, src a local .patch, listed in another derivation's patches attribute) are classified C206 fragment ("a live-bootstrap C function"). Classify them as source, using the patches-attribute evidence spec 1380 added. Four members on the nix-closure-moat golden are affected. The projection walks through fragment and stops at source, so this changes dependency edges: measure the delta on the golden with the s1367 edge accounting (specs/1280-nix-closure-edges/measurements/edge-accounting-2026-11-13.md; the captured closure is reproducible from public-corpus run 43535531153) and review it. Keep fragment for real live-bootstrap functions. Evidence: specs/1380-closure-package-role/measurements/member-evidence-probe-2026-11-13.txt."

## Background

C206 `waybill:closure-member-role` says what a closure member that declares no `pname` **is**. Each member gets one of five values:
- `source`: something a package is built from;
- `package`;
- `check`;
- `scaffolding`: a build script;
- `fragment`: documented as "a live-bootstrap C function", such as `strncpy`.

The role decides two things. A consumer filters a dependency list to `source` and `package` to get the real dependencies. The dependency-graph projection (milestone 1280, FR-013 of milestone 1282) **stops** at `source` and `package` members and **walks through** `scaffolding` and `fragment` ones.

Spec 1380 (#1374) taught the classifier that a member whose output another derivation lists in its `patches` attribute is a patch, and therefore `source`. It applied that evidence only to members whose name carries a version token. It stayed inside the existing `package` population on purpose, so that it moved no dependency edge.

A patch built locally (`replaceVars` on a `.patch` file in the nixpkgs tree, with `dontUnpack`) has no version token. Its name is the file's name. It therefore falls through every rung to `fragment`, and the document calls it a C function.

**Measured** on the pinned x86_64-linux `nix-closure-moat` closure (1,749 derivations; captured in public-corpus run 43535531153; closure sha256 `898fcdd3…44ab`), by asking the reader's own classifier:
- **Population:** of the 245 `fragment` members, exactly **5** are listed in another derivation's `patches`. They are 4 distinct names: `001-fix-rpath.patch`, `catalog-legacy-uris.patch` (two derivations), `fix-build-time-run-tcl.patch` and `remove-hatch-plugins.patch`.
- **Shared shape:** all 5 have `dontUnpack` set and a local `.patch` as `src`.
- **No other role:** no `scaffolding`, `package` or `check` member is listed in any `patches`, so the change can reach no other role on this closure.
- **Already right:** the other 56 patch-named members are already `source`.

## Clarifications

### Session 2026-11-13

- Q: Where does the new `patches` check sit in the classifier's fixed evidence order? → A: Right after the content hash, before `env.text`. A member applied as a patch is `source` however it was built.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A locally built patch is reported as a source (Priority: P1)

A reviewer reads a Nix closure SBOM and filters closure members to the ones that are dependencies (`source` and `package`). A patch that nixpkgs applies to a package it builds is part of what that package was built from. Today it is missing from the filtered list and labelled as a C function. After this change it appears as a `source`, beside the fetched patches that were already `source`.

**Why this priority**: A wrong role is a false statement in the document, and the role is the filter consumers are told to use. A patch hidden among C functions is invisible exactly where a reviewer looks for what was applied to a package.

**Independent Test**: Scan a closure with one locally built patch listed in a consumer's `patches` and no version token in its name. The member carries `source`, and a member with the same shape that no `patches` lists still carries `fragment`.

**Acceptance Scenarios**:

1. **Given** a member with no `pname`, no content hash and no version token, whose output another derivation lists in `patches`, **When** the closure is scanned, **Then** its C206 value is `source` in all three formats.
2. **Given** a member with the same shape that no derivation lists in `patches`, **When** the closure is scanned, **Then** its C206 value stays `fragment`.
3. **Given** the pinned `nix-closure-moat` closure, **When** it is scanned, **Then** exactly the 5 measured derivations change from `fragment` to `source`, and no other member's C206 value changes.

---

### User Story 2 - The graph names the patch a package was built with (Priority: P1)

A reviewer follows a package's dependency edges to see what it was built from. Today the projection walks through a locally built patch to whatever built the patch file, so the patch never appears as an edge target. After this change the walk stops at the patch, as it already does for fetched patches. Edges from the patch to what built it are then the patch's own.

**Why this priority**: The reclassification and the edge change are one change; the projection reads the role. Shipping the role without measuring and reviewing the edge change is what #1502 was split out from #1374 to avoid.

**Independent Test**: On the control closure from Story 1, the consuming package gains an edge to the patch, and the edges that previously passed through the patch now start at it.

**Acceptance Scenarios**:

1. **Given** a package that lists a locally built patch in `patches`, **When** the closure is scanned, **Then** the package has a dependency edge to the patch.
2. **Given** the pinned `nix-closure-moat` closure, **When** the edge accounting from #1367 is run before and after the change, **Then** every edge added or removed is accounted for by the 5 reclassified members. No edge changes for any other reason.

---

### User Story 3 - Fragment keeps meaning a live-bootstrap function (Priority: P2)

A reviewer who filters *for* `fragment` to audit the live-bootstrap chain sees only bootstrap functions and objects, not patches.

**Why this priority**: The catalog definition of `fragment` becomes true of every member that carries it. That has value, but it follows from Story 1 rather than adding behaviour.

**Independent Test**: On the pinned closure, no member whose output any `patches` attribute lists carries `fragment`.

**Acceptance Scenarios**:

1. **Given** the pinned closure after the change, **When** members carrying `fragment` are listed, **Then** none is listed in any derivation's `patches`, and the count is 240 (245 minus the 5).

---

### Edge Cases

- **Patch listed by a derivation outside the closure.** Only `patches` attributes of derivations in the queried closure count, as in spec 1380. A member listed only by a derivation outside it stays as classified today.
- **The same patch derivation listed by several consumers.** It is one member with one role. Each consumer gains its own edge to it.
- **Two derivations of one patch file.** `catalog-legacy-uris.patch` is built twice, with different recipes and the same `src`. Each is its own member with its own identity, as today, and both become `source`.
- **A patch member with a content hash.** It is already `source` through the first rung and is unaffected.
- **A member with `env.text` listed in `patches`** (a generated patch, such as one from `writeText`). It is `source`: the `patches` check precedes `env.text` (FR-010). None exists on the pinned closure.
- **A member listed in `patches` that also has a version token.** It is already `source` under spec 1380 and is unaffected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The classifier MUST give C206 `source` to any member that declares no `pname`, has no content hash, and is listed in the `patches` attribute of a derivation in the same closure, whatever its name.
- **FR-002**: A member not listed in any `patches` attribute MUST be classified exactly as today. In particular, a member with no evidence still gets `fragment`.
- **FR-003**: The classifier MUST stay total: every member still gets exactly one of the five frozen C206 values. No new value is added.
- **FR-004**: The dependency-graph projection MUST treat a reclassified member as a stopping point, like any `source` member. No projection rule changes; the change in edges follows from the role alone.
- **FR-005**: The edge change on the pinned `nix-closure-moat` closure MUST be measured with the #1367 edge accounting, before and after, on the captured closure (sha256 `898fcdd3…44ab`). Every added or removed edge MUST be attributed to a reclassified member. The measurement and its review MUST be committed under this feature's `measurements/`.
- **FR-006**: The three formats MUST agree on the reclassified members' C206 value and on the changed edges. The C206 and C184 parity rows stay as they are.
- **FR-007**: The C184 `waybill:nix-closure` counts MUST describe the new document: `member-roles` (fragment −5, source +5 on the pinned closure), `edge-ineligible-components` and `edges`.
- **FR-008**: The `nix-closure-moat` corpus goldens MUST be regenerated, and the diff reviewed component by component and edge by edge against the FR-005 measurement. Goldens of other closure targets that change MUST be reviewed the same way. A change no reclassified member explains is a defect, not churn.
- **FR-009**: The C206 catalog row MUST say that the `patches`-attribute evidence applies whatever the name. Its description of `fragment` MUST stay "a live-bootstrap C function", now true of every member that carries it.
- **FR-010**: The fixed evidence order MUST become: content hash → `source`; listed in a `patches` attribute → `source`; `env.text` → `scaffolding`; version-shaped name → `package`, `source` or `check` as spec 1380 decides; else `fragment`. The catalog row's statement of the order MUST be updated to match.

### Key Entities

- **Closure member**: a derivation in the closure that declares no `pname`, emitted as a component since milestone 1282. It has one C206 role and an identity.
- **Patch listing**: the closure's own record, in a derivation's `patches` attribute, that a store path is applied as a patch to that derivation's build. It is the evidence this feature uses, and it was added by spec 1380.
- **Projected edge**: a dependency edge between two edge-endpoint members, derived from `inputs.drvs` by walking through non-endpoint members.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the pinned `nix-closure-moat` closure, exactly 5 members change C206 value, all from `fragment` to `source`, and they are the 5 listed above.
- **SC-002**: After the change, 0 members carrying `fragment` on the pinned closure are listed in any `patches` attribute. Before it, 5 are.
- **SC-003**: Every edge that differs between the before and after documents of the pinned closure has a reclassified member as an endpoint, or replaces a walk through one. 0 differing edges are unexplained.
- **SC-004**: Every other closure-target golden is either byte-identical, or changes only on members listed in some `patches` attribute and their edges.
- **SC-005**: The control from Story 1 fails if the new evidence is removed: with the rule disabled, the listed patch is `fragment` again.

## Assumptions

- **Rung order (decided, FR-010).** The `patches` listing is checked before `env.text`, because the closure's record that a path is applied as a patch is stronger evidence of what a member is than how it was built. On the pinned closure this places nothing differently from the narrower fallback-only placement, because no `scaffolding` member is listed in any `patches` attribute (measured). The two differ only for a generated patch, which this order calls `source`.
- **Version token.** No version token is needed. Spec 1380 put the evidence inside the version-name rung only to keep its change edge-neutral. That reason no longer applies, because this feature is meant to change edges and measures the change.
- **Scope of the measurement.** Only x86_64-linux is measured, because that is what the goldens pin. The aarch64-darwin closure is not captured, because evaluating a corpus flake locally is out of bounds.
- **No new annotation.** No new catalog row or annotation is introduced. The change is to the value of an existing one (C206) and to counts in another (C184).
- **Stacking.** The FR-005 measurement uses the edge accounting added in #1509 (#1367). This feature's branch is stacked on that one.
