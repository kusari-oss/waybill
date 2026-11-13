# Implementation Plan: Operator-supplied CPEs, and the curated CPE table as data

**Branch**: `1491-operator-cpe-table` | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md) | **Issues**: #1413, #1491

## Summary

This feature ships as two stacked PRs (FR-017).

**PR A** moves the curated CPE table into one TOML file, `waybill-cli/src/generate/cpe_table.toml`, embedded with `include_str!`. The file replaces two Rust tables:
- `GENERIC_LIBRARY_CPES`: 82 rows;
- `NVD_DICTIONARY_OBSERVED`: 95 observations.

Each pair carries its dictionary count, observation date, dictionary evidence path and title evidence path.

The enforcement tests read the file, and gain two checks: every evidence path exists, and every count is positive and matches its evidence. Emitted output is byte-identical (FR-014).

The schema and parser are shared, so PR B's `--cpe-map` reads the same format.

**PR B** merges PR A's exact head and adds the operator surfaces:

| Surface | What it does |
|---|---|
| `--root-cpe` | Sets the subject's CPEs. Repeatable; conflicts with `--split`. |
| `--cpe-map` | A versionless-PURL-keyed mapping file. |
| `--supplement-cdx` | Now wins over a synthesized CPE (FR-003). |

A single internal marker makes operator CPEs final against every later pass (research R5).

Provenance is recorded natively as a third source, `Operator(surface)`, on every component. C212 gains `purl`, `cpe-map` and `root-cpe` basis values, with `overrode`. No golden changes, because no corpus target uses a supplement or the new flags.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only.
- `toml = "0.8"`, already a direct `waybill-cli` dependency, used by `cargo.rs`;
- `serde` derive, for the row structs;
- `waybill_common::types::cpe::Cpe` (#1403), for validation;
- `waybill_common::types::purl::Purl`, for map keys;
- `clap`, for the two flags;
- `tracing`.

**Zero new Cargo dependencies.**
**Storage**: One committed data file, embedded at build time. The operator file is read once per scan. No cache, no network (FR-015).
**Testing**:
- **Unit tests** in `generate/cpe.rs` (table checks) and in the new `generate/cpe_table.rs` (schema and validation).
- **Integration tests** for PR B in `waybill-cli/tests/operator_cpe.rs`, with synthetic fixtures. They extend `supplement_cdx_integration.rs`'s harness for the supplement cases.
- **Corpus check**, non-regenerating, for PR A's byte-identity.
- **Mutation checks** for each rule (SC-007).

**Target Platform**: Every host. The table is data in the binary; the map is a local file.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: The table is parsed once per process, and the lookup is a hash map instead of today's linear scan over 82 rows. The operator map is O(rows) to load and O(1) per component.
**Constraints**:
- PR A: byte-identical goldens.
- PR B: byte-identical output when no operator surface is used (FR-016).
- C212 stays `SymmetricEqual`.

**Scale/Scope**: 82 rows, 96 pair occurrences and 95 unique pairs. All 95 pairs' dictionary evidence is one committed census, with 2 counts updated to match it (research R1). Title evidence is in six committed files.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, Statically Linked | Pass | No dependency added. |
| III Fail Closed | Pass | A malformed `--root-cpe` or map row stops the scan before any output. A built-in table that fails to parse yields no curated CPEs, never a guessed one (R3). |
| IV Type-Driven Correctness | Pass | Every operator CPE is a `Cpe`. A map key is a parsed `Purl`, rejected when it carries a version. `CpeSource` gains a typed variant and no string flag. |
| V Specification Compliance | Pass | The source is carried in native fields: CycloneDX `evidence.identity[]` and the SPDX `comment` fields. C212 holds only what no format has a field for: the join basis and the overridden identifier. |
| VII Test Isolation | Pass | The table tests read committed files only. Fixtures are synthetic `waybill-fixture-*`. No test touches NVD. |
| IX Accuracy | Pass | This is the point of US2. Today a synthesized guess silently beats an operator's explicit identifier. |
| X Transparency | Pass | Every replaced CPE is recorded as `overrode`, the supplement conflict record is kept, and unused map rows are logged. |
| XII External Data | Pass | No scan-time fetch. NVD figures are dev-time evidence, referenced by path. No CVE data is shipped (R10). |

**Post-design re-check**: Pass. One consumer-visible contract widens: C212 is no longer closure-only and has new `basis` values, as `contracts/wire.md` states. One m119 rule reverses: `cpe` moves from scanner-wins to operator-wins, which clarification Q1 settled.

## Project Structure

### Documentation (this feature)

```text
specs/1491-operator-cpe-table/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/cli.md, contracts/wire.md
├── checklists/requirements.md
└── measurements/
    ├── evidence-attribution.py
    ├── evidence-attribution-2026-11-13.tsv
    └── evidence-attribution-2026-11-13.summary
```

### Source Code

```text
PR A
waybill-cli/src/generate/cpe_table.toml   # NEW: the 82 rows, with per-pair evidence
waybill-cli/src/generate/cpe_table.rs     # NEW: schema (CpeTable/CpeRow/CpePair), parse, validate, built-in OnceLock
waybill-cli/src/generate/cpe.rs           # tables removed; generic arm reads cpe_table; tests read the file
waybill-cli/src/generate/mod.rs           # mod cpe_table
docs/reference/sbom-format-mapping.md     # C209 note: where the curated table lives (wording only)
CHANGELOG.md                              # Changed: the table is data

PR B
waybill-cli/src/generate/cpe_source.rs    # NEW: CpeSource {Operator(surface), NixpkgsDeclared, Curated}, cpe_source(), marker key
waybill-cli/src/scan_fs/package_db/nix/closure/identifiers.rs  # re-export; constants stay
waybill-cli/src/supplement/conflict.rs    # cpe → developer wins; marker + C212 basis purl
waybill-cli/src/supplement/merge.rs       # solo entries: marker + C212; metadata.component.cpe WARN
waybill-cli/src/supplement/parser.rs      # detect metadata.component.cpe for the WARN
waybill-cli/src/cli/scan_cmd.rs           # --root-cpe, --cpe-map; apply_cpe_map pass; marker-aware replaceable / record passes
waybill-cli/src/generate/mod.rs           # RootComponentOverride gains cpes; subject_cpes() helper
waybill-cli/src/generate/cyclonedx/{metadata,evidence}.rs  # subject CPE + provenance; C212 propagation to metadata.component
waybill-cli/src/generate/spdx/{document,packages,v3_document,v3_external_ids}.rs  # subject CPE + comments
waybill-cli/src/generate/root_selector.rs # register waybill:operator-cpe as internal
waybill-cli/tests/operator_cpe.rs         # NEW: US1–US3 integration, all three formats, parity
docs/reference/sbom-format-mapping.md     # C212 row widened
docs/ (CLI reference for the two flags), CHANGELOG.md (non-adjacent to PR A's entry)
```

**Structure Decision**: Single crate (`waybill-cli`).

- **The table and its parser** sit beside `cpe.rs`, which is their only consumer besides the `--cpe-map` loader.
- **Provenance** moves out of the closure module, because it now applies to every component. The closure module re-exports it, so m1377's call sites need not change.
- **Operator precedence** is enforced by one marker that each existing pass checks. There is no single re-ordered pass: the passes run in different places for good reasons, which `scan_cmd.rs:7215–7240` records.

## Complexity Tracking

None. Two concerns get a deliberate shape, not a violation:
- the per-row special cases stay code (R4);
- `--root-cpe` is applied by the emitters through one shared helper, because a synthetic subject is not a component (R5).
