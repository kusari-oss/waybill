# Specification Quality Checklist: `unaddressable` counts patches and tarballs as software

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-07
**Feature**: [spec.md](../spec.md) | **Issue**: #1380

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

Two items warrant comment rather than a bare tick.

**"No implementation details"** is satisfied in the requirements and success
criteria, which name only observable document content. The Summary does quote
a source file and line — `scan_cmd.rs:6616` — because the defect *is* that two
code paths disagree about the same population, and naming where the correct
reasoning is already written down is the shortest honest statement of it. No
requirement depends on that reference.

**"Success criteria are technology-agnostic"**: SC-001 through SC-006 are
stated over the emitted document, which is the user-facing artifact, not over
internals. SC-002's arithmetic (`78 + 166 + 194 + 539 = 977`) is a property a
consumer can check with `jq`.

### A real risk this checklist cannot catch

Every figure here was measured on **aarch64-darwin**. The corpus pins
**x86_64-linux**, which resolves a different closure, and milestone 1366
already recorded one count differing between the two hosts (74 against 75).
The spec's numbers are correct for the host they name and must not be quoted
against the corpus goldens. A task in planning should restate the expectation
per host rather than carry one set of figures into both.

**Clarification has already produced one cross-host figure to anchor this.**
The committed linux `nix-closure-moat` golden carries **1,087** members
against darwin's 539 — the member exclusion is twice as large on the corpus
as this spec's headline suggests, and `unaddressable` there will fall much
further than 733 → 194. Planning must measure the linux figures rather than
scaling the darwin ones.
