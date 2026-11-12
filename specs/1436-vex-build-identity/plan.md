# Implementation Plan: Nix closure VEX that scanners honour, without overclaiming

**Branch**: `1436-vex-build-identity` | **Date**: 2026-11-12 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1436-vex-build-identity/spec.md`

## Summary

Each closure component whose patches resolve a CVE gets a build-identity PURL qualifier, `?nix_drv=<the patching derivation's store hash>`. For each (component, CVE) there is one VEX statement: OpenVEX `fixed`, product = that qualified PURL, no subcomponent. CycloneDX carries it as a single `resolved` entry on the component.

This replaces m1035's affected / not_affected pair. That pair suppressed 0 findings in grype and made CycloneDX mark patched components `exploitable` (#1421). The version-level claim stays in `pedigree.patches[].resolves`.

Measured on main's five closure goldens before any code (research R2): grype suppresses 17 of 17 patched findings with this shape, leaks onto 0 unqualified components, and loses 0 others. The qualifier changes no grype or osv-scanner finding (R2, R3).

A component whose patches come from several derivations is left unscoped, with no statement, and counted in a new row, C220. That is Principle IX: no statement that could cover a build it doesn't describe.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only — `waybill_common::types::purl::Purl` (qualifier encoding), `serde`/`serde_json`, `tracing`. The derivation map is already parsed by the closure tier. **Zero new Cargo dependencies** (FR-012).
**Storage**: N/A. In-process per scan.
**Testing**: `cargo test`.
- `tests/nix_closure_backport_vex.rs` covers the FR-011 cases with a stub `nix` and fixture closures.
- Parity tests for C220.
- The orphan-endpoint check.
- The scanner gate (`measurements/fixed-qualified-probe.sh`), re-run on regenerated goldens with the emitted sidecar.

**Target Platform**: wherever `--nix-closure` runs.
**Project Type**: CLI.
**Performance Goals**: negligible. The derivation keys are already in memory.
**Constraints**:
- No new network access at scan time (which ruled out a NAR-hash `checksum`, research R1).
- Byte-identical output for scans without `--nix-closure`, and for components without a resolved CVE.

**Scale/Scope**:
- One qualifier.
- One statement shape (OpenVEX and CycloneDX).
- One new catalog row (C220), and updates to F1 and the m1035 documentation.
- Closure goldens with CVE-resolving backports regenerate. That is all five Haskell targets: each has 3–6 such components.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, statically linked | Pass | No new crates or subprocesses. |
| IV Type-driven correctness | Pass | The qualifier is built by the `Purl` type. The drv hash is taken from the parsed derivation key, never assembled from strings. No new `.unwrap()`. |
| V Native first | Pass | The identity rides in the PURL's own qualifier slot. The key choice checked the PURL spec first: `checksum` was rejected on evidence (R1). The claim rides in native OpenVEX `fixed` and CycloneDX `vulnerabilities[]` `resolved`. The stronger `resolved_with_pedigree` was rejected because the pedigree holds no diffs (R5). Only C220 is new, with C193's KEEP-NO-NATIVE audit. |
| IX Accuracy | Pass | A statement names only the build it describes, and the leak probe measured 0. The version-level claim isn't restated as VEX. Ambiguous builds (several derivations) get no statement rather than an over-broad one (R4). |
| X Transparency | Pass | Each statement keeps m1035's evidence grade note. C220 counts what couldn't be scoped, and C193 what projection dropped. |
| XII External sources | Pass | No external source is added. The claim comes from the closure's own patches, as in m1035. |

No violations. **Re-check after Phase 1:** still passes.

## Project Structure

### Documentation (this feature)

```text
specs/1436-vex-build-identity/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/backport-vex.md
├── measurements/          # 2026-11-10 scanner shapes (README.md) + 2026-11-12 R2/R3 probes
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs        # qualify CVE-resolving components with nix_drv (one patching derivation), before edges are projected
waybill-cli/src/scan_fs/package_db/nix/closure/patches.rs     # keep which derivation carried each resolves-bearing patch (R4)
waybill-cli/src/generate/openvex/mod.rs                       # backport_statements: one `fixed` per (qualified PURL, CVE), no root, no `affected` half
waybill-cli/src/generate/cyclonedx/vex.rs                     # backport entries: affects = component bom-ref, `resolved`; no vex-subcomponent for backports
waybill-cli/src/generate/{cyclonedx/metadata,spdx/annotations,spdx/v3_annotations}.rs  # C220
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs    # C220
docs/reference/sbom-format-mapping.md                         # F1 (backport shape), C220
docs/reference/nix-evaluation.md                              # "Where the VEX appears": the new shape and the qualifier
waybill-cli/tests/nix_closure_backport_vex.rs                 # new
waybill-cli/tests/fixtures/nix_closure_backport_vex/          # new, synthetic names (waybill-fixture-*), synthetic CVE ids where the fixture rule requires
```

Existing tests that pin the m1035 pair (OpenVEX and CycloneDX VEX unit tests, m1068's `vulnerabilities[]` tests) are rewritten to the new shape. Each rewrite is shown to fail against the old shape (mutation), the same practice as #1478.

## Phasing

1. **Identity.** Record the patching derivation. Apply `nix_drv` to single-derivation CVE-resolving components before edges. Run the orphan check.
2. **US1 (OpenVEX).** `fixed` per (qualified PURL, CVE) with no root; remove the `affected` half; leave m1050 untouched.
3. **US2 (CycloneDX).** Single `resolved` entry per pair on the component's bom-ref; no `exploitable`; no subcomponent property.
4. **US3 (records and docs).** C220 in all three formats with extractors; F1, the Nix reference and the user guide.
5. **Tests.** Rewrite the m1035/m1068 pins with a mutation check each; add the new suite.
6. **Scanner gate and corpus.** Regenerate the goldens. Run `fixed-qualified-probe.sh` with the emitted sidecar on them, and rebuild and probe the six wrapper targets. Record SC-001–SC-005.
7. **Polish.** CHANGELOG, the gate, and the close-out audit.

## Complexity Tracking

No constitution violations to justify.
