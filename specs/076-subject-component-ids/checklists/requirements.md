# Specification Quality Checklist: Subject identifier scheme + per-component user-defined identifiers

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-06
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

- Two-deliverable milestone: (1) document-level `subject:` identifier scheme + build-tier auto-detect, (2) per-component user-defined identifiers via `--component-id` flag. Bundled because both serve the same goal — completing the cross-tier content-addressable correlation chain that milestones 072–075 began.
- Constitution Principle V "native-first" audit: spec asserts both deliverables ride standards-native carriers (FR-004 for `subject:`, FR-008 for per-component identifiers). Specifically calls out that no new `mikebom:*` annotations are introduced. The `subject:` scheme uses the same per-document carrier set milestone 073 established. Per-component user-defined identifiers use CDX `components[].properties[]` or `externalReferences[]` (Phase 0 research will pin which), SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`, SPDX 3 `Element.externalIdentifier[]`.
- Bounded scope: per-component built-in schemes (e.g., per-component `subject:`) are explicitly out. Glob/wildcard component selectors are out (exact PURL match only). `bom-ref`-based selection is out. Hash algos beyond sha256/sha512 are out. These are deliberate MVP cuts; future milestones can revisit each.
- US3 (cross-tier handshake) is verified by an end-to-end harness rather than a built-in mikebom subcommand — keeps the milestone focused on emission contract correctness, not on building a new resolver/walker.
- All items pass on first iteration; spec is ready for `/speckit.clarify` or `/speckit.plan`. Recommend `/speckit.clarify` for one likely-worth-pinning decision: the CDX 1.6 carrier choice for per-document `subject:` identifiers (which existing `externalReferences[].type` enum value best fits the "binary subject hash" semantic — `provenance`, `attestation`, or a different existing value). Phase 0 research can absorb that decision if `/speckit.clarify` is skipped.
- **Post-`/speckit.clarify` integration (2026-05-06)**: applied one clarification — multi-digest subject behavior pinned to "sha256-only auto-emit; non-sha256 algos require manual `--subject-hash`." Updated FR-002 to make the rule explicit and added an edge-case bullet covering "Subject with no sha256 digest." See spec.md `## Clarifications` section. CDX type-enum-value choice for `subject:` was deliberately deferred to Phase 0 research §1 rather than asked as a clarify (decision is "reuse `attestation` per research §1").
- **Post-`/speckit.analyze` remediation pass (2026-05-06)**: applied finding-driven edits to address C1 (added `component_id_deterministic_across_reruns` test in T016 (j) covering SC-004), C2 (added `manual_subject_hash_flag_works_on_image_tier` test in T011 (e) covering image-tier `--subject-hash`), U1 (tightened T001 to enumerate four explicit deliverables — fixture audit, trace's subject-set field name, per-format component-emission sites, pre-existing per-component entries — that T010/T013/T014/T015 depend on), I1 (T002 now lists the known exhaustive-match call sites with line numbers). All findings resolved; no `[NEEDS CLARIFICATION]` markers remain.
