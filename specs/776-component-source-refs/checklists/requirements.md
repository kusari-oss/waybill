# Specification Quality Checklist: Component Source-Provenance References

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — the spec names no file, function, crate, service, or field identifier. It says "the enrichment service", "repository-kind reference", "package identifier" rather than naming the vendor, the CDX field, or the PURL type. Requirements are stated against observable SBOM content.
- [X] Focused on user value and business needs — framed on the consumer question "where did this component come from, and where do I go to inspect or report against it?", which is what source-provenance references exist to answer.
- [X] Written for stakeholders with enough context to review scope
- [X] All mandatory sections completed (User Scenarios, Requirements, Success Criteria, Assumptions)

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous — each FR names an observable condition on emitted output (a reference of kind K with URL U present/absent, deduplicated, ordered, preserved)
- [X] Success criteria are measurable — SC-001 (1-in-109 → ≥80%), SC-002 (0-of-369 → ≥80%), SC-003 (no regression), SC-004 (offline majority), SC-006 (±3% wall time), plus binary criteria SC-005/007/008/009/010
- [X] Success criteria are technology-agnostic where the domain permits — coverage proportions, byte-identity, wall time, dependency count. SC-008 references the project's own verification gate, matching m771/m774/m775 precedent for this repository.
- [X] All acceptance scenarios are defined — six for US1 (supplied link, multiple kinds, unrecognized label, no links, duplicate suppression, enrichment disabled), five for US2 (derivable, non-derivable, missing version, additive to existing, previously-uncovered ecosystem)
- [X] Edge cases are identified — malformed URL, duplicate links, same URL under two labels, empty metadata, components without package coordinates, operator-supplied references, identifiers needing encoding, upstream registry scheme drift
- [X] Scope is clearly bounded — FR-007 forbids new network requests; FR-014 forbids new operator surface; FR-015 forbids new dependencies; Assumptions explicitly exclude ecosystems whose distribution URL needs registry metadata
- [X] Dependencies and assumptions identified — enrichment-enabled default, service link vocabulary stability, best-effort nature of the service, kind-over-count principle, per-ecosystem derivability, measurement-set limits, and the scoring tool's non-authoritative status

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001..008 map to US1 scenarios 1–6; FR-009..011 to US2 scenarios 1–5; FR-012/013/014/015/016 are verifiable by inspection of emitted output, operator surface, and the dependency manifest
- [X] User scenarios cover primary flows — enrichment-enabled (the default path), enrichment-disabled (the offline path), and the degradation paths in between
- [X] Feature meets measurable outcomes defined in Success Criteria — every SC is verifiable with the existing test suite plus the five-fixture measurement set already established
- [X] No implementation details leak into specification — Key Entities describe roles and data shape, not types or call sites

## Notes

- **The strongest evidence here is that no new data fetching is required.** The enrichment payload consumed by US1 is already retrieved and parsed on every enrichment-enabled scan and then discarded — a condition acknowledged in a source comment. FR-007 pins that: satisfying US1 must not add a single network request. This is what makes the milestone cheap relative to its measured effect.
- **Two independent causes, deliberately split into two stories.** US1 (discarded enrichment data) and US2 (narrow offline derivation) have different mechanisms, different ecosystem coverage, and different network posture. They are separable; either can ship without the other.
- **Reference kind matters more than reference count**, and the spec says so in Assumptions. The rust-ripgrep observation — 61 references but near-zero source-provenance coverage, because they are all registry landing pages — is the case that makes this concrete, and is why FR-011 keeps the existing references while adding correct ones rather than swapping them.
- **The scoring tool is treated as an instrument, not a specification.** An Assumptions entry states that where the tool's conventions and the format specifications disagree, the specifications govern. This is deliberate: the immediately preceding investigation rejected a different proposed change precisely because it would have encoded a scoring tool's private convention over a documented format-semantics decision. That failure mode is now written down.
- SC-010 and the final Assumption anticipate expected fixture-output churn: adding references changes stored expected outputs, and the diff must consist solely of added references. Anything else is a defect.
