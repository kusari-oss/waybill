# Feature Specification: Relabel a component tier in an edited SBOM (`sbom edit --retier`)

**Feature Branch**: `1651-edit-retier`
**Created**: 2026-11-24
**Status**: Draft
**Input**: Add a tier relabel to `waybill sbom edit` for testing consumers that act on `waybill:sbom-tier`. Refs #1683, kusaridev/iac#3552.

## Background (measured)

**Some consumers read `waybill:sbom-tier` and act on it.** One ingest pipeline (kusaridev/iac#3552) excludes every `design` component as if it were dev-only, then removes everything left unreachable. waybill's `design` means "declared, version not resolved". It is independent of scope, which is where dev-only is carried (CycloneDX `scope: excluded`).

On the committed corpus goldens (`main` `18691d8`), modelling that rule (drop `design` and `scope: excluded`, then everything reachable only through them):

| Target | `required` components lost today | Components lost today | `required` lost if `design` were relabelled `source` | Components lost then |
|---|---|---|---|---|
| npm-express | 27 of 29 | 43 of 45 | 0 | 16 (the dev-scoped ones) |
| hpack-ogmios-no-generated-cabal | 95 of 95 | 103 of 103 | 0 | 8 |
| haskell-language-server | 325 of 354 | 370 of 399 | 0 | 45 |

The model reproduces the rule as Pico's code states it (`collectCDXExcluded`, then `expandWithOrphans`). It is not Pico's code running.

**The consumer's rule is the defect,** and kusaridev/iac#3552 reports it. Until it is fixed, and to test any consumer's tier handling at all, an operator needs the same graph under a different tier without rescanning. #1683 proposes the other half, a scan-time switch that *removes* the label. This feature *relabels* it.

**waybill already has the right home for this.** `waybill sbom edit` (milestone 1071) derives an edited document from an emitted one. Its derivation record names what was done and carries the original's hash and signature status, and `waybill sbom verify-chain` checks it. An edited document therefore cannot pass for an unedited scan.

**Today's emitted tier vocabulary** (the corpus goldens): `design` 856, `source` 2,157, `build` 8,108, `analyzed` 544, `deployed` 142. The code can also emit `image` (the NuGet `deps.json` and PE/CLR readers) and `declared` (supplement merges), which are outside the CISA six and which waybill's lifecycle mapping does not map.

## Clarifications

### Session 2026-11-24 (decided in this spec, from the request's four questions)

- **Where it lives:** `sbom edit` only. A scan-time form is left to #1683's mechanism, if wanted later. The edit's derivation record is what keeps a relabelled document honest; a scan has no original to point to.
- **Accepted forms:** `--retier FROM=TO`, repeatable.
  - `FROM` is any tier value, matched exactly.
  - `TO` must be one of the six CISA types: `design`, `source`, `build`, `analyzed`, `deployed`, `runtime`. Relabelling *into* a value waybill's lifecycle mapping does not know would create the inconsistency the edit is meant to avoid.
  - The mappings apply simultaneously, not in sequence.
- **How the change is recorded:** the derivation record lists each mapping and how many components it changed. Each relabelled component also carries a new per-component marker naming its original tier. Without it, a component relabelled `design → source` is indistinguishable from one that was always `source`, and the derivation record gives only counts. No standards-native field expresses "this component's lifecycle classification was changed by an edit" (Principle V), so the marker is a new catalog row, shipped with its parity extractor.
- **Document-level values:**
  - **Recomputed:** the aggregated document type (CycloneDX `metadata.lifecycles`, and SPDX 3's `software_sbomType` with its "Observed lifecycle phases" comment), because they are derived from the per-component tiers and would otherwise contradict them.
  - **Left unchanged:** `waybill:graph-completeness` and its reason, including milestone 177's `transitive-edges-unresolvable` clause. A relabel resolves nothing: what was unresolvable in the original is still unresolvable. The same reasoning milestone 1071 applied to cross-tier binding records applies here, and the documentation says so.

### Session 2026-11-24 (`/speckit-clarify`)

- Q: Should each relabelled component carry a per-component marker naming its original tier? → A: Yes (Option A). The marker plus the derivation record's counts, as FR-004 states; a new catalog row with its parity extractor, protected from later removal.
- Q: What does a relabel do to the completeness verdict and its reason? → A: Nothing (Option A). Both stay unchanged, because a relabel resolves nothing. The documentation, and the derivation record's mapping, explain why a milestone-177 clause can name an ecosystem with no `design` components left (FR-007).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Exercise a consumer with the same graph under another tier (Priority: P1)

An operator has an emitted SBOM whose dependencies are `design`, and wants to see how a consumer treats the same graph when those components read as `source` or `build`. They run `waybill sbom edit --retier design=source` on each format, and hand the results to the consumer.

**Why this priority**: it is the request. It also gives a way to demonstrate kusaridev/iac#3552's effect today: the consumer keeps every runtime dependency once the tier no longer reads `design`.

**Independent Test**: edit a synthetic SBOM with `design` components. Every former `design` component reads `source`; nothing else about any component changes; the formats agree.

**Acceptance Scenarios**:

1. **Given** an SBOM with `design` components, **When** it is edited with `--retier design=source`, **Then** every one of them is `source`, and every other component's tier is unchanged.
2. **Given** the same SBOM, **When** compared component by component with the edited one, **Then** the components, PURLs, versions, scopes and edges are identical. The only differences are the tier, the per-component marker, the derivation record, the recomputed document-level type and any signature.
3. **Given** the CycloneDX, SPDX 2.3 and SPDX 3 documents of one scan, **When** each is edited with the same `--retier`, **Then** `waybill sbom parity-check` reports no gap across the three edited documents.
4. **Given** the three corpus targets in the Background, **When** each is edited with `--retier design=source`, **Then** the modelled consumer rule keeps every `required` component.

---

### User Story 2 - The relabel cannot be mistaken for a scan (Priority: P1)

A reader receives a relabelled SBOM. They can tell it was edited, which mappings were applied, how many components each changed, and, for any single component, what its tier was before.

**Why this priority**: a relabel makes the document say something the scan did not establish. Without the record, a test artefact could be mistaken for evidence.

**Acceptance Scenarios**:

1. **Given** a relabelled SBOM, **When** its derivation record is read, **Then** it names each `FROM=TO` mapping and its count, alongside the original's hash.
2. **Given** a relabelled component, **When** it is read in any of the three formats, **Then** it carries the marker naming its original tier.
3. **Given** a relabelled SBOM and its original, **When** `waybill sbom verify-chain` is run, **Then** the derivation link verifies, and signatures verify where they were applied.

---

### User Story 3 - The document stays self-consistent (Priority: P2)

A consumer that reads the document-level type, rather than component tiers, sees a type that agrees with the components after the relabel, and a completeness verdict that still describes what was resolved.

**Acceptance Scenarios**:

1. **Given** a document whose only `design` components are relabelled, **When** its document-level type is read, **Then** `design` is no longer listed, in both CycloneDX and SPDX 3.
2. **Given** a document whose completeness reason says `transitive-edges-unresolvable`, **When** it is relabelled, **Then** the verdict and the reason are unchanged.
3. **Given** a document whose document-level type was asserted by the operator at scan time (`--sbom-type`), **When** it is relabelled, **Then** the operator's assertion is kept and not recomputed.

---

### User Story 4 - Misuse is refused clearly (Priority: P2)

**Acceptance Scenarios**:

1. **Given** `--retier design=sources`, or a `TO` outside the six CISA types, **When** run, **Then** the edit refuses before writing anything, naming the six accepted values.
2. **Given** a value without `=`, an empty side, `FROM` equal to `TO`, or the same `FROM` given twice with different targets, **When** run, **Then** the edit refuses, naming the problem.
3. **Given** a mapping whose `FROM` matches no component, **When** run, **Then** the edit succeeds; the derivation record shows the mapping with a count of 0, and nothing else changes.

### Edge Cases

- **Simultaneous mappings.** `--retier design=source --retier source=build` turns original `design` into `source` and original `source` into `build`. A component is relabelled at most once.
- **A component with no tier** (the document's root, or a component from another generator) is never matched.
- **A document not produced by waybill** carries no `waybill:sbom-tier`, so the edit matches nothing. As with milestone 1071's annotation operations, the derivation record shows a count of 0.
- **Combined with other edit operations** (drop, redact, drop-annotations), the relabel applies to the components that remain. If `--drop-annotations waybill:` removes the tier annotations, there is nothing left to relabel: the counts say so, and the operation order is recorded.
- **Relabelling from a non-CISA value** (`image`, `declared`) is allowed. Relabelling into one is not.
- **An already-edited document** can be edited again. The new derivation record chains to the edited original as usual, and `verify-chain` follows the chain.
- **The per-component marker is protected** like the rest of the edit's provenance: a later `--drop-annotations` on its namespace does not remove it, or the history of a component's tier would be erasable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (the operation)**: `waybill sbom edit` MUST accept `--retier FROM=TO`, repeatable. Each component whose `waybill:sbom-tier` equals a given `FROM` takes `TO`. All mappings apply simultaneously, and a component is relabelled at most once.
- **FR-002 (validation)**: `TO` MUST be one of `design`, `source`, `build`, `analyzed`, `deployed`, `runtime`. Refused before any output is written, with an error naming the problem:
  - a malformed value;
  - an empty side;
  - `FROM` equal to `TO`;
  - one `FROM` mapped to two targets.
- **FR-003 (nothing else changes)**: Components, PURLs, versions, scopes, edges and every other annotation MUST be unchanged. The only changes allowed are:
  - the tier;
  - FR-004's marker;
  - FR-005's record;
  - FR-006's recomputed document type;
  - signatures.
- **FR-004 (per-component marker)**: Each relabelled component MUST carry a marker naming its original tier, identical across CycloneDX, SPDX 2.3 and SPDX 3. It is a new catalog row with its parity extractor in the same change, and joins the edit's protected set.
- **FR-005 (derivation record)**: The edit's derivation record MUST list each mapping and the number of components it relabelled, including 0. `waybill sbom verify-chain` MUST verify the result against its original.
- **FR-006 (document type)**: CycloneDX `metadata.lifecycles`, and SPDX 3's `software_sbomType` with its lifecycle-phases comment, MUST be recomputed from the relabelled tiers. Exception: when the original's document type was the operator's own assertion (`--sbom-type`), it is kept.
- **FR-007 (completeness untouched)**: `waybill:graph-completeness` and its reason MUST be unchanged. A relabel resolves nothing. The documentation says so, including that a milestone-177 `transitive-edges-unresolvable` clause may name an ecosystem whose `design` components were relabelled; it still describes the data, not the labels.
- **FR-008 (parity)**: Editing each of a scan's three documents with the same `--retier` MUST give documents that `waybill sbom parity-check` reports with no gap.
- **FR-009 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **A tier mapping**: `FROM=TO`, with the count of components it relabelled.
- **The relabel marker**: per component, the tier before the edit.
- **The document type**: the lifecycle phases aggregated from component tiers, or the operator's assertion.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (the motivating case)**: On npm-express, hpack-ogmios-no-generated-cabal and haskell-language-server, after `--retier design=source`, the modelled consumer rule keeps every `required` component: 29 of 29, 95 of 95 and 354 of 354, against 2, 0 and 29 today.
- **SC-002 (nothing else)**: For each of those targets, in each format, a component-by-component comparison finds no difference outside the tier, the marker, the derivation record, the document type and signatures.
- **SC-003 (honesty)**:
  - every relabelled component carries the marker in all three formats;
  - the derivation record's counts equal the number of components carrying the marker;
  - `waybill sbom verify-chain` passes, signed and unsigned.
- **SC-004 (parity)**: `waybill sbom parity-check` reports no gap on the three edited documents of each fixture and corpus target used.
- **SC-005 (refusals)**: Each malformed or unknown form in User Story 4 is refused with an error that names the problem, and writes no output.
- **SC-006 (mutations)**: Each rule is mutated, and each mutation is killed:
  - the mapping applied in sequence rather than simultaneously;
  - the marker omitted;
  - the derivation count wrong;
  - the document type not recomputed;
  - the completeness reason recomputed or dropped;
  - `TO` validation removed.

## Assumptions

- The edit acts on one document at a time, as milestone 1071 does. Format agreement comes from applying the same operation to each of a scan's documents.
- The per-component marker's name, row number and exact wire shape are decided in the plan, after the standards-native audit. The audit is expected to find no native field, as for `waybill:sbom-tier` itself.
- How the operator's `--sbom-type` assertion is recognised in an emitted document is established in the plan. If the document does not record it, the plan says so and FR-006's exception is amended.
- `image` and `declared` being outside the CISA vocabulary is a separate, pre-existing inconsistency. This feature neither emits nor fixes them.
- Pico's rule is modelled, not run. kusaridev/iac#3552 tracks the fix there.
