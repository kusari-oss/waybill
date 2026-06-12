# Implementation Plan: User-Supplied Directory Exclusion for `mikebom scan`

**Branch**: `113-exclude-path-flag` | **Date**: 2026-06-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/113-exclude-path-flag/spec.md`

## Summary

Add `--exclude-path <PATH_OR_PATTERN>` to `mikebom scan` so operators can opt-in to excluding directories that contain fixture/sample projects. The flag is repeatable, accepts both literal paths (anchored at scan root) and glob patterns (matched at arbitrary depth), and is honored by every ecosystem walker that emits components (cargo, maven, gem, pip, npm, gradle, nuget, yocto, golang source, go binary). Off by default — zero exclusion entries produce byte-identical output to a pre-feature build. Solves the same shape of inverted-dependency-edge bug that the milestone-113 Go `testdata/` fix solved for Go, but for ecosystems where there's no documented language convention to lean on.

**Technical approach**: introduce `ExclusionSet` (newtype around `Vec<ExclusionEntry>` where `ExclusionEntry` is an enum `Literal(PathBuf) | Pattern(globset::Glob)`) parsed once at CLI boundary, threaded through `scan_path` → each reader → each walker's descent decision. The shared `should_skip_default_descent` and the per-walker `should_skip_descent` helpers gain an exclusion-set parameter and consult it after the existing built-in skips. Pattern-matching uses the `globset` crate (NEW direct dep; pure Rust, no negation, `**` semantics aligned with FR-006).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–113; no nightly required for this user-space-only work).
**Primary Dependencies**: existing only EXCEPT one new direct dep — `globset = "0.4"` (pure Rust, pulls `regex` + `regex-syntax` which are already in the workspace dependency closure). Existing crates reused: `clap` (the new flag via `ArgAction::Append` derive + `value_parser` validation), `serde`/`serde_json` (transparency annotation emission), `tracing` (debug logs for matched directories), `anyhow`/`thiserror` (parse-error class for malformed patterns), `walkdir`/`std::fs::canonicalize` (existing descent helpers — unchanged). `url` (workspace) for env-var-list separator handling on Windows is not needed; we use `std::env::var` + `std::env::join_paths`-style splitting.
**Storage**: N/A — exclusion entries are in-process per scan; no caches, no persistence.
**Testing**: `cargo +stable test --workspace` (existing harness). New tests: ~12 unit tests in `exclude_path::mod` covering classification, matching, edge cases; per-walker integration tests (one per ecosystem) verifying suppression; one cross-ecosystem polyglot fixture in `mikebom-cli/tests/exclude_path_integration.rs`; byte-identity regression test asserting no-flag output equals the pre-feature golden. Reuses milestone-090 fixture-cache mechanism for the polyglot repo.
**Target Platform**: Linux x86_64 + macOS aarch64 + Windows x86_64 (the same matrix `mikebom scan` already supports; no platform-specific code in the new path).
**Project Type**: Single-project Rust CLI (`mikebom-cli/`) — matches every milestone since 001.
**Performance Goals**: ≤10% scan-time overhead vs a no-flag scan (SC-003); achieved by parsing entries once at CLI boundary and using `globset::GlobSet::is_match` (O(1) per directory amortized).
**Constraints**: byte-identical output when no entries supplied (FR-003 / SC-002); malformed patterns must abort before any walker begins (FR-007 / SC-005); cross-platform path semantics (FR-009).
**Scale/Scope**: typical scan walks ~10k–100k directories; expect 1–10 user-supplied entries per invocation in normal use.

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | `globset` is pure Rust. |
| II. eBPF-Only Observation | N/A | This feature affects `scan` (lockfile/manifest discovery), not `trace` (eBPF). Constitution Principle II's discovery-source rule applies to the trace path; `scan` is already permitted to read lockfiles for enrichment per Principle XII. Exclusion narrows what scan emits; it does not introduce new discovery sources. |
| III. Fail Closed | ✓ | Malformed patterns abort the scan with non-zero exit before any walker begins (FR-007). No silent degradation. |
| IV. Type-Driven Correctness | ✓ | `ExclusionEntry` enum + `ExclusionSet` newtype; no raw `String`s threaded across boundaries; production code uses `Result<_, ExcludePathError>` (thiserror) and `anyhow` at CLI boundary; zero `.unwrap()` in production paths. |
| V. Specification Compliance | ✓ | This feature adds no new `mikebom:*` properties to component records. The Principle-X transparency annotation (see below) IS a `mikebom:*` property, but the standards audit (CDX 1.6 / SPDX 2.3 / SPDX 3) finds no native construct for "the operator excluded these directories during scan" — this is a mikebom-specific scan-time control with no upstream-format equivalent, qualifying for a parity-bridging annotation per Principle V bullet 5. Documented in `docs/reference/sbom-format-mapping.md`. |
| VI. Three-Crate Architecture | ✓ | Lives in `mikebom-cli/`; no new crates. |
| VII. Test Isolation | ✓ | Pure logic + filesystem-walk tests; no eBPF, no privileged operations. |
| VIII. Completeness | ✓ | Principle VIII explicitly carves out "unless explicitly filtered by a user-specified exclusion rule" — this feature IS that carve-out's first-class realization. |
| IX. Accuracy | ✓ | Suppresses operator-confirmed false positives; cannot suppress true positives because the operator is the authority on what's a fixture in their repo. |
| X. Transparency | ✓ (with required annotation) | When any exclusion entry is in use, the emitted SBOM MUST carry an envelope-level `mikebom:exclude-path` annotation listing the active entries verbatim, so downstream consumers can see that this isn't an exhaustive component list. Pattern entries are emitted as-is; literal entries are normalized to forward-slash form for cross-platform consistency. No sensitive data flows through — these are operator-typed directory names. |
| XI. Enrichment | N/A | This feature suppresses, doesn't enrich. |
| XII. External Data Source Enrichment | N/A | No external data source involved. |
| Strict Boundary 1 (no lockfile-based discovery) | ✓ | Unchanged — exclusion narrows existing discovery surface. |
| Strict Boundary 2 (no MITM) | N/A | |
| Strict Boundary 3 (no C code) | ✓ | `globset` is pure Rust. |
| Strict Boundary 4 (no `.unwrap()` in production) | ✓ | All new error paths use `?` + `thiserror`/`anyhow`. |

**Result**: Constitution Check PASSES. The Principle-X transparency annotation is a hard requirement of the principle, NOT a violation — it's added to the design here so the implementation phase doesn't omit it.

## Project Structure

### Documentation (this feature)

```text
specs/113-exclude-path-flag/
├── plan.md              # This file
├── research.md          # Phase 0 — dialect/separator/integration decisions
├── data-model.md        # Phase 1 — ExclusionSet, ExclusionEntry
├── quickstart.md        # Phase 1 — operator walkthrough
├── contracts/
│   ├── cli-flag.md      # --exclude-path syntax, env-var counterpart
│   ├── walker-api.md    # the new exclusion-set parameter signature
│   └── annotations.md   # mikebom:exclude-path transparency annotation
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs          # +ExclusionSet parsing from CLI/env, threaded into scan_path
│   ├── scan_fs/
│   │   ├── mod.rs               # scan_path() gains exclusion_set: &ExclusionSet
│   │   └── package_db/
│   │       ├── exclude_path.rs  # NEW — ExclusionSet, ExclusionEntry, parse, match
│   │       ├── project_roots.rs # should_skip_default_descent gains &ExclusionSet
│   │       ├── cargo.rs         # local should_skip_descent gains &ExclusionSet
│   │       ├── maven.rs         # local should_skip_descent gains &ExclusionSet
│   │       ├── gem.rs           # local should_skip_descent gains &ExclusionSet
│   │       ├── go_binary.rs     # local should_skip_binary_descent gains &ExclusionSet
│   │       ├── golang/
│   │       │   └── legacy.rs    # local should_skip_descent gains &ExclusionSet
│   │       ├── mod.rs           # read_all() threads exclusion_set through every per-ecosystem read()
│   │       └── …                # pip/npm/gradle/nuget/yocto inherit via project_roots.rs
│   ├── generate/
│   │   ├── cyclonedx/metadata.rs   # emit mikebom:exclude-path envelope annotation
│   │   ├── spdx/annotations.rs     # emit mikebom:exclude-path on the SPDX 2.3 document
│   │   └── spdx/v3_annotations.rs  # emit mikebom:exclude-path on the SPDX 3 element
│   └── main.rs                  # +clap flag definition, env-var fallback wiring
├── tests/
│   ├── exclude_path_integration.rs  # NEW — polyglot fixture, per-ecosystem suppression + byte-identity
│   └── fixtures/
│       └── exclude_path/        # NEW — vendored polyglot fixture (cargo + npm + pip + maven + go all under tests/fixtures/)
docs/
├── user-guide/
│   └── cli-reference.md         # +--exclude-path section with non-Go worked example
├── reference/
│   └── sbom-format-mapping.md   # +mikebom:exclude-path annotation row
└── ecosystems.md                # +cross-link from each ecosystem section
mikebom-common/
└── src/
    └── resolution.rs            # no changes needed — exclusion happens pre-emission
```

**Structure Decision**: Single-project layout (every milestone since 001). The new `exclude_path.rs` module lives in `scan_fs/package_db/` because that's where every consumer of the exclusion set lives; the CLI parser delegates to its `parse_one_entry` constructor and surfaces a single `ExclusionSet` upward.

## Complexity Tracking

No constitution violations. No complexity to justify.
