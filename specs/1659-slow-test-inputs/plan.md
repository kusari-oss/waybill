# Implementation Plan: Cheaper inputs for the two costliest tests

**Branch**: `1659-slow-test-inputs` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)

## Summary

- **#1725:** i1454 drops its `spdx3-validate` call on the moat output. The same check is already made on a 40 KB document by `closure_runtime_scope::us4`, which edits the same way, including the OpenVEX reference removal (research R1). `us4` gains explicit assertions that the reference is present before the edit and gone after.
- **#1716:** four self-scan tests become one test, `self_scan_guards::waybill_self_scan_guards`, that scans the debug binary once and evaluates all four guards, reporting every failure by name.

## Technical Context

**Toolchain:**
- **Language:** Rust stable, pinned 1.98.1. No `+toolchain`.
- **Dependencies:** none new.

**Changes:**
- **Tests:** test code only. No production change.
- **Platforms:** all three CI lanes. The provenance guard keeps its ELF, Mach-O and PE branches.

**Measurement:** report-only, with targets SC-001 (12 s or less for i1454 plus the conformance check locally) and SC-002 (one scan instead of four).

## Constitution Check

| Rule | Status |
|---|---|
| VII. Test Isolation | Pass. Tests stay unprivileged and offline-capable. |
| IX. Accuracy, and the test-coverage practice ("no test may lose coverage") | Pass, with mutations FR-007 / R2 / R4 required before claiming it. |
| Pre-PR gate (4.1.0) | Unchanged. |
| Zero new Cargo dependencies | Pass. |

## Project Structure

```text
waybill-cli/tests/
├── self_scan_guards.rs        # new: one scan, four guards
├── cpe_binary_id.rs           # remove mikebom_self_scan_emits_no_spurious_openssl_cpe
├── binary_id_enrich.rs        # remove the two mikebom_itself_* tests
├── binary_build_provenance.rs # remove mikebom_self_scan_emits_build_provenance_properties
├── sbom_edit_filter.rs        # i1454: drop the moat spdx3-validate call
└── closure_runtime_scope.rs   # us4: assert the OpenVEX reference is present before and gone after
specs/1659-slow-test-inputs/measurements/
```

## Phases

1. **Baseline:** time the 5 affected tests locally, 3 runs each, alone.
2. **#1725:** add the `us4` assertions, check the R2 mutation, then remove the i1454 validator call.
3. **#1716:** write `self_scan_guards.rs`, check the R4 mutations, then remove the 4 old tests.
4. **After:** take local timings, run the gate, open the PR, and collect 2 CI runs.

## Complexity Tracking

None.
