# Specification Quality Checklist: Homebrew (brew + Linuxbrew) reader

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-22
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec discusses PURL shape, prefix detection, receipt parsing semantics; no Rust/crate/struct-layout references except in the `Dependencies and Constraints` section where the template permits architectural-dep references
- [X] Focused on user value and business needs — every user story names the operator outcome (Apple Silicon dev-machine inventory, Intel + Linuxbrew parity, GUI app cataloging)
- [X] Written for non-technical stakeholders — terms like `INSTALL_RECEIPT.json`, "tap", "cask" are explained inline where they appear
- [X] All mandatory sections completed — User Scenarios, Requirements, Success Criteria all present and substantive

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — informed defaults documented in Assumptions for: standard-prefix-only scope, INSTALL_RECEIPT.json as authoritative source, no live brew invocation, casks as shallower than formulae, file-claim deferral, custom HOMEBREW_PREFIX deferral
- [X] Requirements are testable and unambiguous — every FR-001..FR-011 has explicit MUST/MUST NOT semantics and a corresponding SC- gate or acceptance scenario
- [X] Success criteria are measurable — SC-001..SC-007 are all concretely verifiable (component count match, prefix-independence, type=cask qualifier, byte-identity preservation, exit-code-0 on corrupted-input fixture, jq-able PURL filter, tap qualifier presence/absence)
- [X] Success criteria are technology-agnostic — SC-006 explicitly avoids tool-specific consumer code by requiring the standard PURL filter to work
- [X] All acceptance scenarios are defined — US1×3, US2×4, US3×2
- [X] Edge cases are identified — 9 entries (multi-version, pinned, keg-only, third-party taps, slashed tap names, custom HOMEBREW_PREFIX, malformed JSON, missing receipt, empty Cellar, no Homebrew at all)
- [X] Scope is clearly bounded — explicit Out-of-Scope section listing 8 deferred concerns (live brew, env-var prefix override, pre-2014 installs, file-claim, pinned-marker, cask dep extraction, Brewfile, tap source-URL emission, alpm-style subcommand)
- [X] Dependencies and assumptions identified — both sections present and concrete

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 → US1.3 + SC-004; FR-002/FR-003/FR-004 → US1.1/2 + SC-001; FR-005 → US3 + SC-003; FR-006 → SC-004; FR-007 → SC-005; FR-008 → Edge Cases; FR-009 → US2.2; FR-010 → Assumption; FR-011 → Edge case
- [X] User scenarios cover primary flows — P1 Apple Silicon formulae, P2 Intel + Linuxbrew, P3 macOS Casks; each independently testable per the spec template's MVP-slice discipline
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001..SC-007 each tie back to a user story and a functional requirement
- [X] No implementation details leak into specification — references to `mikebom-cli/src/scan_fs/package_db/dpkg.rs` and milestone numbers in Dependencies / Assumptions refer to mikebom architectural milestones (template explicitly permits this in those sections); no language/crate/struct-layout details appear

## Notes

- The `pkg:brew/` PURL type is intentionally chosen as informal (purl-spec doesn't define a brew type yet). This is called out explicitly in Assumptions and tracked as a follow-up extension to purl-spec. Mirrors the situation milestone 128 hit with Yocto (`pkg:yocto/` also informal pending purl-spec extension).
- File-claim tracker integration (the alpm-reader US3 equivalent) is deliberately deferred to a follow-up. Homebrew's symlink-heavy bottling means a naive file-list walk would miss the canonical install path; doing it right requires symlink resolution beyond the alpm reader's flat-list approach. Acknowledged as a known soft regression in the Out-of-Scope section.
- US2 (Intel macOS + Linuxbrew) is a refinement test surface, not new code — the prefix-detection ladder in FR-001 handles all three locations uniformly. US2 validates that the cross-prefix behavior holds end-to-end; the implementation is the same code path.
- US3 (Casks) is a meaningfully different code path from US1 (formulae) — separate metadata format, separate directory structure (Caskroom vs Cellar), different PURL qualifier. Worth its own slice for incremental shipping.
