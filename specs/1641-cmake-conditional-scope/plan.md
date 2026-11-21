# Implementation Plan: A CMake condition that holds by default does not make a fetch optional (#1639)

**Branch**: `1641-cmake-conditional-scope` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

## Summary

Spec 1538's catch-all `cmake-conditional` makes a fetch Optional under any `if()`. That includes platform checks, the super-build guard and default-ON options, so the libraries a tree ships are excluded. Spec 1614's package searches share the rule.

**The approach** (research R1–R8):
- **An evaluator** (`cmake/conditions.rs`) reads each enclosing condition with CMake's `if()` grammar and decides whether the whole chain holds by default, does not, or is unknown.
  - A platform predicate holds, and so does its negation.
  - The super-build guard holds.
  - Options take their declared default from files in reach.
- **Branch history** is kept beside today's C232 strings (R3), so `elseif` and `else` are evaluated correctly and C232 does not change.
- **Wiring** (R5):
  - only the `Conditional` shape is re-evaluated;
  - a chain that does not hold stays Optional, and holds or unknown is in scope;
  - C232 is written in scope too;
  - R9 keys on C232, not scope;
  - C231's `conditional` keeps its definition;
  - searches use the same evaluation.
- **A census first** (R6): a log-only commit and a CI workflow count every catch-all conditional by category and verdict on TheRock and the quality corpus before the rule is wired.

**One planning amendment** (research R4): FR-002's "before the fetch" is dropped. A definition's position within a file is not tracked, because an option is a cache variable visible for the rest of the configure run. The census counts any option defined only after its use.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- Code:
  - `scan_fs/package_db/cmake/{blocks,reach,fallback,fetches,searches}.rs`;
  - `cmake.rs::collect_set_vars`;
  - the tokeniser `installed_lib::cmake_version::{commands_checked, strip_comments}` via `reach::parse_commands`.
- Crates: `regex`, `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-011).

**Storage**: N/A.

**Testing**:
- **Unit:** `cmake/conditions.rs` (R7).
- **Integration:** `waybill-cli/tests/cmake_conditional_scope.rs`, with cases in `contracts/wire.md`. Each case checks parity of C122, C232, C236 and scope across the three formats, CycloneDX validity and unique `bom-ref`s, and repeats 10 scans byte-identical. C-ctl has a golden from `main`'s binary.
- **Control:** the cases land at the log-only commit, and every changed case must fail there.
- **Mutations:** one per evaluation rule (SC-006), plus:
  - R9's predicate reverted to scope;
  - C232 written only with a scope;
  - searches left on the old rule.

  Restore with `cp`, rebuild after the last one, and use a `^error\[E…\]` guard.
- **CI:**
  - the census (`census-conditions.yml`, fixture repository) at the log-only commit;
  - the public corpus, `regen_goldens`, twice: at the log-only commit (0 differing expected) and at the final commit;
  - the quality corpus against `main`;
  - TheRock `measure-source` against `main`.

**Target Platform**: the waybill CLI on Linux, macOS and Windows.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-005, at most 1.05× `main` on TheRock's source tree, median of at least 3.

**Constraints**:
- FR-009: byte-identical output when no catch-all conditional exists;
- C232/C236 strings unchanged;
- the six fallback shapes, `BUILD_TESTING` and R9's merges unchanged.

**Scale/Scope**: one new module of about 400 lines with tests, plus edits in five CMake files.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| IV. Type-driven correctness | Pass. The evaluation is an enum, never a string. No `.unwrap()` in production. |
| V. Specification compliance | Pass. The scope stays native: CycloneDX `scope`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`. No new row: C232/C236 already record the conditions (spec FR-005). |
| VIII. Completeness / IX. Accuracy | This is the point of the change. Shipped libraries stop being reported as not shipping. Unknown conditions err toward completeness (spec 1269). |
| X. Transparency | Pass. C232/C236 stay on every conditional component, in scope or not, so a consumer can see what gates it. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1641-cmake-conditional-scope/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
├── measurements/          (census, control, mutations, gate, corpus, therock)
└── tasks.md               (/speckit-tasks)
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/cmake/conditions.rs   # new: parse + evaluate, default collection
waybill-cli/src/scan_fs/package_db/cmake/blocks.rs       # branch history (R3)
waybill-cli/src/scan_fs/package_db/cmake/reach.rs        # seed history; scope_files (R4)
waybill-cli/src/scan_fs/package_db/cmake/fetches.rs      # re-evaluate Conditional; C232; R9 predicate
waybill-cli/src/scan_fs/package_db/cmake/searches.rs     # per-search evaluation (R5)
waybill-cli/tests/cmake_conditional_scope.rs             # integration cases
docs/reference/sbom-format-mapping.md, docs/reference/reading-a-mikebom-sbom.md, CHANGELOG.md
```

Fixture repository (`kusari-sandbox/waybill-fixture-therock-sdk`): `census/conditions.py`, `.github/workflows/census-conditions.yml`.

## Complexity Tracking

None.
