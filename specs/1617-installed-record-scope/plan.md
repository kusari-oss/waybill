# Implementation Plan: A package search's conditions do not mark a shipped library non-runtime

**Branch**: `1617-installed-record-scope` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

## Summary

Spec 1614's search half of the installed-record scope rule makes a record Optional when every search of it is non-runtime. On the TheRock SDK that marks 7 shipped libraries non-runtime, the core runtime `hsa-runtime64` among them. It also makes a record searched only from test contexts Optional with no stated reason.

**Fix.** In `record_scopes`:
- **R3:** a record whose own evidence lists a library file it ships is never scoped by the search half. Its C236 is still written, and the link half (C235) is untouched.
- **R4:** a non-shipping record searched only from test contexts becomes Test, with its searches' C62 derivation.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- `scan_fs/cmake_search_pair.rs`: `Pairing`, `pair`, `record_scopes`, `RecordScopes`, `apply_scope`;
- `scan_fs/mod.rs`, the call site;
- the existing search scope and C62 annotations (`cmake/searches.rs`).

**Zero new Cargo dependencies** (FR-008).

**Storage**: N/A. In process per scan.

**Testing**:
- **Unit tests** in `cmake_search_pair.rs` for `record_scopes`:
  - a shipping record joins neither set and goes to `conditions_only`;
  - a non-shipping all-Test record goes to `test`, with the right derivation;
  - a mixed record stays `optional`;
  - unconditional searches, links, existing scopes and incoming edges behave as today;
  - the library-file name test.
- **Integration:** `waybill-cli/tests/installed_record_scope.rs`, cases I-1..I-11, I-ctl and I-rep (`contracts/wire.md`), in three formats through the C42, C62, C122, C235 and C236 extractors. I-ctl's golden comes from `main`'s binary before any source change.
- **Mutations:**
  - the shipping check removed;
  - a shipping record put in `required` (switches off the link half);
  - C236 dropped from shipping records;
  - Test records made Optional;
  - the derivation precedence reversed;
  - `.a` not counted as a library file.
- **CI:**
  - the SDK: measure-full against `main`, with all 9 records checked and every other change traced;
  - TheRock source: measure-source, unchanged;
  - the public corpus: regeneration;
  - the quality corpus: against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows. File names are matched case-insensitively.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-006, ≤ 1.05× `main` on the SDK.

**Constraints**:
- FR-005: the link half is unchanged;
- FR-006: C235, C236 and C237 values are unchanged;
- FR-007: byte-identical output where no installed record is searched only conditionally.

**Scale/Scope**: the SDK has 313 installed records. 9 are non-runtime today; 8 are predicted to change (research R6).

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies. |
| III. Fail closed | ✅ The rule only stops lowering a scope on evidence the record itself carries: a shipped library file. It never invents one. |
| IV. Type-driven correctness | ✅ New typed sets in `RecordScopes`. The scope comes from `LifecycleScope`, and the derivations from the existing constants. |
| V. Specification compliance | ✅ Native scope fields carry the change. C122, C62 and C236 keep their value sets; no new row. |
| VII. Test isolation | ✅ Synthetic installed prefixes in temp dirs; TheRock only in CI. |
| VIII. Completeness | ✅ No component added or removed. |
| IX. Accuracy | ✅ Shipped libraries stop being reported as optional, and test-only records say test. |
| X. Transparency | ✅ C236 keeps recording the conditions under which a shipped record was searched. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1617-installed-record-scope/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/wire.md  # I-1..I-11, I-ctl, I-rep
├── checklists/requirements.md
└── measurements/      # later: trace.md, mutations.md, sdk.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/cmake_search_pair.rs     # Pairing scope fields; record_scopes; apply for test / conditions_only
waybill-cli/src/scan_fs/mod.rs                   # apply the new sets at the existing call site
waybill-cli/tests/installed_record_scope.rs      # I cases
waybill-cli/tests/fixtures/installed_record_scope/  # I-ctl tree and golden
docs/reference/sbom-format-mapping.md            # C236 (may sit on a required record); C122 cmake-conditional (not for shipped records)
docs/ecosystems.md                               # CMake: how an installed record's scope is decided
CHANGELOG.md
```

**Structure decision.** All logic stays in `cmake_search_pair.rs`, beside the rule it changes. The call site in `scan_fs/mod.rs` only applies the two new sets.
