# Implementation Plan: CMake fetches made through user-defined wrappers

**Branch**: `1539-cmake-fetch-wrappers` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1539-cmake-fetch-wrappers/spec.md`

## Summary

Issue #1518, a prerequisite of TheRock's cataloger (#1539).

**The defect.** The CMake reader matches only `FetchContent_Declare` and `ExternalProject_Add`. TheRock fetches all of its third-party source tarballs through its own `therock_subproject_fetch`. The census (CI, SC-001) counts:
- 40 calls, all with a SHA-256, over 39 distinct tarballs;
- 0 of them visible on `main`.

The wrapper's body is read once, as a declaration named `${target_name}`, and counted unresolved. #1518's proposed `${ARGN}` detection would catch none of the 40: the URL arrives through `${ARG_UNPARSED_ARGUMENTS}` after `cmake_parse_arguments(PARSE_ARGV 1 …)`.

**The fix, in four parts:**
1. **Definitions** (R1, R2). The pre-pass that already tokenises every walked CMake file also records `function()`/`macro()` bodies. A body is a wrapper when its single fetch or wrapper call:
   - has no identity of its own (so spec 1538's C-13b body stays as it is);
   - receives the caller's arguments by one of five shapes: name parameter, `${<P>_UNPARSED_ARGUMENTS}`, a parsed keyword re-passed by name, `${ARGN}` or `${ARGV}`.

   Wrappers of wrappers are found to a fixed point.
2. **Binding** (R3–R5). Each call site is bound as CMake 4.4.5 was measured to bind it:
   - positional parameters, then `cmake_parse_arguments`, where a multi-value keyword swallows a following `URL` (W1) and a one-value keyword's last occurrence wins (W4);
   - then the body's literal defaults;
   - a repeated `URL` is a mirror list whose first entry is fetched (W2, W3).

   A wrapper defined more than once is bound against every definition, and only agreement emits.
3. **One declaration type** (R7, R8). A bound call becomes the same `Declaration` a direct fetch is, with the call site's file, conditions and directory. So spec 1538's shapes, first-declaration-wins, one-per-PURL, R9 merge and ownership apply by construction.
4. **Naming, evidence and accounting** (R6, R9, R10).
   - A wrapped URL fetch is named from its archive filename when that parses as `<name>-<version>` (36 of TheRock's 39 tarballs).
   - The new C234 `waybill:cmake-fetch-wrapper` records the wrapper chain.
   - C231 gains a `wrappers` object outside V1, and the body calls of wrappers stop counting as declarations.

**No operator flag** (R11). Every TheRock call with an identity binds statically. The 8 `download_project` calls the census found cannot be bound by any wrapper rule, and are proposed as a follow-up.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: Existing only. `regex`, `serde_json`, `tracing`, the existing tokeniser `installed_lib::cmake_version::{commands_checked, strip_comments}`, and spec 1538's `cmake/{blocks,fallback,fetches}.rs`. **Zero new Cargo dependencies** (FR-013).

**Storage**: N/A. Everything is in process for one scan.

**Testing**:
- **Unit** (`cmake/wrappers.rs`):
  - shape detection for each of the five shapes, plus the not-a-wrapper cases (literal-only body, own identity, two fetch calls);
  - the `cmake_parse_arguments` binder for both forms: options, one-value last-wins, multi-value consumption (W1), unparsed order;
  - defaults in both forms;
  - `ARGV<n>`;
  - the fixed point, recursion and the depth cap;
  - FR-005's filename rule over every census filename shape.
- **Integration:** a new `waybill-cli/tests/cmake_fetch_wrappers.rs` with contract cases W-1 to W-22 (`contracts/wire.md`), on synthetic `waybill-fixture-*` trees, checked in all three formats. Spec 1538's `cmake_nested_fetches.rs` keeps passing, with C-13's expectation updated (R9).
- **Parity:** extractors for C234 in CycloneDX, SPDX 2.3 and SPDX 3. The catalog gate is in the lib target, so the doc row and the extractors land together.
- **Conformance:** the CycloneDX schema and `spdx3-validate`.
- **Mutations:** each shape, each binding rule, the ambiguity rule, the recursion guard, the naming rule and the accounting is mutated and must fail a test, using spec 1537/1538's `mutate.py` pattern.
- **CI:**
  - `census-fetches` at the branch, where SC-002 to SC-004 are checked against `measurements/census.md`'s counts and prediction;
  - interleaved `measure-source`, branch against `main` (SC-005);
  - the public corpus in compare mode;
  - the quality corpus (SC-006).

**Target Platform**: as today. The reader is pure Rust, with no subprocess.

**Project Type**: CLI.

**Performance Goals**: SC-005. The median full-tree TheRock scan is at most 1.5× `main`'s, in the same interleaved run: a warm-up, then three alternating rounds.

**Constraints**:
- Deterministic.
- Static: no CMake is run, and nothing is evaluated beyond the measured binding rules and same-file literal `set()`s.
- No network.
- Fixtures use synthetic names.
- TheRock is measured only in CI.

**Scale/Scope**: TheRock has 6,172 CMake files, 7 wrappers and 53 top-level call sites. 40 of the calls carry an identity, and 13 are LLVM in-tree builds.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates, and no subprocess. |
| III. Fail Closed | Pass. Anything the binder cannot decide is unresolved and counted, never guessed: an unknown variable in the identity, recursion, depth, disagreeing definitions, a loop-variable name. A `URL` swallowed by a multi-value keyword yields no component, as in CMake (W1). |
| IV. Type-Driven Correctness | Pass. `ForwardingShape`, `BodyTarget`, `Binding` and `UnboundReason` are closed enums. The binder's output is the same `Declaration` type direct calls produce, so no second path re-implements spec 1538's rules. |
| V. Specification Compliance | Pass. One new row, C234, with a Principle V audit (R10): CycloneDX `evidence.identity.methods` names a technique, not a build command, and SPDX has no carrier. C231 gains a key inside its existing object. Scope and conditions reuse C122, C232 and the native scope fields. |
| VII. Test Isolation | Pass. Synthetic fixtures and a committed CMake probe. TheRock is measured in CI only; four call-site files were read singly through the GitHub API. |
| VIII. Completeness | Pass. 40 of 40 identity-carrying call sites on TheRock are bound. The 13 in-tree calls become counted, not invisible. |
| IX. Accuracy | Pass. Every binding rule is taken from measured CMake behaviour, including the two where a naive reader is wrong: W1's multi-value swallowing, and W2's first-URL-wins. Names come from the archive only when its version parses (R6). |
| X. Transparency | Pass. C234 says which wrapper produced a component, and C231's `wrappers` object accounts for every definition, call and ambiguity. The `download_project` gap is published with its count (R11). |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1539-cmake-fetch-wrappers/
├── spec.md
├── plan.md                  # this file
├── research.md              # R1–R14
├── data-model.md
├── quickstart.md
├── contracts/wire.md        # W-1…W-22, I-1…I-7
├── measurements/
│   ├── census.md            # SC-001: TheRock's wrappers and call sites, main baseline, prediction
│   ├── census-wrappers.json, census-wrappers.py, census-refs.txt
│   └── cmake-semantics/     # probe.sh, results.txt, cmake-version.txt (W1–W8)
├── checklists/requirements.md
└── tasks.md                 # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── cmake.rs                  # FR-005 archive-name helper beside parse_version_from_url
└── cmake/
    ├── wrappers.rs           # new: Definitions, ForwardingShape, cmake_parse_arguments binder, bind(), fixed point
    ├── fetches.rs            # pre-pass builds Definitions; main pass binds call sites, skips templates; C231 `wrappers`; C234 through one_per_purl
    └── blocks.rs             # function bodies captured for the pre-pass (body commands, inner if-conditions)
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs   # C234
waybill-cli/tests/cmake_fetch_wrappers.rs                    # new: W-1…W-22
waybill-cli/tests/cmake_nested_fetches.rs                    # C-13 expectation (R9)
docs/reference/sbom-format-mapping.md                        # C234 row; C231 `wrappers` keys
docs/ecosystems.md                                           # CMake section: wrappers
CHANGELOG.md
```

**Structure Decision**: a new `cmake/wrappers.rs` beside spec 1538's modules. It holds everything specific to wrappers (definitions, shapes, the binder), and is unit-testable without files. `fetches.rs` changes only where it reads commands. It gains a call-site branch that produces the `Declaration` it already handles, so every rule after that point is shared.

## Complexity Tracking

None.
