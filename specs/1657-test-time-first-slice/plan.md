# Implementation Plan: cut serial test time, first slice

**Branch**: `1657-test-time-first-slice` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)

## Summary

Three independent changes, each verified on its own:
- **One-pass SPDX 3 annotation index** (R1). This is a product fix, and the output must be byte-identical.
- **Harness self-tests in one binary**, and `dual_format_perf` once (R2).
- **Offline scan helpers**, with a hand-reviewed list and a measured enforcement point (R3).

## Technical Context

**Language/Version**: Rust stable 1.98.1 (pinned); no `+toolchain`. **Dependencies**: none new.
**Testing**:
- the gate;
- `--list` counts;
- byte-identity of edits;
- per-binary timing (R4);
- a mutation for any test whose input changes.

**Constraints**: no golden changes, one gate, one PR.

## Constitution Check

| Principle | Status |
|---|---|
| I (no new deps) | Pass |
| VII (test isolation: tests run without network by default) | Improved by #1715 |
| IX (accuracy: edit output byte-identical) | Pass, by FR-002 |
| Measure first: every figure traces to `measurements/` | Pass |

No violations.

## Project Structure

```text
waybill-cli/src/edit/spdx3.rs                          # R1 index
waybill-cli/tests/corpus_harness_195/{js_filter,cache,expectations,layer2_golden}.rs  # test modules out
waybill-cli/tests/corpus_harness_self_tests.rs         # new: the 145 tests
waybill-cli/tests/{holistic_parity,dual_format_perf}.rs, tests/common/  # R2 helpers
waybill-cli/tests/common/{nix_stub,installed_lib,cmake_fetch}.rs, tests/scan_split_basic.rs, + reviewed files  # R3
specs/1657-test-time-first-slice/measurements/          # lists, scripts, before/after, offline-review.md
```

## Phases

- Phase 0: research done.
- Phase 1: design (data model: none; contract: the edit output is unchanged).
- Phase 2: `/speckit-tasks`.
