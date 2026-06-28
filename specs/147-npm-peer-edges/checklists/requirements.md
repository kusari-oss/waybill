# Specification Quality Checklist: npm peerDependencies — emit as edges + annotate peer-kind

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-28
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
  - Spec names specific file paths + line numbers (`package_lock.rs:177-181`, `:168-176`, `:680-711`, `resolve_dep_via_node_modules_walk`) as scope-bounding anchors. No control flow, function signatures, or Rust syntax in the spec body.
- [X] Focused on user value and business needs
  - US1 framed around security engineer + vulnerability scanner reachability. US2 framed around compliance tooling that needs to preserve the install-vs-functional distinction.
- [X] Written for non-technical stakeholders
  - Plain-language user journeys; concrete Trivy-vs-syft-vs-mikebom comparison table; explicit Given/When/Then scenarios.
- [X] All mandatory sections completed
  - Origin, User Scenarios & Testing, Requirements, Success Criteria, Assumptions, Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
  - Zero markers. All design decisions documented in Assumptions.
- [X] Requirements are testable and unambiguous
  - FR-001 to FR-009 each name a specific behavior with a verifiable assertion. Example: FR-004 specifies "MUST carry a `mikebom:peer-edge-targets` value in its `extra_annotations` map. The value MUST be a JSON array of PURL strings naming the peer-driven edge targets" — directly testable via map lookup + type-check.
- [X] Success criteria are measurable
  - SC-001 names a specific orphan count (5 → 0 on the audit lockfile). SC-002 names a specific parity-catalog row addition. SC-003/SC-004/SC-005 name specific unit-test assertions. SC-006 names the golden-refresh scope. SC-007 names the pre-PR gate. SC-008/SC-009 name specific jq queries + cross-tool comparison.
- [X] Success criteria are technology-agnostic (no implementation details)
  - SCs describe observable outcomes (orphan counts, jq query results, value-type assertions). The only "technology" references are the three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3) which are spec-bounded artifact formats.
- [X] All acceptance scenarios are defined
  - US1 has 5 scenarios; US2 has 5 scenarios. Each is Given/When/Then-shaped.
- [X] Edge cases are identified
  - 7 distinct edge cases covered: unmet peer, peer+regular dep precedence, optional peer, peer-also-root-dep, v1/v2 lockfile, cycle through peer-edges, yarn/pnpm out-of-scope.
- [X] Scope is clearly bounded
  - Out of Scope section lists 9 explicit exclusions (v1/v2 lockfiles, yarn, pnpm, bun, edge-level CDX annotation, SPDX 3 future `peer` scope, `--no-peer-edges` CLI flag, non-npm ecosystems, orphan-handling policy changes, new mikebom:* annotations beyond the named one).
- [X] Dependencies and assumptions identified
  - Assumptions section names 9 explicit assumptions including v3-dominance, lockfile-present semantic, CDX no-edge-metadata, omitted-when-empty annotation, edge precedence, yarn/pnpm out-of-scope, no new Cargo deps, operator-cadence verification, comment-text update mandatory.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
  - FR-001 ↔ US1 scenario 1, 5 + SC-001. FR-002 ↔ US1 scenario 4 + SC-005. FR-003 ↔ US2 scenario 4 + SC-004. FR-004 ↔ US2 scenarios 1, 2 + SC-003. FR-005 ↔ US2 scenario 3. FR-006 ↔ US2 scenario 5 + SC-002. FR-007 ↔ SC-003 (test replacement). FR-008 ↔ SC-006 (golden refresh). FR-009 ↔ explicit Constitution V audit in Out of Scope §10.
- [X] User scenarios cover primary flows
  - US1 covers edge emission across all 3 formats + orphan elimination + unmet-peer guard. US2 covers annotation presence, FR-003 precedence, FR-005 omission rule, cross-format byte-equality.
- [X] Feature meets measurable outcomes defined in Success Criteria
  - Every SC is achievable purely via the FR set.
- [X] No implementation details leak into specification
  - File paths and line numbers are scope anchors only. No function signatures or Rust syntax in spec body.

## Notes

- All checklist items pass on first iteration.
- The spec deliberately names `mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` because the change is purely reader-local — touching one file. A planner could mis-scope to the SPDX/CDX emitters without that anchor; the emitters consume `PackageDbEntry.extra_annotations` transparently and don't need code changes.
- The Constitution V audit (FR-009 + Out of Scope §5) deliberately enumerates the three formats' lack of native carriers for "peer-kind edge metadata" — preventing future contributors from second-guessing the `mikebom:peer-edge-targets` annotation as a Principle V violation.
- Ready for `/speckit-clarify` (probably zero questions; spec is self-contained) or directly for `/speckit-plan`.
