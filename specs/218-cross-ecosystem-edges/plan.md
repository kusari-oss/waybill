# Implementation Plan: Cross-ecosystem dep-name edge resolution

**Branch**: `218-cross-ecosystem-edges` | **Date**: 2026-07-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/218-cross-ecosystem-edges/spec.md`

## Summary

Bridge the `(ecosystem, name)`-keyed dep-name resolver at `waybill-cli/src/scan_fs/mod.rs:794-810` so that when a `pkg:generic/`-source main-module's same-ecosystem lookup fails, the resolver falls back to iterating every other ecosystem present in the resolver's `name_to_purl` index. Emit each resulting cross-ecosystem edge with a per-edge `waybill:cross-ecosystem-inference` provenance annotation (payload `{target_purl, from_eco, to_eco, lookup_via}`) and — when the fallback yields multiple candidate ecosystems that tie-break-rule cannot narrow to one — emit ALL candidates each carrying `waybill:cross-ecosystem-inference-ambiguous`. Zero matches → the missing name lands in a document-scope `waybill:cross-ecosystem-inference-unresolved` annotation.

The entire behavior is gated behind an opt-in `--experimental-cross-ecosystem-edges` CLI flag (also `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=1`). Default OFF preserves current post-m216 waybill output byte-identically. Flag ON restores outgoing edges from `pkg:generic/` Ruby-app main-modules (and, per FR-009, all future m216-alike readers).

Three new parity-catalog C-rows (per-edge `waybill:cross-ecosystem-inference` + per-edge `waybill:cross-ecosystem-inference-ambiguous` + doc-scope `waybill:cross-ecosystem-inference-unresolved`) plus one new consumer-facing doc page at `docs/reference/cross-ecosystem-edges.md` linked from the top-level README and from `docs/reference/sbom-format-mapping.md`.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–217; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde` / `serde_json` (annotation value construction + canonicalization), `tracing` (INFO summary log per FR-013), `anyhow` / `thiserror` (error propagation), `clap` (new opt-in flag via `Args`-derive — same shape as m173 `--warm-go-cache` and m119 `--supplement-cdx`). Reuses milestone-071 parity-extractor infrastructure verbatim. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan. The cross-ecosystem resolver operates on the same `name_to_purl: HashMap<(String, String), String>` index that today feeds the same-ecosystem path; the flag-on path adds fallback iteration over the index's keyset.
**Testing**: `cargo +stable test --workspace` — unit tests for the tie-break rule + normalization, integration test extending the existing `waybill-cli/tests/transitive_parity_gem.rs` with flag-on assertions, new synthetic test proving FR-009 ecosystem-agnosticism (`pkg:generic/` → `pkg:pypi/`) without needing a real pip reader.
**Target Platform**: linux-x86_64 + macOS + Windows (all three CI lanes; matches every user-space milestone since m100 Windows-host-build). No eBPF surface touched.
**Project Type**: Library-inside-CLI resolver extension + emitter propagation. Single crate touched primarily (`waybill-cli`); parity catalog rows live in `waybill-cli/src/parity/extractors/`.
**Performance Goals**: Cross-ecosystem lookup cost is bounded by `O(E)` per failed same-ecosystem lookup, where `E` = number of ecosystems present in the scan's resolver index (typically ≤10 for the largest polyglot scans). At m216-baseline gem fixtures (27 DEPENDENCIES entries), flag-on adds ≤27 fallback iterations per scan — negligible against the existing whole-scan cost dominated by walker + reader I/O.
**Constraints**: Byte-identity when flag OFF MUST be exact (SC-009 gate — new integration test asserts). Byte-identity when flag ON MUST hold for scans with zero `pkg:generic/` main-modules (FR-008). Parity gate MUST stay green across CDX / SPDX 2.3 / SPDX 3 (FR-007, three new C-rows).
**Scale/Scope**: Single reader affected today (m216 Gemfile-only gem reader at `waybill-cli/src/scan_fs/package_db/gem.rs`). Future m216-alikes inherit for free per FR-009. New CLI flag surface: one boolean. New annotation surface: three (per-edge + per-edge + doc-scope). New documentation surface: one Markdown page (~200 lines with worked example).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Waybill Constitution v2.0.0 principles evaluated against this milestone:

- **I. Pure Rust, Zero C** — ✅ No C. New code is stdlib + workspace deps.
- **II. eBPF-Only Observation** — ✅ N/A. This is a resolver extension over lockfile-derived data. Lockfiles are permitted for enrichment under Principle XII (which explicitly names Cargo.lock, package-lock.json, go.sum, and by extension Gemfile.lock — the m216 substrate).
- **III. Fail Closed** — ✅ Flag-OFF path unchanged (fails to same behavior). Flag-ON path: cross-ecosystem lookup failing to match any ecosystem → FR-004 records the unresolved name in a doc-scope diagnostic annotation (transparent report per Principle X); resolver does NOT silently drop.
- **IV. Type-Driven Correctness** — ✅ Ecosystem strings are the same `String` used by same-ecosystem lookup (existing type); the new payload is a strongly-typed `CrossEcosystemInferencePayload` struct implementing `Serialize` (canonical-JSON ordering). No `.unwrap()` in production; test-only `.unwrap()` guarded per Constitution.
- **V. Specification Compliance** — ✅ Standards-native audit performed for the three new annotations:
  - **`waybill:cross-ecosystem-inference`** — REJECTED alternatives: CDX `dependency.type` (no such field), CDX `evidence.callstack` (call-stack semantic, not cross-ecosystem-inference), SPDX 2.3 `Relationship.comment` (unstructured; can't carry payload for parity), SPDX 3 `Relationship.evidence` (spec permits but has no `inference_mechanism` slot). **KEEP-NO-NATIVE**. Documented in `docs/reference/sbom-format-mapping.md` C137 row per the m216 C135 precedent.
  - **`waybill:cross-ecosystem-inference-ambiguous`** — REJECTED same alternatives; sibling to C137. **KEEP-NO-NATIVE**. Documented as C138.
  - **`waybill:cross-ecosystem-inference-unresolved`** — REJECTED CDX `metadata.properties[]` doesn't have "unresolved names" semantic; SPDX 2.3 `creationInfo.creators[]` is producer-scope (same rejection as m217 C136); SPDX 3 `SpdxDocument.observedProblems` doesn't exist. **KEEP-NO-NATIVE**. Documented as C139.
- **VI. Three-Crate Architecture** — ✅ Only `waybill-cli` touched. `waybill-common` untouched. `waybill-ebpf` untouched.
- **VII. Test Isolation** — ✅ New tests are unprivileged (no eBPF, no root). Runs under `cargo test --workspace` in every CI lane.
- **VIII. Completeness** — ✅ This milestone materially improves completeness (previously-dropped edges are now emitted with the flag on). Flag-OFF path preserves current completeness surface (unchanged from post-m216).
- **IX. Accuracy** — ✅ FR-003 emit-all-candidates behavior prevents false-positive single-winner picks in the multi-ecosystem ambiguous case. Ambiguity is transparent (annotation on every candidate edge) rather than silently resolved.
- **X. Transparency** — ✅ Three new annotations, one INFO log summary, one doc page. Everything the resolver decides is auditable at consumer parse time. Precedent: m216 `waybill:package-shape`, m217 `waybill:go-toolchain-detected` used the same silence-on-absence pattern for the unresolved doc-scope annotation.
- **XI. Enrichment** — ✅ Cross-ecosystem edges ARE enrichment over lockfile-observed dep-names (Principle XII lifts them from "declared name" to "resolved PURL edge"). The `lookup_via` field is a first-class provenance signal.
- **XII. External Data Source Enrichment** — ✅ Every cross-ecosystem edge is derived from a component that WAS observed by the eBPF trace (or lockfile-reader, per current Principle XII text extending the trace-first model). No new components are introduced by this milestone; only edges between already-observed components. The four Principle XII constraints all hold: (1) no new components; (2) provenance annotated per FR-005; (3) missing lookup gracefully recorded per FR-004; (4) trace + reader remain the authoritative discovery layer.

**Constitution check result: PASS.** No violations. No amendment required.

## Project Structure

### Documentation (this feature)

```text
specs/218-cross-ecosystem-edges/
├── plan.md              # This file
├── spec.md              # Feature specification (committed 5a9a265 + b3d205d)
├── research.md          # Phase 0 output (this /speckit-plan pass)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── cross-ecosystem-flag.md      # CLI flag surface + env-var behavior
│   ├── annotation-payloads.md       # C137/C138/C139 payload shape + landing slots
│   └── tie-break-rule.md            # FR-003 emit-all deterministic algorithm
├── checklists/
│   └── requirements.md              # Spec quality checklist (committed 5a9a265)
└── tasks.md             # Phase 2 output — /speckit-tasks
```

### Source Code (repository root)

The touched surface is entirely inside `waybill-cli`; `waybill-common` and `waybill-ebpf` stay byte-identical.

```text
waybill-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs                                          # add `--experimental-cross-ecosystem-edges` flag (Args-derive)
│   ├── scan_fs/
│   │   ├── mod.rs                                               # extend resolver at :794-810 with flag-gated cross-ecosystem fallback
│   │   └── package_db/gem.rs                                    # untouched (m216 emitter unchanged — the fix is at the resolver layer)
│   ├── generate/
│   │   ├── mod.rs                                               # add `cross_ecosystem_edges_report: Option<&'a CrossEcosystemEdgesReport>` to ScanArtifacts (+ narrow helper)
│   │   ├── cross_ecosystem_edges/                               # NEW module — 3-file dir
│   │   │   ├── mod.rs                                           #   payload type + report type
│   │   │   ├── tie_break.rs                                     #   FR-003 algorithm (pure function, unit-tested standalone)
│   │   │   └── normalize.rs                                     #   FR-012 target-ecosystem normalization helper
│   │   ├── cyclonedx/
│   │   │   ├── builder.rs                                       # thread the report field
│   │   │   ├── metadata.rs                                      # emit C139 doc-scope unresolved annotation
│   │   │   └── dependencies.rs (existing)                       # emit C137/C138 per-edge properties on dependencies[i]
│   │   └── spdx/
│   │       ├── annotations.rs                                   # SPDX 2.3 Package-scoped annotation with in-payload target-PURL
│   │       ├── v3_annotations.rs                                # SPDX 3 Annotation on the Relationship IRI
│   │       └── (companion doc/relationship files as needed for C139 doc-scope)
│   └── parity/extractors/
│       ├── cdx.rs                                               # register c137_cdx / c138_cdx / c139_cdx
│       ├── spdx2.rs                                             # register c137_spdx23 / c138_spdx23 / c139_spdx23
│       ├── spdx3.rs                                             # register c137_spdx3 / c138_spdx3 / c139_spdx3
│       └── mod.rs                                               # 3 new EXTRACTORS rows + use-list additions
└── tests/
    ├── transitive_parity_gem.rs                                 # extend with flag-on scan asserting recovered edges
    ├── cross_ecosystem_edges.rs                                 # NEW — 5+ scenarios (flag-off byte-identity; flag-on gem edges; flag-on synthetic pip edges; ambiguous multi-eco; unresolved-name)
    └── fixtures/
        └── cross_ecosystem/                                     # NEW — 2-3 tiny synthetic fixtures (pip-app-lookalike, multi-eco-ambiguous)

docs/
└── reference/
    ├── cross-ecosystem-edges.md                                 # NEW — FR-014 consumer-facing doc
    └── sbom-format-mapping.md                                   # 3 new C-rows: C137 / C138 / C139
```

**Structure Decision**: Resolver fix at `scan_fs/mod.rs:794-810` is the localized, ecosystem-agnostic choice (per spec Assumption). Emitter propagation follows the established m134/m173/m204/m217 pattern of threading a scan-scoped report through `ScanArtifacts`. Parity catalog gets three new C-rows to cover the three distinct annotations (per-edge inference, per-edge ambiguous, doc-scope unresolved). The three-file `generate/cross_ecosystem_edges/` module directory factors the tie-break algorithm and normalization helper into standalone unit-testable pieces.

## Complexity Tracking

> No constitution violations. Complexity table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
