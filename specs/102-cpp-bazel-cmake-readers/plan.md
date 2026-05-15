# Implementation Plan: C/C++ source-tree readers (Bazel + CMake)

**Branch**: `102-cpp-bazel-cmake-readers` | **Date**: 2026-05-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/102-cpp-bazel-cmake-readers/spec.md`

## Summary

Add 4 new source-tree manifest readers under `mikebom-cli/src/scan_fs/package_db/` — `bazel.rs`, `cmake.rs`, `vcpkg.rs`, `conan.rs` — following the existing cargo.rs / gem.rs / maven.rs template. Each parses its respective manifest format (Bazel `MODULE.bazel` + `WORKSPACE.bazel` / CMake `CMakeLists.txt` + `cmake/*.cmake` / `vcpkg.json` / `conanfile.txt` + `conanfile.py`) and emits `PackageDbEntry` records that flow through the existing deduplicator + emitters into CDX 1.6 / SPDX 2.3 / SPDX 3 components. Adds one new CLI flag (`--include-vendored`) for opt-in CMake `add_subdirectory(third_party/...)` emission, with operator docs in `docs/user-guide/cli-reference.md`. Test-driven implementation against synthetic Rust-test fixtures (one per ecosystem) reusing the milestone-090 fixture-cache pattern.

Zero new Cargo dependencies (regex + toml + serde_json already in tree). Zero changes to existing reader code paths — purely additive. The 4 readers are independent (file-level isolation), so they can land in any order; US1 (Bazel) + US2 (CMake) are both P1 and naturally ship together.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–101; no nightly required).
**Primary Dependencies**: Existing only — `regex = "1"` (CMakeLists.txt pattern extraction; already a direct dep per milestone 013), `toml = "0.8"` (conanfile.txt INI-shaped parsing; already direct dep), `serde_json` (vcpkg.json parsing; workspace), `tracing` (parse-error warnings per FR-015), `anyhow`/`thiserror` (error propagation). **No new crates.** No subprocess calls. No network access.
**Storage**: N/A — all parsing is in-process per scan; results flow through the existing `PackageDbEntry` → `ResolvedComponent` pipeline.
**Testing**: Standard `cargo test` integration tests per reader (`tests/scan_bazel.rs`, `tests/scan_cmake.rs`, `tests/scan_vcpkg.rs`, `tests/scan_conan.rs`), each with a synthetic fixture under `mikebom-cli/tests/fixtures/{bazel,cmake,vcpkg,conan}/` validated against expected component sets. Goldens regression suite extended by 4 ecosystems on each format (CDX/SPDX 2.3/SPDX 3 = 12 new goldens total) to lock byte-identity.
**Target Platform**: Cross-platform (no `#[cfg(unix)]` gates per FR-013) — all 4 readers parse text/JSON/INI manifests via std + workspace crates. Verified Windows-compatible since milestone 100 (path normalization at SBOM-emission chokepoint already in place).
**Project Type**: Single-crate workspace (`mikebom-cli`) extension. Net addition: 4 reader modules + 4 integration-test files + 4 fixture directories + 12 goldens + 1 CLI flag + docs.
**Performance Goals**: <100ms per manifest-file parse on average hardware (Bazel WORKSPACE files can hit 5MB+ on the long tail; CMakeLists.txt rarely exceeds 100KB). Aggregate scan-time impact <500ms on a typical C/C++ source tree.
**Constraints**: Diff scope per SC-008 — ≤8 NEW source files (4 readers + 4 test files) + ≤2 modified production files (`path_resolver.rs` + `scan_cmd.rs` + a tiny `mod.rs` for the new readers) + docs (`README.md` + `docs/user-guide/cli-reference.md`) + 12 NEW goldens. Zero new Cargo deps.
**Scale/Scope**: Roughly 4 × 300-line reader files + 4 × 200-line integration tests = ~2000 LOC net production+test code, plus 12 generated goldens (~30 KB).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution version: **1.4.0**.

| Principle | Compliance | Notes |
|-----------|------------|-------|
| **I. Pure Rust, Zero C** | ✅ PASS | All parsing in pure Rust via existing workspace deps (regex, toml, serde_json). No new crates; no C deps. |
| **II. eBPF-Only Observation** | ✅ PASS | This is source-tree manifest analysis = enrichment (per Principle II + XII), not dependency discovery from runtime. Cargo / npm / pip / gem / maven / golang readers are established precedent. |
| **III. Fail Closed** | ✅ PASS | The fail-closed contract applies to the eBPF trace, not per-file source-tree parsing (which uses skip-with-warn per FR-015 — same precedent as maven/golang). |
| **IV. Type-Driven Correctness** | ✅ PASS | New readers use `Purl` newtype via `Purl::new()` + `encode_purl_segment()`; `LifecycleScope` enum for scope semantics; `thiserror`/`anyhow` for error propagation. No `.unwrap()` in production. |
| **V. Specification Compliance** | ✅ PASS (with audit) | **Standards-native audit per FR-014/FR-016**: Dev-dependency / test-dependency semantics MUST use the existing `LifecycleScope::Development`/`Test` field (which already maps to CDX `scope` + SPDX 2.3 `DEV/TEST_DEPENDENCY_OF` + SPDX 3 `LifecycleScopeType`). New `mikebom:*` properties: `mikebom:download-url` (reuses milestone-052+ convention; no native equivalent for source-declared URLs vs CDX `externalReferences[].url` which mikebom uses for cached download URLs only). `mikebom:vendored` (Boolean marker for the new `add_subdirectory(third_party/...)` case; no native equivalent — CDX `evidence.method` is too generic). `mikebom:bazel-archive-name` (Bazel-specific archive identifier from `http_archive(name = "...")`; no native equivalent). All three audited; documented in spec FR-014 + FR-016 audit clauses; reviewers can verify against the standards-native-precedence table. |
| **VI. Three-Crate Architecture** | ✅ PASS | New readers live under `mikebom-cli/src/scan_fs/package_db/` — no new crate. |
| **VII. Test Isolation** | ✅ PASS | All tests are pure-Rust integration tests; no eBPF privileges required. |
| **VIII. Completeness** | ✅ PASS | Per FR-015, parse errors emit transparency annotation; per Principle X, gaps are surfaced via `mikebom:parse-error` scan-summary annotation. |
| **IX. Accuracy** | ✅ PASS | `--include-vendored` is default-OFF per FR-016 to prevent false-positive inflation. CMake `find_package` excluded per FR-011 to avoid double-counting against OS-package readers + vcpkg/Conan. |
| **X. Transparency** | ✅ PASS | `mikebom:source-files` recorded per FR-012; parse errors surfaced per FR-015; vendored flag annotated per FR-016. |
| **XI. Enrichment** | ✅ PASS | Source-tree manifest readers are enrichment per Principle XI; output uses spec-native fields where available. |
| **XII. External Data Source Enrichment** | ✅ PASS | New readers consume local manifest files only; no external services. The PURL ecosystems (bazel/vcpkg/conan) are content-policy choices, not external-data lookups. |

**Strict Boundaries**:
1. No lockfile-based dependency *discovery* → ✅ this milestone is source-tree manifest *enrichment* (per Principle II + XII clarification); existing readers establish the precedent.
2. No MITM proxy → N/A.
3. No C code → ✅ no new deps.
4. No `.unwrap()` in production → ✅ test code only.

**Result**: All gates PASS. No complexity-tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/102-cpp-bazel-cmake-readers/
├── plan.md                                  # This file
├── spec.md                                  # Feature spec (with 3 clarifications)
├── research.md                              # Phase 0: 8 sections of design Q&A
├── data-model.md                            # Phase 1: file-by-file shapes
├── contracts/
│   └── reader-contracts.md                  # Phase 1: 12 behavioral contracts
├── quickstart.md                            # Phase 1: 6 maintainer recipes
├── checklists/
│   └── requirements.md                      # Spec quality checklist (12/12 PASS)
└── tasks.md                                 # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── Cargo.toml                               # unchanged — no new deps
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       ├── bazel.rs                     # NEW: MODULE.bazel + WORKSPACE.bazel reader
│   │       ├── cmake.rs                     # NEW: CMakeLists.txt + cmake/*.cmake reader
│   │       ├── vcpkg.rs                     # NEW: vcpkg.json reader
│   │       ├── conan.rs                     # NEW: conanfile.txt + conanfile.py reader
│   │       └── mod.rs                       # MODIFY: declare 4 new submodules
│   ├── resolve/
│   │   └── path_resolver.rs                 # MODIFY: 4 new dispatch arms
│   └── cli/
│       └── scan_cmd.rs                      # MODIFY: --include-vendored flag plumbing
└── tests/
    ├── scan_bazel.rs                        # NEW: integration test for US1
    ├── scan_cmake.rs                        # NEW: integration test for US2
    ├── scan_vcpkg.rs                        # NEW: integration test for US3 (vcpkg portion)
    ├── scan_conan.rs                        # NEW: integration test for US3 (conan portion)
    ├── scan_cmake_vendored.rs               # NEW: integration test for --include-vendored
    └── fixtures/
        ├── bazel/                           # NEW: synthetic Bazel project
        │   ├── MODULE.bazel
        │   └── WORKSPACE.bazel
        ├── cmake/                           # NEW: synthetic CMake project
        │   ├── CMakeLists.txt
        │   └── cmake/third_party.cmake
        ├── vcpkg/                           # NEW: synthetic vcpkg manifest
        │   └── vcpkg.json
        ├── conan/                           # NEW: synthetic conanfile.txt + .py
        │   ├── conanfile.txt
        │   └── conanfile.py
        └── golden/
            ├── cyclonedx/                   # NEW: 4 ecosystem goldens (bazel/cmake/vcpkg/conan)
            ├── spdx-2.3/                    # NEW: 4 ecosystem goldens
            └── spdx-3/                      # NEW: 4 ecosystem goldens

README.md                                    # MODIFY: ecosystems table + cli-reference link
docs/user-guide/
└── cli-reference.md                         # MODIFY: --include-vendored flag docs
```

**Structure Decision**: 4 new readers follow the existing template (`mikebom-cli/src/scan_fs/package_db/cargo.rs` as the gold reference; see Existing-Architecture Context below). Each reader exports a `read(scan_root: &Path) -> Vec<PackageDbEntry>` entry function called from `scan_fs::scan_path()` where the existing 11 readers are dispatched. The path-resolver dispatcher gets 4 new arms in its chained `.or_else()` resolution chain. The `--include-vendored` flag follows the milestone-052 `--exclude-scope` pattern — added to `ScanArgs`, passed through `scan_cmd::execute()`, applied as a retention filter before serialization (lines 1240+1539-1570 of `scan_cmd.rs`).

The 4 readers are file-level independent (no shared types beyond `PackageDbEntry`), so each can be implemented + tested in isolation. US3's vcpkg + conan readers share a tiny "C/C++ ecosystem PURL helper" utility that codifies the `pkg:vcpkg/` and `pkg:conan/` PURL construction patterns alongside Bazel's `pkg:bazel/`.

## Existing-Architecture Context

Survey of the patterns the new readers slot into (referenced from research §1+§2):

| Concern | Existing location | Pattern to mirror |
|---|---|---|
| Reader entry point | `mikebom-cli/src/scan_fs/package_db/cargo.rs` ~line 106 — `build_cargo_purl(name, version)` + internal `package_to_entry` | Each new reader exports `read(scan_root: &Path) -> Vec<PackageDbEntry>` |
| Path-resolver dispatch | `mikebom-cli/src/resolve/path_resolver.rs:23` — `resolve_path_with_context(path, deb_codename) -> Option<Purl>` | Chained `.or_else(resolve_bazel_path).or_else(resolve_cmake_path)...` |
| Scope field | `mikebom-common/src/resolution.rs:64` — `ResolvedComponent.lifecycle_scope: Option<LifecycleScope>` | Readers set `PackageDbEntry.lifecycle_scope` during parse; downstream emitters map to CDX `scope` / SPDX `DEV/TEST_DEPENDENCY_OF` automatically |
| CLI flag plumbing | `mikebom-cli/src/cli/scan_cmd.rs:1240` — `pub async fn execute(..., exclude_scope: Vec<LifecycleScope>, ...)` | Add `include_vendored: bool` parameter; apply as retention filter (mirroring `exclude_scope`'s pre-serialization filter at lines 1539-1570) |
| PURL construction | `mikebom-common/src/types/purl.rs:47` — `encode_purl_segment(s)` + `Purl::new(&str)` | Each reader formats `pkg:<ecosystem>/{encode_purl_segment(name)}@{encode_purl_segment(version)}` and validates via `Purl::new()` |
| `mikebom:source-files` | Already plumbed through `PackageDbEntry.source_path` → `ResolvedComponent.evidence.source_file_paths` | Set `entry.source_path = manifest_path` per FR-012 |
| `mikebom:*` annotation | `PackageDbEntry.extra_annotations: Vec<(String, serde_json::Value)>` (milestone-080 pattern) | Set `extra_annotations` for `mikebom:download-url`, `mikebom:vendored`, `mikebom:bazel-archive-name` |
| Parse-error reporting | `tracing::warn!` calls in maven.rs / gem.rs precedent | Same `tracing::warn!` for unparseable files; scan-summary `mikebom:parse-error` annotation populated by the per-reader return tuple |
| Cargo deps | `mikebom-cli/Cargo.toml` — `regex = "1"`, `toml = "0.8"` both present | No new deps; reuse existing |

## Complexity Tracking

No constitution violations. The milestone is purely additive (4 new readers slotting into the existing 11-reader architecture). The standards-native-precedence audit (Principle V) is recorded in spec FR-014 + FR-016 for the 3 new `mikebom:*` properties; each has a justification clause naming why no native field carries the same semantic. No new crates, no new architectural patterns, no constitution-amendment required.

The largest implementation risk is the CMakeLists.txt regex parser — CMake is Turing-complete, so the parser is heuristic by construction. SC-002 caps coverage at ≥90% explicitly to call out the heuristic ceiling; non-literal `FetchContent_Declare` calls (inside macros, with variable-substituted arguments) are documented as out-of-scope. The corpus-grounded ≥90% target is reachable based on spot-checks of LLVM / gRPC / Envoy / RocksDB CMake trees referenced in the spec's Assumptions section.
