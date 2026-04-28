# Spec Quality Checklist: cargo-auditable Extraction

**Checklist for** `/specs/029-cargo-auditable-extraction/spec.md`

## Coverage

- [X] Background section explains why cargo-auditable extraction is
      missing today + cites file:line evidence (`scan.rs:122-135`
      for the section-reader pattern; `mikebom-cli/Cargo.toml:48`
      for `flate2 = "1"`; `binary/entry.rs:21` for the
      version_match_to_entry shape this milestone mirrors).
- [X] User story has a P-priority (P1 — correctness) and a "why this
      priority" justification grounded in the same data-quality
      argument that 023 (ELF identity) and 025 (Go BuildInfo VCS)
      made: the binary already carries the answer; mikebom needs to
      read it.
- [X] Independent Test is concrete (specific test commands +
      observable manifest extraction).
- [X] Acceptance scenarios use Given/When/Then framing (5 scenarios
      covering happy path + cross-format + malformed + source
      qualifier mapping + negative case).
- [X] Edge Cases section names the corner cases (Mach-O segment
      prefix, duplicate crate entries, workspace-vs-binary, build
      vs dev kinds, massive manifests, deterministic emission).
- [X] Functional Requirements numbered (FR-001 through FR-012).
- [X] Key Entities — `CargoAuditableManifest` /
      `CargoAuditablePackage` types specified inline in FR-002.
- [X] Success Criteria measurable (SC-001 through SC-008), each
      with a verification mechanism.
- [X] Clarifications section captures the 4 scope decisions
      (Bool-typed annotation; per-crate components flow through
      PackageDbEntry not the bag; dep-graph edges preserve the
      manifest's index list; high-confidence tier).
- [X] Out of Scope explicitly names every adjacent concern (kind
      filtering; cross-binary dedup; workspace awareness;
      source-tree validation; container-layer cross-link; format
      version tracking).

## Tighter spec set rationale (4 files vs 8)

- [X] No `research.md` — recon answered every architectural
      question (8th use of the 4-file template after 021, 022, 023,
      024, 025, 026, 028). Pattern fully validated.
- [X] No `data-model.md` — only two new typed structs in
      `cargo_auditable.rs`, fully specified inline in FR-002.
- [X] No `contracts/` — no public API surface change beyond
      catalog row C36.
- [X] No `quickstart.md` — 4 short files self-explanatory.

This is the **8th use** of the 4-file template. Pattern is
fully validated for contained binary-extraction milestones.

## Independence

- [X] Single user story self-contained.
- [X] Each per-commit deliverable (3 commits) is independently
      verifiable (per FR-012 each commit's pre-PR passes).

## Concreteness

- [X] FRs cite specific file paths and line numbers
      (`mikebom-cli/Cargo.toml:48` for flate2; `scan.rs:122-135`
      for section-reader; `binary/entry.rs:21` for version_match_to_entry
      shape).
- [X] FR-001 names the exact entry-point signature.
- [X] FR-002 names the exact struct shape with serde derive.
- [X] FR-005 enumerates every PackageDbEntry field with its source.
- [X] SC-004 quantifies the LOC ceiling (400 for cargo_auditable.rs).
- [X] SC-007 (27-golden regen zero diff) names the verification mechanism.
- [X] SC-008 (no new crate deps) is verifiable via `cargo tree`.

## Internal consistency

- [X] FR-001-006 (parser + types + scan.rs + entry.rs + binary/mod.rs)
      flow end-to-end.
- [X] FR-007 + FR-008 (catalog + parity) align with the
      holistic_parity regression gate.
- [X] Edge Case "Mach-O segment-prefix on `.dep-v0`" aligns with
      the existing object-crate pattern (`section_by_name_bytes`
      ignores Mach-O segment prefix per the
      `__DATA,__cstring`/`__TEXT,__const` reads in
      `scan.rs::collect_string_region`).
- [X] Determinism contract (sort by `(name, version, source)`)
      aligns with the existing cargo lockfile reader's ordering.

## Lessons from milestones 016-028

- [X] FR-012 carries the per-commit-clean discipline.
- [X] Bag-first design (milestone 023) means SC-005 is automatic.
- [X] **5th amortization-proof consumer** — three identity-cohort
      milestones (023, 024, 028), one go-vcs milestone (025), and
      now this. Bag pattern fully validated.
- [X] Recon-first: every claim in the spec backed by a file:line
      reference from the pre-spec investigation.
- [X] R3 in plan.md (PURL qualifier shape for git source) anticipates
      the same kind of "real-world data may diverge from documented
      shape" pattern that recurs in package-format readers.

## Pre-implementation

- [X] [PHASE-1] T001 reconnaissance done (2026-04-28).
- [ ] [PHASE-1] T002 baseline snapshot captured.
- [ ] [PHASE-2] Commit 1 (parsers + types + dead_code) landed.
- [ ] [PHASE-3] Commit 2 (wire-up + per-crate-entries + integration test) landed.
- [ ] [PHASE-4] Commit 3 (parity row) landed.
- [ ] [POLISH] SC-001-SC-008 verified.
- [ ] [POLISH] All 3 CI lanes green.

## Post-merge

- [ ] [QUALITATIVE] Next time someone scans a Rust binary built in
      Debian Trixie / Fedora 40 / Alpine Edge / a Rust container
      image, mikebom emits the full `pkg:cargo/<name>@<version>`
      crate closure automatically, and consumers can directly
      query OSV / NVD / Vex / Kusari Inspector against each crate.
      If yes, milestone delivered.
- [ ] [BAG STREAK] 5 consecutive amortization-proof bag consumers
      (023 → 024 → 025 → 028 → 029) — design pattern conclusively
      validated.
