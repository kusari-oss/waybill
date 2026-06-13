# Specification Quality Checklist: Automatic binary-name binding via produces-binaries annotation

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-13
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

- The "user" of this feature is the operator running `mikebom sbom scan --image <ref> --bind-to-source <source.cdx.json>` in a CI release pipeline. The shipping benefit is "one fewer flag to remember per image scan" for the operator's flagship component.
- The spec carries an unusual amount of language about ecosystem MANIFEST conventions (cargo's `[[bin]]`, npm's `bin`, pip's `[project.scripts]`, gem's `executables`, maven's `<finalName>`, Go's `package main`). These are NOT implementation details — they are user-visible facts of life for operators who maintain projects in those ecosystems. The spec uses them to bound the per-ecosystem extraction surface so the operator's mental model matches the tool's behavior.
- Constitution Principle V bullet 5 (standards-native fields take precedence over `mikebom:`-prefixed properties) is REFERENCED in FR-011 but the audit's CONCLUSION is deferred to the planning phase. The Assumptions section captures the working hypothesis (no native field covers "list of executable names"); the planning phase will confirm and cite the result.
- FR-002's case-insensitive + `.exe`/`.jar` suffix tolerance is unconditional — there's no flag to disable it. This is intentional: the only downside is over-matching at `weak` strength, which already conveys "evidence is partial."
- US2 bundles four ecosystems (npm + pip + gem + maven) into one P2 story because each ecosystem's extraction is roughly equivalent in complexity and they all follow the same shape: read the manifest, find the bin-declaration field, emit names. Splitting them into four P2/P3/P4/P5 stories would be over-decomposed for the spec phase; the planning phase may decide to split per-PR if the implementation diff is large.
- US3 (Go) is genuinely harder because the binary name lives in filesystem layout rather than a manifest field. P3 lets us ship US1 + US2 to production and learn from operator feedback before committing to the Go filesystem-walk shape.
- The Edge Cases section explicitly addresses two collision scenarios (same binary name across multiple source components, image-side suffix conventions) because these are foreseeable polyglot-monorepo gotchas, not theoretical concerns.
- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
