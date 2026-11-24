# Implementation Plan: GHC boot libraries carry their dependency edges, or the document says they don't

**Branch**: `1653-boot-library-edges` | **Date**: 2026-11-24 | **Spec**: [spec.md](spec.md)
**Input**: `specs/1653-boot-library-edges/spec.md`. Refs #1688, #1650, #1681, #966.

## Summary

**The data:** a committed table, `ghc_boot_dependencies.tsv`, gives each boot library's direct dependencies for each of the 36 GHC releases milestone 966 covers. A probe builds it from GHC's own release bindists (R1).
- It agrees with nixpkgs' installed package databases in 5 of 5 releases checked, including every compiler output present in CI.
- It agrees with milestone 966's library sets and versions in 36 of 36.

**The scan:** under `--nix-closure`, a post-pass adds the edges between the boot-library components of the same compiler that the document already contains, and stamps the new per-component row C245 with their source (R4, R5).

**The verdict:** a new completeness reason, `boot-library-edges-incomplete`, counts boot libraries whose dependencies are absent from the document or not in the table. CycloneDX `compositions[]` agrees through an explicit case.

**Measured effect on the goldens (R2):**
- 148 to 169 edges per target that can be drawn to components already in the document; the count after is measured, because some may already exist (the `ghc` library has edges today).
- `base` reaches `ghc-prim` on every project compiler.
- All five goldens carry the reason, because `rts` and `ghc-internal` are never components (R3).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde_json` (C245, the reason), `tracing`. The table is `include_str!` data parsed with std, as milestone 966's is. The probe is Python 3 stdlib (`urllib`, `lzma`, `tarfile`), run with `-I -B` at development time only. **Zero new Cargo dependencies.**
**Storage**: one committed TSV (1,371 rows: the 36 releases' libraries, Windows-only rows excluded), embedded. No scan-time network or store reads.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests for the table, the pass and the reason code;
- integration tests on milestone 966's stub-nix closure fixtures, in three formats with parity;
- mutations;
- a corpus dispatch with `regen_goldens=true` for the five closure goldens, every diff traced.

**Target Platform**: every host.
**Project Type**: CLI (`waybill sbom scan --nix-closure`).
**Performance Goals**: one pass over the C218 components, with table lookups. No measurable cost.
**Constraints**: FR-006 (no scan-time network, build, eval or store read), and FR-007 (byte-identical without `--nix-closure`).

**Scale/Scope**:
- `scripts/ghc-boot-dependencies.py` (the probe);
- `waybill-cli/src/scan_fs/package_db/nix/haskell_packages/{ghc_boot_dependencies.tsv, boot_dependencies.rs}` (the table and its loader);
- `waybill-cli/src/resolve/boot_library_edges.rs` (the pass), plus its call in `cli/scan_cmd.rs`;
- `generate/graph_completeness/{reason_codes,mod}.rs` and `generate/cyclonedx/compositions.rs`;
- `parity/extractors/{mod,cdx,spdx2,spdx3}.rs` (C245);
- docs: `sbom-format-mapping.md` (C245, C105), and the nix closure docs page where milestone 966 is described;
- CHANGELOG;
- the five closure goldens, through CI.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. Committed data and std parsing. The Python probe is development-time only, like milestone 966's. |
| III. Fail closed | Pass. A release or library missing from the table is `unknown`, and is counted in the verdict. A dependency absent from the document is listed, and is never synthesised. |
| IV. Type-driven correctness | Pass. The table is typed rows. The reason is a closed enum variant. `compositions` gets an explicit arm. No `.unwrap()` in production. |
| V. Specification compliance | Pass. Audit: CycloneDX `dependencies[]` has no per-edge provenance, and SPDX's relationship `comment` cannot be matched in CycloneDX. KEEP-NO-NATIVE, so C245 is per component (R5). |
| VII. Test isolation | Pass. Stub nix, no network, isolated `HOME`. The probe is never run by tests. |
| VIII. Completeness | Restored. Edges that exist are drawn; edges that cannot be are counted. |
| IX. Accuracy | The issue's premise was corrected by measurement: 9.10's `base` depends on `ghc-internal`, not `ghc-bignum`. No edge is guessed; every one comes from a release's own package database. |
| X. Transparency | C245 says, on each boot library, where its dependencies came from and which are absent. |

**Post-design re-check:** unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1653-boot-library-edges/
├── spec.md
├── plan.md
├── research.md            # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/wire.md      # C245, the reason code, compositions
├── measurements/          # probes, agreement check, baseline
└── checklists/requirements.md
```

### Source Code (repository root)

```text
scripts/ghc-boot-dependencies.py
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/
├── ghc_boot_dependencies.tsv     # new, committed data
├── boot_dependencies.rs          # new: loader + queries
└── mod.rs                        # `mod boot_dependencies;`
waybill-cli/src/resolve/
├── boot_library_edges.rs         # new: the pass
└── mod.rs
waybill-cli/src/cli/scan_cmd.rs   # call after closure_resolved_tier::retier
waybill-cli/src/generate/graph_completeness/{reason_codes,mod}.rs
waybill-cli/src/generate/cyclonedx/compositions.rs
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs
waybill-cli/tests/boot_library_edges.rs   # new integration tests, stub nix
docs/reference/sbom-format-mapping.md
CHANGELOG.md
```

## Complexity Tracking

No violations.

**A follow-up issue,** to be filed during implementation: emitting each compiler's full boot-library set as components, so that edges to `rts`, `ghc-internal` and the others have targets (R3). It is out of scope here, because it adds a component population.
