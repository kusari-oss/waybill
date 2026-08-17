# Research: Universalize `waybill:unresolved-reason`

**Milestone**: 236 | **Date**: 2026-08-16

Phase 0 output. All decisions consumed by `plan.md` Phase 1 + `tasks.md`.

## R1 — Type shape: raw `String` vs newtype

**Decision**: raw `String`.

**Rationale**: The annotation value is a display-only human-readable string (per Q1 clarification). Principle IV's newtype rule protects against primitive-collision bugs (PURL vs license expression etc); no such collision risk exists here. NuGet's raw-String precedent (PR #656) already ships; matching it minimizes surface area.

**Alternatives**:
- `UnresolvedReason(String)` newtype in `waybill-common`: adds boilerplate (Display/From/Serialize) without unlocking correctness.
- `UnresolvedReason` enum: rejected — reason strings are open-ended per Q1.

## R2 — Per-reader reason strings

**Decision**: One reader-specific human-readable string per reader; each names the specific resolution boundary + fallbacks tried. Locked contract in `contracts/per-reader-strings.md`.

**Rationale**: FR-002 requires human-readable + boundary-naming. Each string is ASCII English (matches NuGet precedent), <200 chars, no PII/paths/credentials (FR-010).

**Alternatives**:
- Terser one-liners ("no lockfile"): rejected as insufficiently actionable for human reviewers.
- Localized strings (i18n): out of scope — waybill has no i18n infrastructure anywhere.

## R3 — Catalog row for `waybill:unresolved-reason`

**Decision**: Task-time verification via `grep -n "waybill:unresolved-reason" docs/reference/sbom-format-mapping.md`. Two paths:

1. **Row exists (PR #656 landed it)**: extend the catalog's reason-string enumeration to cover all readers; the parity extractor already treats it as per-component. Add extractor coverage assertion for the new readers.
2. **No row exists**: land the row + extractor triple (matches m235 C147/C148/C149/C150 exact pattern).

**Rationale**: Either path ships identical wire behavior. Determined at task-execution time to avoid stale assumptions.

**Alternatives**: Skip parity plumbing → rejected (m071 `every_catalog_row_has_an_extractor` + `holistic_parity` tests would fail; also breaks CDX/SPDX/SPDX3 symmetric-equality guarantee per FR-008 + SC-002).

## R4 — Cross-reader integration test corpus

**Decision**: 17 minimal per-reader fixtures at `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/<reader>/`, each producing ≥1 design-tier component. Cross-reader integration test in `waybill-cli/tests/unresolved_reason_universal.rs` scans a directory containing all 17 fixtures + asserts every emitted design-tier component carries the annotation.

**Rationale**: Matches m235 gradle_ladder + m226 pants_go patterns. Minimal fixture size = tight test isolation. Synthetic names throughout per `feedback_fixture_synthetic_package_names` memory.

**Alternatives**:
- Reuse existing fixtures: rejected — they carry unrelated assertions that would complicate the test.
- Single mega-fixture with all 17 ecosystems: rejected — noise (readers not under test) would obscure failures.
