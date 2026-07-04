# Specification Quality Checklist: pnpm/yarn npm-alias syntax support (milestone 159)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `pnpm_lock.rs:129` (existing `parse_pnpm_key` helper) and `mikebom-cli/src/scan_fs/package_db/npm/` as code anchors for existing parsers; FRs describe emitted-artifact behavior (component-emission, edge-rewriting, annotation presence) not implementation shape.
- [X] Focused on user value and business needs — the SBOM consumer (Kusari Inspector, vulnerability scanner, compliance auditor) gets accurate alias resolution so CVE lookups on `@slorber/react-helmet-async` actually hit. The 6 + 1 + 3 = 10 dropped-edges-per-repo audit is the ROI narrative.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer scanning for CVE against the ACTUAL installed package gets a hit" not "rewrite `depends` in the ResolvedComponent." The Motivation section names concrete real-world alias examples.
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions + Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the alias grammar for both pnpm and yarn is unambiguous per empirical inspection of the test-podman-desktop + test-guac-visualizer + test-rails lockfiles. FR-006's ecosystem-specific annotation naming (`mikebom:pnpm-alias` vs `mikebom:yarn-alias`) is informed by the "different lockfile grammars" observation.
- [X] Requirements are testable and unambiguous — each FR names a concrete emitted-artifact behavior; each SC names a verification method (spot-check, byte-diff, integration test, empirical measurement).
- [X] Success criteria are measurable — SC-001 names exactly which 6 alias-edges test-podman-desktop MUST emit with specific canonical PURLs; SC-005 targets ≥708 BFS-reachable npm components (empirically-adjustable per milestone-156/157/158 pattern); SC-007 = ≥12 unit tests; SC-008 = new integration test; SC-011 = closes #493.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (specific PURL strings, jq-inspectable annotation presence, BFS reachability). CDX / SPDX 2.3 / SPDX 3 formats named as inputs to per-format annotation contract, not as implementation choice.
- [X] All acceptance scenarios are defined — US1 has 5 Given/When/Then scenarios covering both quoted-string and unquoted-string pnpm alias forms + yarn key-side alias + local-name-referring dep resolution + full audit-testbed sweep. US2 has 4 scenarios for annotation presence. US3 has 3 scenarios for byte-identity regression guard. Total: 12 acceptance scenarios.
- [X] Edge cases are identified — 7 edge cases enumerated: scoped-alias parsing, peer-dep suffix in alias value, local-name-referring dep, circular alias, alias to non-lockfile-top-level package, alias-across-ecosystem, self-reference alias.
- [X] Scope is clearly bounded — Out of Scope enumerates 7 explicit exclusions (issues #498/#494/#495/#496 named, non-npm alias forms in yarn, npm-shim aliases in package-lock.json, cross-ecosystem alias, provenance-based BFS gating, cross-repo consistency).
- [X] Dependencies and assumptions identified — 7 Assumptions covering aliased-name authority, pnpm `parse_pnpm_key` reuse, yarn `npm:` marker, milestone-090 no-alias verification, no new Cargo deps, empirical BFS metric adjustability, ecosystem-specific annotation naming.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 through FR-013 each map to a US1/US2/US3 acceptance scenario, an SC, or both. Zero orphaned FRs.
- [X] User scenarios cover primary flows — US1 (correct alias resolution → fully-connected graph) is the P1 fix; US2 (alias-provenance annotation) is the P2 transparency mechanism; US3 (byte-identity regression guard) is the P3 safety guarantee.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (test-podman-desktop 6 spot-checks), SC-002 (test-guac-visualizer + test-rails coverage), SC-003 (SC-002-style dual-side byte-identity), SC-004 (annotation universal presence), SC-005 (BFS reachability improvement), SC-006 (pre-PR gate), SC-007 (unit test floor ≥12), SC-008 (integration test), SC-009 (CHANGELOG), SC-010 (parity catalog), SC-011 (closes #493).
- [X] No implementation details leak into specification — the `pnpm_lock.rs:129` reference in FR-001 is a code anchor for the existing `parse_pnpm_key` helper being reused; describes WHERE the semantic lives, not HOW it's implemented.

## Notes

- All 16 checklist items pass on first authoring pass.
- The fix shape is well-scoped: two parsers to update (pnpm + yarn), two annotation types to emit, ~250 LOC of source-tree code plus 12+ unit tests + 1 integration test + 2 parity-catalog rows. Comparable to milestone 157's shipping size.
- The 3-repo empirical audit already surfaced 10 concrete alias-edges that will move from "dropped" to "correctly resolved" after this milestone ships. That's the load-bearing SC-001 + SC-002 evidence.
- The `mikebom:pnpm-alias` vs `mikebom:yarn-alias` split (rather than a single `mikebom:npm-alias`) is a deliberate assumption codified in the Assumptions section — the two lockfile grammars ARE different and downstream tooling may want to filter.
- SC-003 byte-identity guard is verified achievable pre-authoring by empirical grep against milestone-090 fixtures (no alias syntax present).
- The out-of-scope section explicitly names the four sibling issues from the earlier audit (#494, #495, #496, #498) so the reader knows this milestone does NOT close them.
- Ready for `/speckit-clarify` OR `/speckit-plan`. Two candidate clarifications identified but none blocking:
  - Q1 candidate: Should the alias-provenance annotation carry the FULL peer-dep suffix from the pnpm-value string (`"react-helmet-async(react-dom@18.3.1)"` for auditability), or just the local-name (`"react-helmet-async"`)? Recommended default per FR-007: just the local-name.
  - Q2 candidate: For yarn's key-side alias where the same key line has MULTIPLE alias-spec forms (`"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":`), do we emit the annotation ONCE (with the first spec's local-name) or ONCE-PER-SPEC-VARIANT? Recommended default: once per unique local-name.
- Both are LOW-impact question — could be resolved at /speckit-clarify or deferred to plan-time.
