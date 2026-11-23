# Control: spec 1646 at the log-only commit (T001–T008)

## Binaries

- **`main`**: `e114881803f149c8ef58884a63b21428464c3690`, a debug build saved as the scratchpad's `waybill-main-e1148818` (`waybill 0.17.0`). The branch's code was identical when it was built: `git diff e1148818 -- waybill-cli waybill-common Cargo.toml Cargo.lock` gave 0 lines.
- **Branch**: the log-only commit. It adds:
  - `ResolutionTechnique::BinaryAnalysis`, `IdentityMethod` and `ResolutionEvidence.also`;
  - `absorb_methods` at S1 and S2 behind `APPLY_ABSORBED = false`;
  - the shared conversion helpers `entry_technique` (still `PackageDatabase`) and `census_entry_technique`;
  - the emitters' handling of `also`, inert because `also` is always empty;
  - three census events.

  Nothing in the output changes.

## T003: the literals

`also: Vec::new()` was added to every `ResolutionEvidence { … }` literal. The compiler reported them, and a brace-matching script inserted 69. It also matched once on `-> ResolutionEvidence {`, a function's return type followed by its body, in `generate/cyclonedx/evidence.rs::tests::make_evidence`. That one was fixed by hand, and it is the only such signature in the tree. The three technique `match`es gained the new arm: CycloneDX through one `cdx_technique` helper, SPDX through `ResolutionTechnique::kebab_name`.

## T005: conversion sites

Every `technique: ResolutionTechnique::PackageDatabase` in `scan_fs/`:

| Site | Receives | Spec 1646 |
|---|---|---|
| `scan_fs/mod.rs` `scan_path` (~1249) | every scanned `PackageDbEntry`, binary matches included | `entry_technique(entry)` and the census |
| `package_db/nix/closure/contents.rs::component_from_entry` | closure binary contents: Go build info and `cargo-auditable` only (spec 1378) | the same helper, so a future binary match cannot take another path |
| `file_tier/dedupe.rs`, `file_tier/walker.rs` | file-tier components, not binary matches | unchanged |
| `package_db/nix/mod.rs` (~604) | a unit-test helper (`rc`) | unchanged |
| `scan_fs/mod.rs` ~3542, ~3664, ~3976 | unit tests | unchanged |

## Every case, both binaries (T007)

`cargo test -p waybill --test merged_binary_evidence -- --include-ignored`. For the `main` column, `WAYBILL_MERGED_EVIDENCE_BIN` points at `main`'s binary.

Totals: `main` 1 passed, 6 failed. Branch: 2 passed, 5 failed, exactly the planned red cases.

| Case | `main` | log-only branch | Meaning |
|---|---|---|---|
| b1_banner_and_pc | FAILED (`[("manifest-analysis", 0.85)]`) | FAILED (same) | red until T011 |
| b2_banner_and_cmake | FAILED (same) | FAILED (same) | red until T011 |
| f2_fingerprint_banner_and_pc | FAILED (same) | FAILED (same) | red until T011 |
| b3_banner_alone | FAILED (same) | FAILED (same) | red until T010 |
| f1_fingerprint_alone | FAILED (same) | FAILED (same) | red until T010 |
| c1_census_reports_the_merge | FAILED (`left: 0`, no census) | ok | positive control: one `binary_merge` (S1, winner `pkg-config`, loser `embedded-version-string`, absorbed `["binary-analysis"]`) and one `technique_change` |
| r1_record_into_record | ok | ok | guard; teeth from M6 |

Every scan validates its CycloneDX against the 1.6 schema and runs `parity-check` before any assertion, so P-1 and S-1 held in all 7 scans on the branch.

F-1's fingerprint fixture is recognised as exactly one `zlib` component. The case reached its technique assertion, so a fixture that failed to match would have failed earlier and visibly.

## Unit tests at this commit

`cargo test -p waybill --bins s1646`: 8 passed.
- `deduplicator`:
  - a banner into a record absorbs `binary-analysis`, in the log-only state and in the wired one;
  - a fingerprint does too;
  - record into record absorbs nothing;
  - a binary winner absorbs the record;
  - one method per technique, and chains carry;
  - variant order;
  - an empty `also` is not serialized.
- `cyclonedx::evidence`: methods list the absorbed technique.
- `spdx::annotations`: D1 gets `methods` only when there are two.

## Gate at the log-only commit

`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`:
- `GATE_EXIT=0`, `GATE_RESULT=pass`, and the expected line present;
- 429 `test result: ok.` lines, 0 `FAILED`, and no failure block;
- 11,041 passed, 0 failed, 52 ignored. The 52 are spec 1645's final 47 plus this spec's 5 planned red cases.

## Fixture-repository control, run locally before dispatch

The extended `census/anchored_control.py` identity tree, which now includes a fingerprint-only zlib, was scanned with this commit's debug build, and both workflow assertions were run on the result:
- `identity control ok: [('ncurses', 'merge')] [] []`: spec 1645's wired state;
- `spec 1646 control ok: log-only`. The census reports 2 merges with a record (`llvm@23.0.0git` from `cmake-package-version`, `ncurses@6.6.20251230` from `pkg-config`, each absorbing `["binary-analysis"]`), and `technique_change` for 3 PURLs: llvm, ncurses, and the fingerprint-only `pkg:generic/zlib`.
