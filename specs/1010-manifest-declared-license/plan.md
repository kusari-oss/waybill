# Implementation Plan: The scanned project's declared license reaches its SBOM

**Branch**: `1010-manifest-declared-license` | **Date**: 2026-09-26 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1010-manifest-declared-license/spec.md`

## Summary

Thirteen of the fourteen production main-module construction sites emit
`licenses: Vec::new()` while holding, a few lines above, the parsed manifest table
that declares the license. This reads it. The fourteenth is haskell, already
populated by #957, whose failure branch this feature nonetheless changes. The
thirteen span eleven ecosystems; gem and npm each have two sites.

The technical approach is a per-reader extraction function feeding a shared
two-step resolution ladder: attempt strict canonicalisation, and on failure
preserve the raw text leniently so the existing emission path mints a non-listed
license reference. Where a manifest declares several licenses the reader combines
them itself, using its ecosystem's documented operator and conjunction where the
ecosystem is silent. Where a manifest declares inheritance rather than a literal
value, the reader resolves it against the workspace or parent root.

Phase 0 established that **no emitter change is required** — the declared/concluded
attribution and the `LicenseRef` path both already exist and are already exercised
by the OS-package readers. The work is therefore concentrated entirely in
`scan_fs/package_db/`, plus one correction to the reader #957 already shipped.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `spdx` (already backs `SpdxExpression`), `toml`, `serde_json`, `serde_yaml`, `quick-xml`, `regex`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; licenses are attached to `PackageDbEntry` and flow through the existing resolution pipeline.
**Testing**: `cargo +stable test --workspace` with per-reader unit tests and per-ecosystem fixtures; `cargo +stable clippy --workspace --all-targets`.
**Target Platform**: Linux, macOS, Windows — user-space only, no platform-specific paths.
**Project Type**: CLI / library (three-crate Cargo workspace).
**Performance Goals**: No measurable change. Extraction reads a field from a table the reader has already parsed; it adds no file reads, no subprocess calls and no network access. Inheritance resolution reuses a lookup the cargo reader already performs for `version.workspace`.
**Constraints**: Offline-capable by construction (FR-012) — the whole point is that this data does not require network access. Deterministic output (FR-015).
**Scale/Scope**: 14 production main-module sites in total, 13 requiring work, across 11 ecosystems (gem and npm have two sites each); 1 shared helper; 1 correction to #957. No new crates, no new CLI flags, no new annotations.

## Constitution Check

*GATE: evaluated before Phase 0, re-evaluated after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | PASS — no new dependencies at all, so no linkage question arises. |
| **II. eBPF-Only Observation** | PASS — and explicitly so. Principle II permits external sources to **enrich** discovered components and names "license data" as an example. No component is introduced. |
| **III. Fail Closed** | PASS — FR-007 and FR-011b require extraction never to fail a scan, which is the correct reading here: an absent or unresolvable *license* is missing metadata, not a failed observation. Principle III governs failure to observe dependencies, which this does not touch. |
| **IV. Type-Driven Correctness** | PASS — licenses are carried by the existing `SpdxExpression` newtype, not `String`. No new domain primitive is introduced. Test code using `unwrap` must keep the established `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard. |
| **V. Specification Compliance** | PASS, with audit cited below. No `waybill:*` property is introduced; every signal uses a native construct. |
| **VI. Three-Crate Architecture** | PASS — changes are confined to `waybill-cli`; `waybill-common` is used unchanged. |
| **VII. Test Isolation** | PASS — all tests are unit/integration level, no privileges, no eBPF. |
| **VIII. Completeness** | ADVANCES — this is a completeness feature. It removes a class of false negative in which a declared fact is discarded. |
| **IX. Accuracy** | ADVANCES, and is the reason for two design decisions: the reader (not the emitter) chooses the multi-license operator (FR-010b), and an uncanonicalisable value is never presented as a recognised identifier (FR-004c). |
| **X. Transparency** | PASS — FR-004b requires a diagnostic when a value is preserved rather than canonicalised; the declared/concluded attribution makes the source of every license visible natively. |
| **XI. Enrichment** | PASS — unchanged. |
| **XII. External Data Source Enrichment** | PASS — XII.1 satisfied (no new components); XII.2 satisfied by native provenance, see audit; XII.3 satisfied (no external service involved, so nothing to degrade); XII.4 unaffected. |

### Principle V native-construct audit (required citation)

Audited each target format for an existing construct before considering any
`waybill:*` property. One exists for every signal, so none is introduced:

| Signal | CycloneDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| License declared by the project | `licenses[].license.acknowledgement: "declared"` | `licenseDeclared` | declared-attribution expression element |
| License concluded by a third party | `acknowledgement: "concluded"` | `licenseConcluded` | concluded-attribution element |
| License not on the standard list | — (expression carries it) | `LicenseRef-<id>` + `hasExtractedLicensingInfos` | custom-license element |

This audit also discharges **Principle XII.2**, which requires externally-sourced
data to carry provenance. The declared-versus-concluded distinction *is* that
provenance expressed natively, which is why clarification declined a
`waybill:license-source` annotation: Principle V permits a `waybill:*` field only
for information the standard cannot express.

### Noted pre-existing tension (not introduced here)

Principle II forbids static manifest parsing **as a dependency source**, while
`sbom scan` performs manifest-based discovery. That conflict predates this feature
and is tracked in **#987**. This feature neither widens nor narrows it: it attaches
metadata to components the scan already discovered by whatever means, and adds no
component.

## Project Structure

### Documentation (this feature)

```text
specs/1010-manifest-declared-license/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── license-extraction.md   # Per-ecosystem extraction contract
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── declared_license.rs       # NEW — shared resolution ladder + join helper
├── cargo.rs                  # build_cargo_main_module_entry            (licenses@703)
├── npm/
│   ├── walk.rs               # build_npm_main_module_entry              (licenses@670)
│   └── mod.rs                # synthesize_nameless_nested_mainmods      (licenses@716)
├── pip/mod.rs                # build_pip_main_module_entry              (licenses@1022)
├── gem.rs                    # build_gem_main_module_entry              (licenses@1505)
│                             # build_gem_application_main_module_entry  (licenses@1786)
├── maven.rs                  # build_maven_main_module_entry            (licenses@4434)
├── composer.rs               # emit_main_module                         (licenses@560)
├── elixir.rs                 # emit_main_module                         (licenses@1074)
├── erlang.rs                 # build_main_module_component              (licenses@1489)
├── scala.rs                  # build_main_module_component              (licenses@1322)
├── cocoapods.rs              # emit_main_module                         (licenses@686)
├── nuget/mod.rs              # build_nuget_main_module_entry            (licenses@781)
├── haskell.rs                # build_main_module — CORRECT to preserve  (licenses@1780)
└── golang/legacy.rs          # comment-only: stale #103 references @965, @4185

waybill-cli/src/generate/          # UNCHANGED — Phase 0 R4 established this
docs/reference/                    # per-ecosystem operator + inheritance table
```

**Structure Decision**: one new module, `declared_license.rs`, holding the
resolution ladder (FR-004a), the join helper with its per-ecosystem operator
table (FR-010), and the shared diagnostic (FR-004b). Each reader supplies only
the ecosystem-specific extraction — which key to read and how to resolve
inheritance — and calls the shared helper. The alternative, duplicating the
ladder in thirteen places, would let the ecosystems drift apart, which is the
very defect User Story 3 exists to prevent and which #957 has already begun by
implementing drop-on-failure alone.

## Phase 1 Design Notes

**Scan-root inheritance (FR-016/FR-017)** is not a reader concern. It needs the
resolved component set to count main-modules, so it belongs after reader output —
in the same pass that already selects the root. Placing it in a reader would be
wrong: no single reader can know whether another ecosystem also produced a
main-module, and the rule is conditional on exactly that count.

**Ordering against #1008.** This changes emitted output for eleven ecosystems, so
corpus goldens must be regenerated. The corpus lane is presently red from
unrelated accumulated drift. Regenerating both at once makes the diff unreadable,
and reading that diff is what catches regressions (the #890 refresh needed three
passes and caught two). #1008 should be cleared first.

### Post-Phase-1 re-evaluation

Re-checked after design. No gate changes, and one improves:

- **Principle IV strengthens.** Phase 1 introduced `DeclaredLicense`, a
  three-state reader-internal type, in place of `Option<SpdxExpression>`. `Option`
  cannot distinguish a canonicalised value from preserved raw text, yet the two
  must be emitted differently and only one may be presented as an identifier.
  Encoding that in the type makes FR-004c enforceable at compile time rather than
  by reviewer attention.
- **Principle V unchanged** — Phase 1 added no `waybill:*` property. The emission
  contract in `contracts/license-extraction.md` maps every reader outcome to a
  native construct in all three formats.
- **Principle VI unchanged** — the one new module lives in `waybill-cli`; no crate
  is added and `waybill-common` is used as-is.
- **Principle I unchanged** — still zero new dependencies.

No violations. Nothing to record below.

## Complexity Tracking

> No Constitution Check violations. Nothing to justify.
