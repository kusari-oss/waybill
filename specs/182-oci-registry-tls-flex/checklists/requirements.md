# Specification Quality Checklist: OCI registry TLS + transport flexibility

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-10
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references reqwest builder methods (`.add_root_certificate`, `.danger_accept_invalid_certs`) in Assumptions and Key Entities as planning-phase pointers, but the user-story text stays at the "operator wants" level; Constitution Alignment cites internal types as design signals, not prescriptions
- [X] Focused on user value and business needs — three P1 user stories anchor on distinct real-world blockers (Harbor devenv, private-CA prod, self-signed CI); every FR traces to a user-visible failure mode
- [X] Written for non-technical stakeholders — reader can follow "Harbor devenv exposes http://core:8080; today mikebom can't reach it; ship the flag" without opening code
- [X] All mandatory sections completed — User Scenarios (4 stories), Requirements (14 FRs), Success Criteria (8 SCs), Assumptions, Constitution Alignment all present

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — every design decision documented in Assumptions with peer-tool precedent (podman/docker/skopeo) as the tiebreaker; per-invocation-only scope (no config file) explicitly assumed to match mikebom's CLI posture
- [X] Requirements are testable and unambiguous — FR-001 through FR-014 all specify observable CLI/HTTP behavior; FR-004/FR-005 pin exact matching semantics; FR-014 lists the four error-message templates verbatim
- [X] Success criteria are measurable — SC-001/002/003 are single-command reproducibles; SC-004 is a byte-identity gate; SC-005 is a message-quality gate; SC-007 is the Harbor devenv acceptance
- [X] Success criteria are technology-agnostic — reference operator-visible behaviors (scan completes, WARN log emitted, byte-identical SBOM) rather than TLS-library or reqwest-builder internals
- [X] All acceptance scenarios are defined — US1 has 4 scenarios, US2 has 4, US3 has 3, US4 has 3; every scenario uses GIVEN/WHEN/THEN
- [X] Edge cases are identified — 8 cases: no-port matching, name-vs-endpoint scoping, PEM bundle, additive-CA-on-public-registry, skip-verify-plus-bearer-failure, coexistence with `--registry-credentials-dir`, byte-identical regression on public-registry scans, URL-scheme-vs-flag precedence
- [X] Scope is clearly bounded — Deferred to Future Milestones section explicitly lists mTLS, persistent config, per-image scoping, DER format as OUT of m182 scope; the three-gap fix is tightly scoped
- [X] Dependencies and assumptions identified — 8 assumptions covering flag naming (peer precedent), per-invocation scope, PEM-only, no auth changes, single reqwest integration point, webpki additive behavior, Harbor-team feedback loop, testing infra (wiremock + possibly rcgen)

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 ⇔ US1 + SC-001 + SC-007; FR-002 ⇔ US2 + SC-002; FR-003 ⇔ US3 + SC-003; FR-004/FR-005 ⇔ US1 acceptance scenarios 3+4; FR-006 ⇔ US2 acceptance scenarios 3+4; FR-007 ⇔ US3 acceptance scenario 3 + SC-003 WARN-log requirement; FR-008/FR-009/FR-010 ⇔ US4; FR-011 ⇔ Edge Cases coexistence; FR-012 ⇔ SC-004 byte-identity; FR-013 ⇔ Edge Cases URL-scheme; FR-014 ⇔ SC-005 error-message quality
- [X] User scenarios cover primary flows — three distinct failure-mode-to-fix flows (US1/US2/US3) + a composition regression pin (US4)
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-007 is the concrete Harbor devenv target; SC-001/002/003 are the three individual gap fixes; SC-004 is the byte-identity gate
- [X] No implementation details leak into specification — spec names reqwest builder methods in Assumptions + Key Entities as design pointers but frames them as planning-phase surfaces, not user-facing prescriptions

## Notes

- **All 16 checklist items PASS as of 2026-07-10**. Ready for `/speckit-implement`.
- /speckit-analyze findings applied (2026-07-10): C1 — added T017b for FR-013 URL-scheme regression pin; C2 — extended T027 with FR-009 plain-HTTP-wins-over-skip-verify assertion; C3 — extended T028 with FR-011 `--registry-credentials-dir` coexistence assertion; C4 — added `insecure_matcher_ignores_registry_endpoint_resolution` unit test to T004 for FR-005 name-vs-endpoint distinction. A1/A2/I1 (LOW findings) left as-is — legitimate implementation-time deferrals.
- Delivery cadence: 4 P1 user stories (US1 + US2 + US3 elevated for parity; US4 as P2 composition regression) fit a single-PR bundle. The three flags all thread through the SAME `RegistryClient::new` integration point — no cross-file coordination beyond the CLI plumbing.
- No clarifications needed. The peer-tool precedent (podman/docker/skopeo/crane) resolves every "how should this behave?" question. Any Harbor-team-specific quirk gets caught by the pre-release-binary validation loop before merge (per the user's stated "spec now, offer them a pre-release binary" strategy).
- The bot report's "no Bearer" claim was inaccurate (Bearer + Basic auth already work — see mikebom-cli/src/scan_fs/oci_pull/registry.rs:197-208 + fetch_bearer_token at line 267). m182 does NOT change auth. The scope is TLS + transport only.
- Testing infra: `wiremock` (dev-dep since m055) covers the mock-registry cases. `rcgen` (or shell-out to `openssl`) covers the throwaway CA generation. Both are planning-phase confirmations, not spec commitments.
