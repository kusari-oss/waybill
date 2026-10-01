# Specification Quality Checklist: nixpkgs security declarations as VEX

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-30
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

**Both markers resolved by the maintainer, 2026-09-30.**

- *Where do prose declarations go?* Per-component SBOM annotation (FR-010).
  They are composition facts — each one says there are components inside this
  one that the SBOM does not list — so the SBOM is their home, and nothing has
  to be fabricated to carry them. The rejected alternative needed a synthetic
  vulnerability identifier and would have produced invalid OpenVEX.
- *Declaration versus patch on one CVE?* The declaration wins in VEX
  (FR-012). The patch-derived `not_affected` is withheld, but the patch itself
  stays in pedigree, so what is suppressed is the suppression and not the
  evidence. A count at document scope keeps silence distinguishable from
  suppression.

**Every measured number in this spec is traceable** to
`measurements/README.md`, which carries the probes. Nothing here is derived
arithmetic presented as measurement — the repository rule that caught a
fourfold undercount in milestone 1035.

Two of the three questions the feature request raised were closed by informed
guess rather than marker, and both are recorded in Assumptions:

- *Does this need evaluation?* Yes. `meta.knownVulnerabilities` is a Nix
  attribute. A file-parsing route would silently miss computed cases while
  appearing to work on literal ones, which is a worse failure than not
  offering it.
- *Does a declaration warrant its own evidence grade?* Yes (FR-009). A
  maintainer asserting "this version is vulnerable" is stronger evidence than
  a filename containing a CVE identifier, and milestone 1035 built the grade
  as an enum with one variant precisely so a second could be distinguished.


## Analysis remediation, 2026-09-30

`/speckit.analyze` found one CRITICAL and three HIGH gaps. All six findings
are fixed; the spec and tasks below reflect that.

- **G1 (CRITICAL)** — no task touched the three emitter files, so four
  document-scope signals were computed into `NixpkgsSecuritySummary` and never
  emitted. Per-component annotations ride the `extra_annotations`
  pass-through; document-scope ones need explicit emission in
  `cyclonedx/metadata.rs`, `spdx/annotations.rs` and `spdx/v3_annotations.rs`,
  as milestone 1035 required. Four tasks added, ahead of the catalogue rows —
  otherwise those rows would have shipped extractors pointing at fields
  nothing writes.
- **G2, G3 (HIGH)** — FR-017 (no vulnerability arrays) and FR-018 (no external
  advisory queries) were negative requirements with no verification. One task
  each. FR-018's test exercises the path rather than reading the source,
  because inspection cannot prove a negative about code that has not run.
- **A1 (HIGH)** — the T016 gate, the one task whose failure invalidates the
  plan, said "materially different" with no threshold. Now a band: confirmed
  65–77%, path-mismatch ≥ 5%, haskell > top-level. The lower bound on
  mismatch matters as much as the coverage band — a mismatch near zero means
  the verification is rejecting nothing and may not be running.
- **U1 (MEDIUM)** — SC-002 has two halves and T021 asserted one. Now asserts
  the nixpkgs attribution positively, not only the contrast with
  patch-derived.
- **A2 (MEDIUM)** — SC-004 was non-discriminating: it described milestone
  1035's existing pedigree output, so it passed before this milestone began.
  Restated around what this feature actually adds, with the original recorded
  rather than replaced.

Task count 53 → 59. Renumbered by a single-pass map over file order rather
than sequential per-id replacement, which clobbers when a target id already
exists. All 25 cross-references verified to resolve, and verified to point at
the semantically intended task rather than merely at *a* task.
