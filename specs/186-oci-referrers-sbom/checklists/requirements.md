# Specification Quality Checklist: OCI Referrers API SBOM discovery

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references `oci-spec` crate + reqwest + Distribution Spec v1.1 endpoint shape as design signals, but user-story text stays at the operator level (fetch upstream SBOM if available; fall through to scan otherwise; strict-mode for compliance workflows). Constitution Alignment cites internal enums (`SbomSourceMode`) as design signals, not prescriptions.
- [X] Focused on user value and business needs — three P1 user stories pin distinct workflows (either mode for CPU/network savings; strict referrer mode for compliance; scan mode for backward compat). Every FR traces to a user-visible behavior.
- [X] Written for non-technical stakeholders — reader can follow "fetch a pre-existing SBOM from the registry if the upstream published one; otherwise scan the image bytes" without opening OCI Distribution Spec.
- [X] All mandatory sections completed — User Scenarios (3 stories + 10 edge cases), Requirements (16 FRs), Success Criteria (9 SCs), Assumptions, Constitution Alignment, Deferred to Future Milestones all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — every design decision documented in Assumptions (opaque-bytes emission, default `scan` for backward compat, three-media-type initial set, 100 MiB size cap, m182 TLS flag inheritance, no new deps).
- [X] Requirements are testable and unambiguous — FR-001 through FR-016 all specify observable CLI/HTTP/emission behavior; FR-002 pins the exact endpoint URL; FR-003 pins the exact media type set; FR-004 pins the priority order for multiple referrers; FR-006 pins byte-identity emission; FR-007 pins the two provenance-marker keys.
- [X] Success criteria are measurable — SC-001 has a timing comparison target (referrer emit <2s vs scan ≥5s for typical 50 MB image); SC-002 pins the 10% overhead ceiling for `either`-mode fall-through; SC-003 pins the 5-second error-exit gate; SC-004 pins byte-identity on the default path; SC-005 pins log-content requirements; SC-006 pins size-cap integration testability; SC-007 pins m182 flag inheritance; SC-008 is the `cargo tree` line-count invariant; SC-009 pins CLI help text requirements.
- [X] Success criteria are technology-agnostic — reference operator-visible behaviors (emission time, log content, exit codes) rather than internal implementation.
- [X] All acceptance scenarios are defined — US1 has 4 scenarios (either happy path + provenance markers + no-referrer fall-through + 404 silent-fallback); US2 has 3 (referrer-mode happy path + no-referrer error + no-v1.1-support error); US3 has 2 (scan-mode preserves pre-m186 + default is scan); every scenario uses GIVEN/WHEN/THEN.
- [X] Edge cases are identified — 10 cases: multiple-referrers, format-mismatch, multi-arch platform, signed-attestation-envelope, referrer download failure, auth-failure semantics, rate-limit handling, size-cap violation, wrong-input-type flag rejection, tag-vs-digest resolution.
- [X] Scope is clearly bounded — Deferred section explicitly lists signed-verification, format transcoding, additional media types, multi-referrer emission, artifactType filtering as OUT of m186 scope.
- [X] Dependencies and assumptions identified — 8 assumptions covering v1.1 endpoint availability, canonical media types, opaque-bytes emission, no-verification-yet, no-transcoding-yet, conservative default, size-cap sizing, m182 TLS inheritance, zero-new-deps.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 ⇔ US1/US2/US3 flag; FR-002/003/004/005 ⇔ US1 acceptance 1; FR-006 ⇔ SC-004 byte-identity; FR-007 ⇔ US1 acceptance 2 + SC-005 log content; FR-008 ⇔ US1 acceptance 3+4 + SC-002 fall-through timing; FR-009 ⇔ US2 acceptance 2+3 + SC-003 error timing; FR-010 ⇔ US3 acceptance 1+2 + SC-004 byte-identity; FR-011 ⇔ Edge Cases wrong-input-type; FR-012/013 ⇔ SC-007 m182 TLS inheritance; FR-014 ⇔ SC-006 size-cap testability; FR-015 ⇔ SC-004 regression pin; FR-016 ⇔ SC-008 zero-new-dep.
- [X] User scenarios cover primary flows — three distinct workflows: `either` for cost savings, `referrer` for compliance, `scan` for backward compat. Combined they cover 100% of the flag's semantic space.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 is the concrete cost-savings gate; SC-002/003 are the fall-through and strict-mode timing gates; SC-004 is the pre-m186 regression guard; SC-005 is the audit-log-content gate; SC-006/007 are integration-testable end-to-end.
- [X] No implementation details leak into specification — spec names internal types (`SbomSourceMode`, `oci-spec::image::ImageIndex`) and file-path anchors as design signals but frames them as planning-phase surfaces, not user-facing prescriptions.

## Notes

- **All 16 checklist items PASS as of 2026-07-11**. Ready for `/speckit-plan`.
- Delivery cadence: 3 P1 user stories in one PR. Zero new Cargo deps + reusable infra (m034 credentials, m036 cache, m182 TLS, oci-spec crate) keep the scope tight. Estimated ~25-30 tasks across 6 phases.
- **`--sbom-source scan` is the default** for backward compatibility — every pre-m186 invocation continues to work unchanged. FR-015 + SC-004 pin this.
- **Signed-verification + transcoding + additional media types** all deferred. m186 is the "discovery + fetch" MVP; each deferred feature has its own natural follow-up milestone.
- **m182 TLS flags** (`--insecure-registry`, `--registry-ca-cert`, `--insecure-tls-skip-verify`) automatically apply because the Referrers endpoint reuses the same reqwest client — no per-endpoint plumbing. FR-013 + SC-007 pin this.
- **Size cap** (100 MiB, env-overridable via `MIKEBOM_REFERRER_MAX_BYTES`) matches the m036 layer-cache pattern.
- No clarifications needed. The OCI Distribution Spec v1.1 defines the endpoint shape; the media type set is industry-standard; the three-mode CLI is a standard fetch-vs-scan pattern (see cosign's own similar flags).
