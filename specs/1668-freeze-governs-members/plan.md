# Implementation Plan: A cabal.project.freeze governs the packages its cabal.project lists

**Branch**: `1668-freeze-governs-members` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)
**Input**: specs/1668-freeze-governs-members/spec.md (#1744, refs #1741)

## Summary

The Haskell reader decides which package directories a lock file governs in two places:
- the freeze-file loop (Phase B) uses only the freeze file's own directory;
- the `stack.yaml.lock` loop (Phase C) uses its own directory plus the `packages:` of its sibling `stack.yaml`.

One function, `lock_governed_dirs`, replaces both copies (R2). For a freeze file, its membership source is the sibling `cabal.project`. That file is read by a per-entry expansion taken out of `resolve_root_declared_members`, so `packages:` keeps a single reader (R3). Pin lookup (Phase G, keyed by the canonical directory) is unchanged. A member under `app/` now finds the root freeze file's pins.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `globset` (already used for `cabal.project` globs), `serde_yaml`, `tracing`. **Zero new Cargo dependencies.**
**Storage**: N/A. Everything is in process for one scan.
**Testing**:
- cargo-nextest;
- three-format `waybill-fixture-*` integration tests with `common::parity::assert_parity`;
- recorded mutations (`measurements/mutate.py`);
- corpus goldens from the `public-corpus.yml` dispatch.

**Target Platform**: all three CI lanes. Matching is on `/`-joined relative keys, the same as today's membership code.
**Project Type**: CLI, one crate (`waybill-cli`).
**Performance Goals**: no measurable change. One extra read of `cabal.project` per freeze file, and only when one sits beside it.
**Constraints**:
- C45 and `waybill:source-files` keep their shapes. No catalog row changes (contracts/annotations.md).
- `resolve_root_declared_members` must return the same result for every existing test (FR-002).

**Scale/Scope**: one file, `waybill-cli/src/scan_fs/package_db/haskell.rs`:
- Phase B and Phase C call the new helper;
- the `packages:` entry expansion is taken out of `resolve_root_declared_members`;
- `package_dirs` (~line 875) moves out of its block so that Phase B can reuse it.

New test file: `waybill-cli/tests/haskell_freeze_members.rs`.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | Pass: no dependencies. |
| IV. Type-driven correctness | Pass: one function answers "which directories does this lock govern"; the two copies were the defect. |
| V. Specification compliance / standards-native first | Pass: no new annotation. A real pin becomes a real dependency edge. |
| VII. Test isolation | Pass: tempdir fixtures with `waybill-fixture-*` names. |
| VIII. Completeness / IX. Accuracy | Pass: removes a false `edgeless-lock-pin` and a versionless twin. |
| Pre-PR gate | `./scripts/gate.sh … prepr -- ./scripts/pre-pr.sh`, default features. |

No violations, so Complexity Tracking is empty.

## Project Structure

```text
specs/1668-freeze-governs-members/
├── spec.md  plan.md  research.md  data-model.md  quickstart.md
├── contracts/annotations.md
└── measurements/           # T-measure: fixture.sh, main/, branch/, mutate.py
waybill-cli/src/scan_fs/package_db/haskell.rs
waybill-cli/tests/haskell_freeze_members.rs
```
