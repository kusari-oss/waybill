# Implementation Plan: Emit main-module for Gemfile-only Ruby applications

**Branch**: `216-gemfile-main-module` | **Date**: 2026-07-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/216-gemfile-main-module/spec.md`

## Summary

Extend `waybill-cli/src/scan_fs/package_db/gem.rs` with a NEW walker
`find_top_level_gemfiles(rootfs) -> Vec<PathBuf>` and NEW builder
`build_gem_application_main_module_entry(gemfile_path) -> Option<PackageDbEntry>`
that mirrors the existing m069 gemspec-driven `find_top_level_gemspecs` +
`build_gem_main_module_entry` pair. The application builder emits
`pkg:generic/<dirname>@<version>` (per FR-002's purl-spec-blessed choice)
with a `waybill:package-shape = "application"` annotation. The application
walker excludes any directory that also carries a `.gemspec` (FR-007 — the
gemspec path already emits the main-module identity).

**Zero new Cargo dependencies**: reuses `Path`, `std::fs`, the existing
`safe_walk` infrastructure, `waybill_common::types::purl::Purl`, and the
existing `tracing`/`anyhow`/`serde_json` workspace deps.

**Zero risk to pre-feature scans**: the new emission path only fires on
directories carrying `Gemfile` AND NOT carrying `.gemspec`. Every existing
fixture (gemspec-carrying) hits FR-007's gemspec-wins branch and produces
byte-identical output → the 33 pre-feature `{cdx,spdx,spdx3}_regression`
tests continue to pass.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–215; no nightly required for this user-space-only reader extension).

**Primary Dependencies**: Existing only. **Zero new Cargo dependencies.**
- `std::fs`, `std::path::{Path, PathBuf}` for filesystem walks.
- `waybill_common::types::purl::Purl` for PURL construction + validation (`pkg:generic/` accepted by the `packageurl` crate under the same validator as any other type).
- `tracing` for `warn!` on parse errors / `debug!` for detection.
- `anyhow` / `thiserror` for error propagation.
- `serde_json::Value` for the `waybill:package-shape = "application"` annotation payload.
- The milestone-114 `scan_fs::walk::safe_walk` for descent (matches the m069 walker's convention).

**Storage**: N/A — pure filesystem reads; all state in-process for the lifetime of a scan.

**Testing**:
- Unit tests in `gem.rs::tests` for `find_top_level_gemfiles` (walker semantics: skips `vendor/`, `gems/`, `.bundle/`, respects the gemspec-wins guard) and `build_gem_application_main_module_entry` (PURL shape, annotation set, name derivation, version fallback).
- Integration test in `waybill-cli/tests/gemfile_main_module.rs` (new file) exercising a real Gemfile-only fixture end-to-end via the `waybill sbom scan` subprocess.
- Real-world reproducer: rerun m215's `--split` against `~/Projects/iac`, assert 37 sub-SBOMs (was 34) + confirm the 3 Ruby app entries carry the expected shape.

**Target Platform**: All existing platforms (linux-x86_64 default + ebpf-tracing, macOS, Windows). No platform-specific behavior.

**Project Type**: Same three-crate architecture (Constitution Principle VI) untouched — this feature edits `waybill-cli/src/scan_fs/package_db/gem.rs` (+ new tests). `waybill-common`: no changes. `waybill-ebpf`: no changes.

**Performance Goals**:
- Detection overhead: the new walker traverses the same tree the m069 gemspec walker already visits (`safe_walk` from the scan root, depth-bounded by `MAX_GEMSPEC_WALK_DEPTH`). Zero new I/O on non-Ruby scans (the walker short-circuits on `find_top_level_gemfiles(rootfs) == []`). Ruby-carrying repos incur one additional `Path` comparison per directory visited — sub-millisecond on the 38-boundary iac reproducer.
- Emit overhead: linear in the count of application main-modules (typically 1-5 in a monorepo).

**Constraints**:
- **FR-007 no-double-emit**: gemspec-carrying directories MUST NOT get a Gemfile-derived main-module. Enforced by checking for `.gemspec` sibling in the walker.
- **FR-009/FR-010 backwards-compat**: no drift on non-Ruby scans or gemspec-only fixtures. Enforced by pre-feature regression tests + the walker's conditional-entry pattern (nothing emitted when no Gemfile exists).
- **Constitution Principle I**: no Ruby-runtime shellouts. Static parsing only (Gemfile's `source`/`gem`/`gemspec` DSL is NOT parsed by this feature — application name comes from the directory basename per FR-003; version comes from a git-describe fallback OR `0.0.0-unknown` per FR-004).
- **Constitution Principle V**: `pkg:generic/` PURL type is the purl-spec-blessed escape hatch (per spec's Clarifications section 2026-07-22). Companion `waybill:package-shape = "application"` is a parity-bridging annotation — the purl-spec has no application-vs-library type distinction, so this is a genuinely new signal (not a reinvention of a native construct). Passes the standards-native audit.

**Scale/Scope**:
- New code: ~150 LOC in `gem.rs` (walker + builder + `is_manifest_basename`-analog check + version-inference helper).
- Modified: same file's `read()` dispatch loop gets a second pass mirroring the gemspec loop.
- New test file: `waybill-cli/tests/gemfile_main_module.rs` (~100 LOC covering happy-path + gemspec-precedence + no-lock-graph-partial + split-mode-integration).
- New fixture: `waybill-cli/tests/fixtures/gemfile_application/` (`Gemfile` + `Gemfile.lock` with 2-3 declared deps, no gemspec).
- Total estimated diff: **~250 LOC production + ~150 LOC test + 2-4 fixture files**.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No C. All new code in `gem.rs`.
- **II. eBPF-Only Observation**: ✅ N/A. Filesystem-driven package-DB reader (enrichment layer per Principle XII, not discovery). Same category as every other `scan_fs/package_db/` module.
- **III. Fail Closed**: ✅ New emission is additive — nothing fails; if the Gemfile parse encounters an edge case, the walker skips that directory and emits a WARN (matches the m069 pattern).
- **IV. Type-Driven Correctness**: ✅ Uses existing `Purl` newtype for PURL construction. New annotation value is a typed `serde_json::Value::String`. Zero `.unwrap()` in production paths; test code guards with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per convention.
- **V. Specification Compliance**: ✅
  - **PURL spec audit** (per Principle V's standards-native-precedence requirement): `pkg:generic/` is the purl-spec-blessed choice per spec's Clarifications resolution. Cited in FR-002.
  - **Companion annotation audit**: `waybill:package-shape = "application"` is a genuinely new signal — the purl-spec has no application-vs-library distinction, CycloneDX 1.6 has no direct equivalent (its `Component.type` enum values `application` / `library` / `framework` / etc. describe the *component role in the assembly*, which overlaps but isn't equivalent to "how was this component's identity inferred"), SPDX 2.3 has no field, SPDX 3.0.1 has no field. New parity-bridging annotation is justified. Will be documented in `docs/reference/sbom-format-mapping.md` per Principle V requirements as part of implementation.
- **VI. Three-Crate Architecture**: ✅ Only `waybill-cli` changes. `waybill-common` + `waybill-ebpf` untouched.
- **VII. Test Isolation**: ✅ Pure-Rust unit + integration tests. No eBPF required. Cross-platform.
- **VIII. Completeness**: ✅ Positive impact — the feature CLOSES a completeness gap (Ruby apps invisible to `--split` today). Every `Gemfile` observed by the walker becomes an emitted component under the orphan-fallback contract's spirit.
- **IX. Accuracy**: ✅ Directory-basename-derived names carry an appropriate `waybill:package-shape` marker so consumers know the name came from inference, not from a manifest declaration. No false-positive risk beyond that.
- **X. Transparency**: ✅ WARN log on any Gemfile-parse edge case (matches m069). The new annotation is inherently transparent about the component's inferred source.
- **XI. Enrichment**: ✅ N/A — this is discovery-adjacent (per Principle II's clarification that lockfile reads for existing components are enrichment, not discovery). The main-module *identity* is inferred from filesystem shape, matching how every ecosystem reader today emits its main-module.
- **XII. External Data Source Enrichment**: ✅ N/A. No new external calls. `git describe` fallback (FR-004 assumption) uses the same subprocess pattern as milestone-053 Go-module version resolution — already-accepted precedent.

**Strict Boundaries**:
1. No lockfile-based discovery — N/A (main-module identity comes from filesystem, not `Gemfile.lock` content).
2. No MITM proxy — N/A.
3. No C code — enforced.
4. No `.unwrap()` in production — enforced.
5. No file-tier duplicates in default mode — N/A (this feature is a package-DB-reader extension; file-tier walker is unaffected).

**Verdict**: ✅ Constitution check passes. Zero unjustified violations. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/216-gemfile-main-module/
├── plan.md                    # This file (/speckit.plan output)
├── research.md                # Phase 0 — walker + name-derivation + version-fallback strategies
├── data-model.md              # Phase 1 — one new record type (application main-module PackageDbEntry shape)
├── quickstart.md              # Phase 1 — operator recipe: scan a Gemfile-only dir, inspect emitted PURL + annotation
├── contracts/
│   └── application-main-module.md  # C1: emission contract (walker predicate, PURL construction, annotation set, backwards-compat guarantees)
├── checklists/
│   └── requirements.md        # (already exists from /speckit.specify — all PASS)
└── tasks.md                   # Phase 2 output — NOT created here
```

### Source Code (repository root)

```text
waybill-cli/
└── src/
    └── scan_fs/
        └── package_db/
            └── gem.rs         # +find_top_level_gemfiles walker
                               # +build_gem_application_main_module_entry builder
                               # +dispatch loop in read() mirroring the gemspec loop
                               # +unit tests in the existing `tests` module

waybill-cli/tests/
├── gemfile_main_module.rs     # NEW — 4 integration tests
├── fixtures/
│   └── gemfile_application/   # NEW — Gemfile + Gemfile.lock + no gemspec (2-3 declared deps)
└── (rest unchanged)

docs/reference/
└── sbom-format-mapping.md     # +row for waybill:package-shape (parity-bridging annotation
                               #  documentation per Constitution Principle V)

waybill-common/                # UNTOUCHED
waybill-ebpf/                  # UNTOUCHED
xtask/                         # UNTOUCHED
```

**Structure Decision**: All edits in `waybill-cli/src/scan_fs/package_db/gem.rs` (~150 LOC production addition), one new integration-test file, one new fixture directory (~3 files), one doc row addition. Matches the m069 (initial gem main-module) code shape exactly — the feature is m069's "Phase B" if m069 hadn't scoped to gemspec-carrying projects.

## Complexity Tracking

> No Constitution violations. Complexity tracking section unused.
