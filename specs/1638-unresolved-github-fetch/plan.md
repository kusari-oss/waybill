# Implementation Plan: Name a CMake git fetch with an unresolved reference after its repository

**Branch**: `1638-unresolved-github-fetch` | **Date**: 2026-11-20 | **Spec**: [spec.md](spec.md)

## Summary

#1595: a CMake git fetch whose `GIT_REPOSITORY` is a literal GitHub URL, but whose `GIT_TAG` is unresolved, is emitted as `pkg:generic/<declared name>`. It also has no repository identity, so spec 1538 R9 cannot merge it into the component that already supplies its repository.

**The approach** (research R1–R8):
- **Identity.** Split `fetch_entry`'s unresolved branch by what is unresolved.
  - A literal GitHub repository gives a versionless `pkg:github/<owner>/<repo>`, using the resolved branch's owner/repo rule and lowercasing.
  - A literal non-GitHub repository keeps `pkg:generic/<declared name>`, but records its URL.
  - Every other unresolved shape is unchanged.
- **Wrappers.** Keep spec 1539's wrapper recognition answering exactly as on `main` (R2). Otherwise the aotriton wrapper stops being one, and a wrapper that forwards a literal tag regresses.
- **Merging.** R9, R4, C233 and C231 already do the right thing once the identity exists (R4).
- **The reconciler** may need one guard so FR-014 holds. A probe decides, with its decision rule fixed in R5.

Two planning amendments refine the spec, recorded in its Clarifications:
- R2: wrapper recognition is unchanged;
- R3: an `ExternalProject_Add` non-GitHub fetch gets its URL but, like its resolved form, no repository identity.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- `cmake.rs`: `fetch_entry`, `build_cmake_entry`, `super::forge::lowercases_owner_and_name`.
- `cmake/fetches.rs`: `declared`, `read_fetches`, `repository_identity`, `merge_into_existing`, `superseded`.
- `cmake/wrappers.rs`: `Definitions::collect`.
- `resolve/reconciler.rs`.
- Crates: `regex`, `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-012).

**Storage**: N/A. In process per scan.

**Testing**:
- **Integration:** `waybill-cli/tests/cmake_unresolved_github_fetch.rs`, cases U-1..U-17 plus U-ctl, U-rep and U-xhost (the cross-host golden, SC-005) (`contracts/wire.md`), checked in all three formats through `waybill sbom parity-check`. U-ctl's golden comes from `main`'s binary before any source change.
- **Unit tests:**
  - `fetch_entry`: each unresolved shape;
  - the wrapper predicate's mask;
  - `repository_identity` on the new entries.
- **Mutations** (`cp` restore, rebuild after the last one, a `^error\[E…\]` guard so a compile failure is not counted as a kill):
  - M1 the GitHub branch;
  - M2 the literal-repository check;
  - M3 the non-GitHub `download_url`;
  - M4 the wrapper mask;
  - M5 the reconciler guard, if R5 adds one;
  - M6 `download_url` leaking to a URL fetch.
- **CI:**
  - census (T001) on TheRock and the three quality-corpus targets;
  - TheRock measure-source against `main`;
  - the public corpus (`regen_goldens`);
  - the quality corpus against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-006, ≤ 1.05× `main` on TheRock's source tree (median of ≥ 3).

**Constraints**:
- FR-010: byte-identical when no fetch takes the unresolved branch with a literal repository;
- FR-002: no version guessed;
- FR-014: required fetches are never joined.

**Scale/Scope**: counted by the census (SC-001, T001). The spec quotes no figure in advance.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies. |
| III. Fail closed | ✅ No version is taken from an unresolved variable's candidates (FR-002). An unresolved repository is never parsed for an owner (R1). Required versionless and versioned components are not merged on the assumption that they are one commit (FR-014, R5). |
| IV. Type-driven correctness | ✅ The unresolved shapes are distinguished explicitly in one place, and every other shape falls through to today's code unchanged. |
| V. Specification compliance | ✅ `pkg:github` without a version is valid purl-spec, lowercased per its `github` definition. No new row; C231, C233 and C234 rows are corrected to match behaviour (`contracts/wire.md`). |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs. TheRock and the quality corpus only in CI. |
| VIII. Completeness | ✅ FR-008 emits a fetch that was dropped. |
| IX. Accuracy | ✅ A repository is named as itself, not as a build target. A fallback is no longer a second, misnamed component. |
| X. Transparency | ✅ C151 still states why the version is missing. Merges are recorded in C233 with `"version": ""`. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1638-unresolved-github-fetch/
├── spec.md
├── plan.md               # this file
├── research.md           # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/wire.md     # component shapes, rows touched, cases U-1..U-17, U-ctl, U-rep, U-xhost
├── checklists/requirements.md
└── measurements/         # later: census.md, reconciler-probe.md, mutations.md, therock.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/cmake.rs            # fetch_entry: split the unresolved branch (R1)
waybill-cli/src/scan_fs/package_db/cmake/fetches.rs    # read_fetches: the wrapper-recognition mask (R2)
waybill-cli/src/resolve/reconciler.rs                  # guard, only if the R5 probe collapses
waybill-cli/tests/cmake_unresolved_github_fetch.rs     # U cases
waybill-cli/tests/fixtures/cmake_unresolved_github_fetch/  # U-ctl tree and golden
docs/reference/sbom-format-mapping.md                  # C231, C233, C234 wording
docs/ecosystems.md                                     # CMake fetch declarations: identity bullet
CHANGELOG.md
```

**Structure decision.**
- **Identity** changes where it is built, `fetch_entry`, so every route gets it at once (FR-003).
- **Wrapper recognition** changes only at its one call site in `read_fetches`; `wrappers.rs` is untouched.
- **No change to `merge_into_existing`, `one_per_purl` or `superseded`** (R4).
