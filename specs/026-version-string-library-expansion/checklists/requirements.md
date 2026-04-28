# Spec Quality Checklist: Curated Version-String Library Expansion

**Checklist for** `/specs/026-version-string-library-expansion/spec.md`

## Coverage

- [X] Background section explains why expanding the scanner is the
      goal + cites file:line evidence (`version_strings.rs:53` for
      the `scan` entry point; line 21 for `EmbeddedVersionMatch`;
      line 80 for the `at_boundary` guard; lines 161-296 for the
      5 existing parsers).
- [X] User story has a P-priority (P1 — coverage breadth tied to
      CVE matchability) and a "why this priority" justification.
- [X] Independent Test is concrete (specific test commands +
      observable matches).
- [X] Acceptance scenarios use Given/When/Then framing (7 scenarios
      covering all 4 libraries + dedup + negative).
- [X] Edge Cases section names the corner cases (boundary contract,
      OpenJDK two schemes, optional `+build` suffix, LLVM strict-
      prefix gate, 4-segment versions on GnuTLS/LibreSSL, case
      sensitivity, format applicability).
- [X] Functional Requirements numbered (FR-001 through FR-009).
- [X] Key Entities — 4 new `CuratedLibrary` enum variants specified
      inline in FR-001.
- [X] Success Criteria measurable (SC-001 through SC-008), each with
      a verification mechanism.
- [X] Clarifications section captures the 4 scope decisions (strict
      semver only on GnuTLS/LibreSSL/LLVM; two-scheme parser for
      OpenJDK; `LLVM version ` strict prefix; output-shape
      unchanged; not a bag consumer).
- [X] Out of Scope explicitly names the deferred 3 libraries
      (glibc / musl / V8) with the technical blocker for each,
      and labels them as research-and-attempt 026.x follow-on.

## Tighter spec set rationale (4 files vs 8)

- [X] No `research.md` — recon answered every architectural
      question (7th use of the 4-file template after 021, 022,
      023, 024, 025, 028). Pattern fully validated.
- [X] No `data-model.md` — only enum-variant additions.
- [X] No `contracts/` — public API unchanged beyond the enum gaining
      variants (which is `pub`).
- [X] No `quickstart.md` — 4 short files self-explanatory.

This is the **7th use** of the 4-file template. The pattern is now
fully validated for contained scanner-extension milestones.

## Independence

- [X] Single user story self-contained.
- [X] Each per-commit deliverable (1 or 2 commits) is independently
      verifiable (per FR-009 each commit's pre-PR passes).

## Concreteness

- [X] FRs cite specific file paths and line numbers
      (`version_strings.rs::CuratedLibrary` enum, `match_prefix`
      function, `parse_semver_triple` reuse, `binary/entry.rs::version_match_to_entry`
      for downstream).
- [X] FR-002 names exact prefix bytes (`b"GnuTLS "`, `b"LibreSSL "`,
      `b"LLVM version "`, `b"OpenJDK "`).
- [X] FR-003 names exact OpenJDK acceptance grammar (modern +
      legacy schemes).
- [X] FR-004 names 8 specific test names + their inputs/expected
      outputs.
- [X] SC-004 quantifies the LOC ceiling (700 — current 452 +
      ~150-200 expected).
- [X] SC-007 (27-golden regen zero diff) names the verification
      mechanism.

## Internal consistency

- [X] FR-001 (enum variants) align with FR-002 (prefix arms) align
      with FR-003 (parser) align with FR-004 (tests).
- [X] FR-005 (TODO marker) + FR-006 (design-notes deferred-backlog
      entry) align with the spec's Out of scope deferred-3
      callout. Both touch points name the same 3 libraries with
      matching blocker descriptions.
- [X] Edge Case "OpenJDK two schemes" aligns with Scenarios 4 + 5
      and FR-003 acceptance grammar.

## Lessons from milestones 016-028

- [X] FR-009 carries the per-commit-clean discipline.
- [X] R3 in plan.md (LLVM strict-prefix gate) anticipates the
      false-positive-surface trade-off pattern that recurs in
      curated scanners.
- [X] Recon-first: every claim in the spec backed by a file:line
      reference from the pre-spec investigation.
- [X] **Not** framed as a bag consumer: this milestone produces
      new components, not annotations on existing components.
      The bag amortization streak stays at 4 (023/024/025/028);
      026 is purely scanner coverage breadth. The spec calls
      this out explicitly so future readers don't expect a
      5th-consumer framing.

## Pre-implementation

- [X] [PHASE-1] T001 reconnaissance done (2026-04-28).
- [ ] [PHASE-1] T002 baseline snapshot captured.
- [ ] [PHASE-2] Commit 1 (parsers + tests + TODO) landed.
- [ ] [PHASE-3] Commit 2 (deferred-backlog design-notes entry)
      landed.
- [ ] [POLISH] SC-001-SC-008 verified.
- [ ] [POLISH] All 3 CI lanes green.

## Post-merge

- [ ] [QUALITATIVE] Next time someone scans a curl-with-GnuTLS or
      a clang-built binary, mikebom emits the corresponding
      `pkg:generic/gnutls@X.Y.Z` or `pkg:generic/llvm@X.Y.Z`
      component automatically. If yes, milestone delivered.
- [ ] [TRACKING] Deferred-backlog entry in `docs/design-notes.md`
      is the canonical "what's left and why" for glibc/musl/V8.
      Future contributors discover it via grep on `TODO(milestone-026.x)`
      in `version_strings.rs` or by reading the deferred section.
