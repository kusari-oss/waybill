# Feature Specification: SPDX 3 native dependency completeness

**Feature Branch**: `1069-spdx3-relationship-completeness`
**Created**: 2026-10-04
**Status**: Draft
**Input**: Issue #878: "SPDX 3: use native Relationship.completeness instead of relying on waybill annotations".

## Context

SPDX 3.0.1 gives every relationship a `completeness` value: `complete`, `incomplete` or `noAssertion`. It says whether the relationship's targets are the whole set. waybill never sets it.

Whether a component's dependencies were fully resolved is expressed only in waybill's own annotations (`waybill:graph-completeness`, `waybill:orphan-reason`). A reader that knows the standard but not waybill's annotations cannot see it.

The other formats are already settled:
- **CycloneDX** gained the native equivalent in milestone 866: `compositions[]` with `complete` for components whose graph was resolved, and `unknown` for the rest.
- **SPDX 2.3** has no native construct, so its annotations are correct and final.

SPDX 3 is the one format with a native carrier that waybill leaves unused (Constitution Principle V).

**What makes this more than setting a field (measured, `measurements/relationship_shape.txt`)**:
- waybill writes **one SPDX 3 relationship per dependency edge**, each with a single target. In all 17 public-corpus goldens, every `dependsOn` relationship has exactly one target.
- Most components with dependencies have several such relationships (opentelemetry-go 30 of 30, ripgrep 25 of 39, haskell-language-server 285 of 327).
- None carries `completeness`.

`completeness` qualifies a relationship's own target list. On a one-target relationship:
- `complete` would claim the component has exactly that one dependency, which is false for most components.
- `incomplete` would be true of every component with more than one dependency, however well it was resolved.

So the native field can carry this signal only if a component's dependencies are expressed so that one relationship holds the set the qualifier describes.

**Decisions (clarified 2026-10-04)**:
- **Shape:** each component's dependencies of one kind become one relationship whose targets are the whole set.
- **Unknown leaves:** a component whose dependencies are unknown, and that has no outgoing relationship, gets a dependency relationship to `NoAssertionElement` marked `noAssertion`.
- **Claim strength:** `complete` is asserted on resolved components that have a relationship. Resolved components with no dependencies get no added relationship, since an absent qualifier claims nothing.

## Clarifications

### Session 2026-10-04

- Q: Relationship shape? → A: **Group** each component's dependencies of one kind into a single relationship whose targets are the whole set, so `completeness` describes that set. Every SPDX 3 golden with dependencies changes shape.
- Q: Components whose dependencies are unknown and that have no outgoing relationship? → A: Emit a dependency relationship to **`NoAssertionElement`** marked `noAssertion`.
- Q: Claim strength? → A: **`complete` on resolved components that have a relationship; nothing added for resolved leaves.** `incomplete` / `noAssertion` wherever CycloneDX says `unknown`.
- Q: Components CycloneDX makes no dependency claim about (their ecosystem is not one waybill enumerates completely)? → A: **No qualifier and no added relationship**, mirroring CycloneDX's silence.
- Q: The scan root, which CycloneDX lists under `aggregate: complete` when trace integrity is clean? → A: **Mirror it**: the root's grouped dependency relationship is `complete` exactly when CycloneDX emits that record, and unqualified otherwise.

## Out of Scope

- Relationships for resolved components with no dependencies (`NoneElement`). Not added: an absent qualifier claims nothing.
- Changes to CycloneDX or SPDX 2.3 output.
- Changes to the milestone-866 completeness predicate itself.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A standards-only SPDX 3 reader can tell which dependency lists are incomplete (Priority: P1)

A consumer reads waybill's SPDX 3 output with a tool that understands SPDX 3 but not waybill's annotations. They scan a Go project offline, where two components' parents could not be determined. Today the relationships carry no qualifier, so the graph reads as whole. After this feature, the relationships for components whose resolution did not complete say so natively.

**Why this priority**: it is the issue. The fact exists and is only invisible to standards-native readers.

**Independent Test**: scan spf13/cobra offline (the issue's case). Every component CycloneDX lists under `aggregate: unknown` is marked in SPDX 3 as not complete. No component CycloneDX lists under `aggregate: complete` is marked incomplete.

**Acceptance Scenarios**:

1. **Given** a component in an ecosystem whose transitive resolution did not complete, **When** SPDX 3 is emitted, **Then** its dependency relationships carry a completeness value other than `complete`.
2. **Given** the same scan emitted in CycloneDX and SPDX 3, **When** the two are compared, **Then** every component's completeness agrees between them: CycloneDX's `complete` / `unknown` aggregate, and SPDX 3's relationship completeness.

---

### User Story 2 - The waybill annotations stay, unchanged (Priority: P1)

`waybill:graph-completeness` and `waybill:orphan-reason` stay in every format. SPDX 2.3 has no native carrier and needs them, and cross-format annotation parity depends on all three formats carrying them.

**Independent Test**: the parity rows for those annotations pass unchanged, and their values are identical before and after for every corpus target.

**Acceptance Scenarios**:

1. **Given** any scan, **When** SPDX 3 is emitted, **Then** the document-scope and per-component completeness annotations are present with the same values as before.

---

### User Story 3 - Output changes only where the specification says it does (Priority: P1)

CycloneDX and SPDX 2.3 output is byte-identical for every scan. SPDX 3 output changes only in the relationships this feature defines. Every SPDX 3 document still passes the SPDX 3 conformance validator.

**Independent Test**:
- the CycloneDX and SPDX 2.3 goldens are byte-identical;
- every SPDX 3 golden change is reviewed and attributable to this feature;
- the conformance validator passes.

**Acceptance Scenarios**:

1. **Given** the public corpus, **When** goldens are regenerated, **Then** only SPDX 3 goldens change, and each change is in dependency relationships or added completeness values.

---

### Edge Cases

- **Mixed lifecycle scopes.** A component with runtime and development dependencies has relationships of different kinds (dependency, and lifecycle-scoped per scope). Each kind is grouped and qualified separately; the component's completeness applies to each.
- **A component in a complete ecosystem that the dependency walk did not reach.** It is treated as not complete, exactly as milestone 866 treats it for CycloneDX.
- **Workspace and main-module edges** that waybill synthesizes are ordinary dependency relationships and follow the same rule.
- **Root-override and split documents.** Completeness is computed over the components and relationships of the document being written.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A component's SPDX 3 dependencies of one kind MUST be emitted as one relationship whose targets are the whole set of that kind.
- **FR-001a**: That relationship MUST carry `completeness` from the same predicate milestone 866 applies to CycloneDX `compositions[]`, so the two formats agree by construction:
  - `complete` when CycloneDX lists the component under `aggregate: complete`;
  - `incomplete` when CycloneDX lists it under `aggregate: unknown`.
- **FR-001b**: A component CycloneDX lists under `aggregate: unknown` that has no outgoing dependency MUST get one dependency relationship to `NoAssertionElement`, marked `noAssertion`.
- **FR-001c**: A resolved component with no dependencies MUST NOT gain a relationship.
- **FR-001d**: A component CycloneDX makes no dependency claim about (its ecosystem is not among those enumerated completely) MUST get no `completeness` on its relationships and no added relationship.
- **FR-001e**: The scan root's grouped dependency relationship MUST be `complete` exactly when CycloneDX lists the root under `aggregate: complete` (trace integrity clean), and unqualified otherwise.
- **FR-002**: A completeness value MUST NOT claim more than waybill knows. A relationship MUST NOT be marked `complete` unless its targets are the component's whole dependency set of that kind.
- **FR-003**: For every component, the completeness SPDX 3 expresses MUST agree with the CycloneDX aggregate the same scan emits:
  - every CycloneDX `unknown` component is marked `incomplete` or `noAssertion`;
  - every CycloneDX `complete` component that has a dependency relationship is marked `complete`;
  - every component CycloneDX lists under neither aggregate carries no `completeness`;
  - no other combination occurs.
- **FR-004**: The waybill completeness annotations (`waybill:graph-completeness`, `waybill:graph-completeness-reason`, `waybill:orphan-reason`) MUST remain in all three formats, with unchanged values.
- **FR-005**: CycloneDX and SPDX 2.3 output MUST be byte-identical to the output before this feature.
- **FR-006**: Every SPDX 3 document waybill emits MUST pass the pinned SPDX 3 conformance validator.
- **FR-007**: The format mapping reference MUST document how SPDX 3 relationship completeness is derived, and that it is the native counterpart of CycloneDX `compositions[]`.

### Key Entities

- **Dependency set**: for one component and one dependency kind, the components it depends on.
- **Completeness predicate**: the milestone-866 decision of whether a component's dependency graph was resolved. Its inputs are the ecosystem's transitive-resolution outcome and whether the dependency walk reached the component.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On spf13/cobra scanned offline, the components CycloneDX lists under `aggregate: unknown` are exactly the components SPDX 3 marks `incomplete` or `noAssertion`: 100% agreement, both ways.
- **SC-002**: Across the public corpus, every component's SPDX 3 completeness agrees with its CycloneDX aggregate.
- **SC-003**: Zero SPDX 3 relationships claim `complete` for a component whose dependency set they do not fully contain.
- **SC-004**: CycloneDX and SPDX 2.3 goldens are byte-identical. Every SPDX 3 golden passes the conformance validator.
- **SC-005**: The completeness annotations' values are unchanged for every corpus target.

## Assumptions

- The milestone-866 predicate (ecosystem resolution outcome, plus reachability) is the source of truth. This feature changes how SPDX 3 carries it, not what it decides.
- An absent `completeness` claims nothing about the relationship. To be confirmed against the SPDX 3.0.1 model text in planning; if the model says otherwise, FR-002 governs.
- The pinned conformance validator (`spdx3-validate`, milestone 078) accepts `completeness` and `NoAssertionElement`. This is to be confirmed in planning by validating a sample document.
