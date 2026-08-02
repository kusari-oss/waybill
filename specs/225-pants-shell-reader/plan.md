# Implementation Plan: Pants shell reader

**Branch**: `225-pants-shell-reader` | **Date**: 2026-08-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/225-pants-shell-reader/spec.md`

## Summary

Adds a new source-tier reader at `waybill-cli/src/scan_fs/package_db/pants_shell/`
that walks `BUILD` files under the scan root, extracts
`shell_source` / `shell_sources` / `shunit2_test` / `shunit2_tests`
target declarations via a regex-scoped Pants-DSL parser (per
Constitution Principle I — no embedded Python interpreter), resolves
each target's `source=` / `sources=[glob...]` expression against the
BUILD file's own directory, and emits one `pkg:generic/*` file-tier
component per resolved `.sh` file with a full SHA-256 hash and a
new `waybill:pants-target=<address>` annotation. Also parses
`pants.toml` at the scan root for `[shellcheck]` / `[shfmt]` /
`[shunit2]` `version = "..."` pins and emits each as a design-tier
`pkg:generic/*` build-tool component.

This is the **first Pants BUILD-file walker** across the m223 →
m224 → m225 sequence (m223 walked Pex JSON lockfiles, m224 walked
coursier TOML lockfiles; neither read BUILD files). The BUILD-DSL
extractor infrastructure here lays the groundwork for future Pants
readers (Go / Docker / Kotlin backend).

**One new parity-catalog row** — C145 `waybill:pants-target` —
plus its 3 extractor entries (cdx / spdx23 / spdx3). This is
unavoidable per memory `feedback_sbom_format_mapping_extractor_gate`
because m223's C143 (`waybill:pants-resolve`) carries the resolve
name only, not the target address. Every other annotation reuses
existing catalog rows: `waybill:source-file` (m080-shipped) for the
tool-tier `waybill:source-file=pants.toml` provenance;
`waybill:source-files` (existing row) for the multi-BUILD-file
provenance on scripts. No other new rows.

**Critical Phase 0 items** (research must resolve):
1. Exact `BUILD` file target-function-call shape verified against
   a real Pants shell backend example (`shell_source`,
   `shell_sources`, `shunit2_test`, `shunit2_tests` — arg names +
   ordering + optional kwargs).
2. Regex-scoped extractor pattern: multi-line balanced-parens handling
   for `sources=[...]` list literals + name/source string-literal
   extraction.
3. File-tier PURL shape decision: match m133's placeholder
   (`pkg:generic/file-tier?content-sha256=<sha>`) OR use a
   pants-shell-specific shape that makes each script uniquely
   identifiable in a component listing. Recommend
   `pkg:generic/<basename>@<sha256[:12]>` for readability, with full
   sha256 in `hashes[]` and target address in
   `waybill:pants-target`.
4. `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` subsystem
   sections — exact key names (`version` vs `known_versions` vs
   `install_from_resolve`), value shapes (leading `v` prefix
   convention).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–224; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `regex = "1"` (workspace;
already used pervasively by cmake, alpm, brew, yocto, cocoapods
readers for DSL / line-format extraction), `toml = "0.8"` (workspace;
`pants.toml` parsing), `serde` / `serde_json` (annotation values),
`sha2` (SHA-256 fingerprinting; workspace), `data-encoding` (hex
encoding for content-addressed PURL fragments; workspace), `tracing`
(INFO / WARN diagnostics), `anyhow` / `thiserror` (error propagation).
**Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; mirrors every
language-reader milestone since 002.
**Testing**: `cargo test --workspace` per Constitution Principle VII.
New test binary `waybill-cli/tests/pants_shell_reader.rs` for
integration coverage; per-module `#[cfg(test)]` blocks for BUILD-DSL
parser + target resolver unit tests. Synthetic fixtures under
`waybill-cli/tests/fixtures/pants_shell/` — per
`feedback_fixture_synthetic_package_names`, only
`waybill-fixture-*.sh` script names.
**Target Platform**: Linux + macOS + Windows (matches m100+, m223, m224).
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**:
- Reader adds < 200 ms on a Pants monorepo with 100 BUILD files
  declaring 500 shell scripts (NFR-001).
- Zero cost on repos without any Pants BUILD files (NFR-002 —
  walker early-return once no BUILD files found).
**Constraints**:
- Byte-identical golden output when no Pants BUILD files present
  (SC-003 / FR-011).
- Fail-open at both file scope AND target scope per FR-009 /
  SC-005 (WARN + skip, not scan-abort).
- No shell-out to `pants` binary.
- **One new parity-catalog row** (C145 `waybill:pants-target`) —
  the only unavoidable parity work vs m224's zero.
- BUILD-file DSL parsing is regex-based (Constitution Principle I:
  no embedded Python interpreter, no PyO3).
**Scale/Scope**: 3 user stories (P1/P2/P3), 12 functional
requirements, 6 success criteria. Estimated diff: ~450 LOC
production (BUILD-file walker + regex extractor + target resolver +
tool-pin extractor) + ~350 LOC tests + 6-8 synthetic fixture
directories + ~40 LOC parity work (1 catalog row + 3 extractor
entries).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | Zero new Cargo dependencies. BUILD-file parsing is regex-based per FR-002 / FR-003 assumption — no embedded Python interpreter, no PyO3. Verified post-implementation by rerunning `no_c_dependencies_in_tree` regression test. |
| II. eBPF-Only Observation | ➖ | N/A | User-space reader; `waybill-ebpf` untouched. |
| III. Fail Closed | ✅ | PASS | Fail-open at per-file AND per-target grain per FR-009 (matches every existing regex-driven reader — cmake, alpm, brew, yocto). Scan-wide-halt on one bad BUILD file would break polyglot repos where 99% of BUILD files are fine. |
| IV. Type-Driven Correctness | ✅ | PASS | Introduces typed `ShellTargetKind` enum (`ShellSource` / `ShellSources` / `Shunit2Test` / `Shunit2Tests`) + typed `TargetDeclaration` + `TargetParseError` sum types. Uses `waybill_common::types::purl::Purl` newtype for PURL construction (never raw strings past the boundary). `#[cfg_attr(test, allow(clippy::unwrap_used))]` at test-mod level per existing convention. |
| V. Specification Compliance | ✅ | PASS with 1 new C-row | Native-fields-first audit (Principle V bullet 5): PURLs go to native slots, SHA-256 goes to native `hashes[]`, `lifecycle_scope` goes to native CDX `scope=excluded` / SPDX 3 `LifecycleScopeType`. Only one waybill-namespaced signal remains: `waybill:pants-target` (target address string) — no CDX / SPDX 2.3 / SPDX 3 native carrier exists (CDX `evidence.identity[].technique` is per-parse-technique, not per-build-declaration; SPDX 3 `software_Package.additionalPurpose` is a per-component role). New catalog row C145 `waybill:pants-target` documented in Phase 1 contracts/. `waybill:source-file` (C-row shipped by m080) reused for tool-tier provenance. |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli/src/scan_fs/package_db/pants_shell/`. Parity extractor entries land in `waybill-cli/src/parity/extractors/`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Reader runs without root / CAP_BPF. Integration tests use synthetic fixtures. No network access (BUILD files are on-disk). |
| VIII. Completeness | ✅ | PASS | Coverage delta: adds shell-script inventory for Pants monorepos where zero components emit today. Also inventories pinned lint/test tooling (`shellcheck`, `shfmt`, `shunit2`) — currently ~invisible to waybill on Pants repos. |
| IX. Accuracy | ✅ | PASS | Source-tier fidelity: BUILD file is authoritative; SHA-256 fingerprints computed from on-disk file bytes at scan time (no cached / stale hashes). `sbom_tier="source"` on scripts, `sbom_tier="design"` on tool pins. No fabrication. |
| X. Transparency | ✅ | PASS | FR-010 INFO log records `build_files_discovered=N build_files_parsed_ok=N build_files_skipped_corrupt=N shell_targets_found=N script_components_emitted=N tool_components_emitted=N` per scan — Principle X signal. WARN diagnostics on per-file or per-target corruption name the offending BUILD file + line range. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature; no online enrichment. |
| XII. External Data Source Enrichment | ➖ | N/A | No external data source. Reader is purely filesystem-local. |

**Result**: PASS on all 12 principles. One legitimate parity-work
addition (C145) documented + budgeted in Phase 1 contracts/. No
entries required in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/225-pants-shell-reader/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   ├── build-file-dsl-schema.md               # BUILD-file target grammar + fail-open contract
│   └── c145-waybill-pants-target.md           # New catalog row spec + extractor contract
├── checklists/
│   └── requirements.md                        # /speckit.specify output (16/16 PASS)
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── scan_fs/
│   │   └── package_db/
│   │       ├── mod.rs                         # +pub mod pants_shell; + read_all wire-in
│   │       └── pants_shell/                   # NEW module directory
│   │           ├── mod.rs                     # Public read() entry + orchestrator
│   │           ├── build_dsl.rs               # BUILD-file target-declaration regex extractor
│   │           ├── target_resolver.rs         # source=/sources=[glob...] → Vec<PathBuf>
│   │           ├── config.rs                  # pants.toml [shellcheck]/[shfmt]/[shunit2] version pins
│   │           └── component_emit.rs          # Script + tool → PackageDbEntry mapping
│   ├── parity/
│   │   └── extractors/
│   │       ├── mod.rs                         # +C145 ParityExtractor row
│   │       ├── cdx.rs                         # +c145_cdx extractor
│   │       ├── spdx2.rs                       # +c145_spdx23 extractor
│   │       └── spdx3.rs                       # +c145_spdx3 extractor
├── tests/
│   ├── pants_shell_reader.rs                  # NEW integration test file
│   └── fixtures/
│       └── pants_shell/                       # NEW synthetic fixtures directory
│           ├── minimal_scripts/               # US1 baseline (2 scripts, 1 target)
│           │   ├── scripts/BUILD
│           │   ├── scripts/waybill-fixture-deploy.sh
│           │   └── scripts/waybill-fixture-rollback.sh
│           ├── glob_sources/                  # US1 scenario 2: sources=["*.sh"] resolves N files
│           │   └── helpers/BUILD + 3 fixture scripts
│           ├── with_shell_setup/              # US2: pants.toml pins
│           │   ├── pants.toml
│           │   └── scripts/BUILD + 1 fixture script
│           ├── shunit2_dev_scope/             # US3: test-scoped tagging
│           │   ├── tests/BUILD
│           │   └── tests/waybill-fixture-deploy-test.sh
│           ├── missing_source_file/           # Edge case: BUILD points at nonexistent .sh
│           │   └── scripts/BUILD (declares source that doesn't exist)
│           ├── malformed_build_partial/       # Edge case: 3 valid + 1 malformed target in one BUILD
│           │   └── scripts/BUILD + 3 fixture scripts
│           └── dupe_target_owners/            # Edge case: same .sh owned by 2 targets
│               └── scripts/BUILD + 1 fixture script (referenced by both shell_source and shunit2_tests glob)
```

**Changes to `docs/reference/sbom-format-mapping.md`**:
- Add row C145 `waybill:pants-target` (per Principle V bullet 5
  full-audit format with all 5 columns: description, CDX carrier,
  SPDX 2.3 carrier, SPDX 3 carrier, KEEP-NO-NATIVE rationale).

**Structure Decision**: Module-directory layout
(`package_db/pants_shell/`) matches m223 (`pants/`) and m224
(`pants_jvm/`) exactly. Naming: `pants_shell` (not `pants_bash`
or `pants_bourne`) mirrors the upstream Pants backend module name
(`pants.backend.shell`).

Reader-surface contract (matches existing readers):
- `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>` at
  `pants_shell/mod.rs`, called from
  `scan_fs/package_db/mod.rs::read_all`.
- Emits `PackageDbEntry` per shared struct with
  `sbom_tier: Some("source")` for scripts,
  `sbom_tier: Some("design")` for tool pins.
- Fail-open at per-file AND per-target grain (finer than m223 /
  m224's per-lockfile grain — one bad target inside a BUILD file
  does not skip the rest of that file).

Coexistence with the m133 file-tier walker: the pants-shell reader
emits `PackageDbEntry` records via the standard `package_db::read_all`
path. The m133 walker's dedupe index (built AFTER all package-db
readers complete) will see the script files' paths in the reader's
emitted `source_path` field — the walker automatically de-dupes so
no double-emission. Zero interaction changes needed on the m133 side.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated (research/ files being written by this command)
- [x] Phase 1: data-model.md, contracts/*, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS on all 12 principles + one legitimate C-row addition

## Follow-ups (out-of-scope for this branch)

- **`shell_command` target emission** (Pants's arbitrary-command
  wrapper): deferred per spec Out-of-Scope. Would require modeling
  "actions" as SBOM subjects — architectural addition that touches
  more than just this reader.
- **Custom plugin-registered shell target types**: deferred per
  spec Out-of-Scope. Currently only the four built-in target types
  are recognized.
- **Nested `pants.toml` files**: only scan-root `pants.toml`
  consulted per Assumption. If operator repos actually use nested
  configs, revisit.
- **BUILD-file walker as generalized infrastructure**: this feature
  puts the walker + regex extractor in `pants_shell/build_dsl.rs`.
  When m226 (hypothetical Pants Go BUILD walker) lands, promote the
  walker + extractor to a shared `pants_common/build_walker/`
  module. Not done here per YAGNI — one consumer.
- **shunit2 built-in bundle discovery**: Pants ships an embedded
  shunit2; not enumerated in v1 (only operator-pinned
  `[shunit2] version = "..."`).
