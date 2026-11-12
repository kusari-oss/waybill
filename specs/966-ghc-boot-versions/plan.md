# Implementation Plan: GHC boot-library versions from the compiler the closure names

**Branch**: `966-ghc-boot-versions` | **Date**: 2026-11-12 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/966-ghc-boot-versions/spec.md`

## Summary

When `--nix-closure` identifies the GHC that built the project, waybill gives every GHC boot library that GHC's version. The versions come from a committed, dated table keyed by `ghc-X.Y.Z`. The table is produced by a probe from GHC's release notes, with Stack's `global-hints.yaml` filling gaps and GHC's release tags settling disagreements. It is verified against every GHC package database available to the test.

The compiler is derived from the closure before m926 runs. m926 therefore classifies the design tier's declarations with the exact boot set and versions boot libraries in its existing pass. The m1275 identity pass binds each confirmed member's boot-library edges to the version its own compiler shipped, which gives the bootstrap compiler its own components.

Two new rows record the outcome:
- C218 (per component): which compiler a version came from.
- C219 (document): which compilers were found, whether the table had them, and the table's date.

Scans without a closure are byte-identical.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only. The table is `include_str!` data parsed with std, `serde_json` serializes C219, and `tracing` is used. The probe is Python 3 stdlib, run with `-I -B`. **Zero new Cargo dependencies** (FR-012).
**Storage**: one committed TSV (1,407 rows across 36 releases in the 2026-11-12 Stack snapshot), embedded in the binary. No runtime state and no scan-time network.
**Testing**: `cargo test`.
- Unit tests for the table parser and the `GhcRelease` parser.
- `tests/nix_closure_boot_versions.rs`: stub `nix` plus fixture closures, covering the FR-013 cases.
- `tests/ghc_boot_libraries_db.rs`: the FR-004 check against real package databases, made required in CI with `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1`.
- Corpus goldens regenerated in CI.

**Target Platform**: wherever `--nix-closure` runs (macOS, Linux). The table check runs on both. CI makes it required on Linux.
**Project Type**: CLI.
**Performance Goals**: negligible. One table load per process, and a per-member `nativeBuildInputs` scan that m1275 already performs.
**Constraints**:
- Byte-identical output without a closure (FR-011).
- No version is ever chosen between two (Principle IX). Two compilers that disagree yield two components.
- `Win32` is never versioned on a non-Windows system.

**Scale/Scope**:
- One table and one probe.
- Two catalog rows (C218, C219), with parity extractors in all three formats.
- Changes to m926 (`enrich` and `ResolveOptions`), m1275 (`emit_declared_edges`) and `scan_cmd` wiring.
- Five closure goldens regenerate.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, statically linked | Pass | No new crates. The table is embedded data. |
| III Fail closed / X transparency (scan mode) | Pass | An unknown compiler, or one absent from the table, leaves today's `compiler-supplied` behaviour, recorded in C219. C218 names the source of every table-derived version. |
| IV Type-driven correctness | Pass | `GhcRelease` is produced only by the shared store-path parser. Table rows are parsed once, with every malformed line a test failure, never a runtime panic. No new `.unwrap()` outside `#[cfg(test)]`. |
| V Specification compliance (native first) | Pass | Research R6 audits CDX `evidence.identity`, SPDX 2.3/3 and existing rows C170, C171, C175, C181, C174, C216 and C217. Both new rows are KEEP-NO-NATIVE with that justification and have extractors in all three formats. |
| VII Test isolation | Pass | Every behavioural test uses stub `nix` and fixtures. The package-database check reads the store read-only and evaluates nothing. It is required only where CI opts in. |
| IX Accuracy | Pass | A version comes only from a published fact keyed by the exact compiler release. Two compilers that disagree are two components, not one picked. Platform-specific rows are filtered. The probe refuses an unexplained disagreement between sources. |
| XII External data sources | Pass | The table is external data observed on a recorded date and committed. Scan mode resolves what the evidence declares: the project's `.cabal` and the members' declared relations. Every resulting version carries its provenance (C218, C219), and no network access happens at scan time. |

No violations. **Re-check after Phase 1:** still passes. The design adds no new kind of evidence. It changes the input m926 already classifies with, and the target m1275 already binds to.

## Project Structure

### Documentation (this feature)

```text
specs/966-ghc-boot-versions/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── boot-library-table.md
│   ├── c218-boot-library-compiler.md
│   └── c219-ghc-boot-libraries.md
├── measurements/        # #966 research probes and results (README.md = posted findings)
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
scripts/ghc-boot-libraries.py                                         # probe → TSV (stdlib only)
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/
├── ghc_boot_libraries.tsv                                            # committed table
├── boot_table.rs                                                     # new: table load, ships(), version_of()
├── closure_compilers.rs                                              # new: GhcRelease, ClosureCompilers from a ClassifiedClosure
├── mod.rs                                                            # ResolveOptions.compilers; exact boot set; versioned boot resolution; C218
├── boot_libraries.rs                                                 # unchanged union rule (fallback)
└── closure_identity.rs                                               # emit_declared_edges: per-member compiler binding (research R7); is_ghc_store_path shares the parser
waybill-cli/src/cli/scan_cmd.rs                                       # derive ClosureCompilers after nix_closure; pass to enrich + reclassify; C219 into the summary
waybill-cli/src/generate/{cyclonedx/metadata,spdx/annotations,spdx/v3_annotations}.rs  # C219
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs            # C218, C219
docs/reference/sbom-format-mapping.md                                 # C218, C219 rows; C170 note
docs/user-guide/ (Nix closure section)                                # one paragraph: boot-library versions and their source
waybill-cli/tests/nix_closure_boot_versions.rs                        # new
waybill-cli/tests/ghc_boot_libraries_db.rs                            # new (FR-004)
waybill-cli/tests/fixtures/nix_closure_boot_versions/                 # new fixture closures (synthetic names where not GHC libraries)
.github/workflows/public-corpus.yml                                   # WAYBILL_REQUIRE_GHC_PACKAGE_DB=1 on the Linux job
```

**Structure Decision**: the existing single-workspace layout. The new code lives beside m926's `boot_libraries.rs`, because it replaces that file's answer whenever the compiler is known.

## Phasing

1. **Table.** Probe, TSV, `boot_table.rs` with parser tests, and the FR-004 database check passing locally on 9.6.6 and 9.10.3, with a mutation shown to fail it.
2. **Compilers.** `GhcRelease`, `ClosureCompilers`, and the shared store-path parser.
3. **US1 + US2 (design tier).** `ResolveOptions.compilers` and the exact boot set in `enrich`. Boot libraries resolve to table versions through m926's resolved path, which brings renames, C218, and removal of C170, C171 and C151. `os-string` re-enters ordinary resolution.
4. **Edges (closure tier).** Per-member compiler binding in `emit_declared_edges`, plus versioned FR-012c components.
5. **US3 + records.** C219 in all three formats, the C218 extractors, and the catalog rows.
6. **Corpus.** CI regen, golden review, and `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1` in `public-corpus.yml`.
7. **Polish.** CHANGELOG, the user-guide paragraph, the SC-002 osv-scanner check on the regenerated goldens, and the gate.

## Complexity Tracking

No constitution violations to justify.
