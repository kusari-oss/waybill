# Implementation Plan: Haskell pin annotations say what is true

**Branch**: `1666-haskell-pin-annotations` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)
**Input**: specs/1666-haskell-pin-annotations/spec.md (#1682, #1741)

## Summary

- **C172's note.** It now names the source that produced the version: the Stack lock file or the freeze file, with its path.
- **Main modules.** They carry no C172.
- **After the twin merge.** C172 is recomputed: removed when the built version equals nixpkgs' version (C221 already records the range violation), and rewritten when the closure built a different version.
- **C174.** It counts only the C172 annotations that remain.
- **#1741.** An unreachable `hackage-freeze` pin gets `edgeless-lock-pin`, as `hackage-stack-lock` does.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `serde_json`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. Everything is in process for one scan.
**Testing**: cargo-nextest. Three-format `waybill-fixture-*` integration tests, recorded mutations (`measurements/mutate.py`), and corpus goldens from the `public-corpus.yml` dispatch.
**Target Platform**: all three CI lanes.
**Project Type**: CLI (single crate change, `waybill-cli`).
**Performance Goals**: no measurable change. Each change is a loop over components that already exists.
**Constraints**: C172's keys are unchanged (R8). The corpus measurement needs CI's nixpkgs cache (R1).
**Scale/Scope**: 4 code sites:
- `nix/haskell_packages/mod.rs`, the disagreement loop;
- `nix/haskell_packages/twin_merge.rs`, `fold` and `merge`;
- `cli/scan_cmd.rs`, the C174 adjustment;
- `generate/orphan_reason.rs`, the freeze arm.

Plus two rows in `docs/reference/sbom-format-mapping.md`: C172 and C45.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | Pass: no dependencies. |
| III. Fail closed | Pass: an annotation that contradicts its component is removed or corrected, not hidden. |
| IV. Type-driven correctness | Pass: the version source is read from `source_type`. No new raw-string API crosses a module boundary. The note is built in one function. |
| V. Standards-native first | Pass: no new row. C172, C174 and C45 are existing KEEP-NO-NATIVE rows; only their text and values change. |
| X. Transparency | Pass: the annotations now describe what produced the version. |
| XII. Enrichment never overwrites local truth | Pass: which version wins is unchanged. |
| Pre-PR gate | `./scripts/gate.sh … prepr -- ./scripts/pre-pr.sh`, default features. |

No violations, so Complexity Tracking is empty.

## Project Structure

```text
specs/1666-haskell-pin-annotations/
├── spec.md  plan.md  research.md  data-model.md  quickstart.md
├── contracts/annotations.md
└── measurements/  (measure.sh, main/, branch/, mutate.py, mutations.md)

waybill-cli/src/scan_fs/package_db/nix/haskell_packages/mod.rs        # R2, R3
waybill-cli/src/scan_fs/package_db/nix/haskell_packages/twin_merge.rs # R4
waybill-cli/src/cli/scan_cmd.rs                                       # R5
waybill-cli/src/generate/orphan_reason.rs                             # R6
waybill-cli/tests/haskell_pin_annotations.rs                          # FR-009 control
docs/reference/sbom-format-mapping.md                                 # C172, C45 text
```

**Structure decision**: there is no new module. Each fix sits where the wrong value is written today.

## Phase 0 / 1 outputs

- `research.md`, R1–R8. It resolves the version source (R2), the main-module skip (R3), the merge recompute (R4, which refines spec FR-005), the count (R5), the freeze arm (R6), the I7 fixture (R7) and the value shape (R8).
- `data-model.md`: the C172 forms.
- `contracts/annotations.md`: C172, C174 and C45.
- `quickstart.md`.
- Agent context: I'll add the line to the `CLAUDE.md` Recent Changes section by hand at implement time. Running `update-agent-context.sh` would delete earlier lines.

## Post-design Constitution re-check

The table above is unchanged: R4 removes an annotation only when C221 already carries its fact.

## Complexity Tracking

None.
