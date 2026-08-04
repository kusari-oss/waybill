# Implementation Plan: Pants Go reader

**Branch**: `226-pants-go-reader` | **Date**: 2026-08-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/226-pants-go-reader/spec.md`

## Summary

Adds a Pants-aware Go enrichment layer at
`waybill-cli/src/scan_fs/package_db/pants_go/` that:

1. Walks `BUILD` files under the scan root via `safe_walk` (same
   discovery as m225 pants_shell), extracts `go_binary` /
   `go_package` / `go_third_party_package` / `go_mod` target
   declarations via a regex-scoped Pants-DSL parser (Constitution
   Principle I — no embedded Python interpreter).
2. Builds an `import_path → Vec<TargetAddress>` map plus a
   `go_mod_root_dir → TargetAddress` map from the parsed targets.
3. Runs a **post-`read_all` enrichment pass** at
   `scan_fs/mod.rs:1001` (immediately after m191 reconciler +
   before m148 canonicalization) that iterates existing
   `pkg:golang/*` components, matches them by module path /
   go.sum source directory, and injects a `waybill:pants-target`
   annotation (reusing m225's C145 catalog row — no new row).
4. Optionally parses `pants.toml` `[golang] expected_version`
   and emits a standalone design-tier `pkg:generic/go@<version>`
   component via the normal `read_all` path — analogous to m225's
   shellcheck/shfmt/shunit2 tool pins.

**Zero fabrication**: pants_go emits ZERO `pkg:golang/*`
components of its own (FR-012, Principle IX). It only enriches
components the existing Go reader already produced from
authoritative go.sum entries. A `go_third_party_package(import_path=X)`
with no matching go.sum entry logs an INFO diagnostic + emits
nothing.

**Zero new parity-catalog rows** vs m225. C145 `waybill:pants-target`
is broadened via a doc-only description update — the extractor
macro already matches on annotation-key regardless of ecosystem.
This is the same "reuse m223's C143/C144" savings that m224 got.

**Critical Phase 0 items** (research must resolve):
1. Exact Pants Go BUILD-DSL target-function-call shape verified
   against a real Pants Go sample (`go_binary`, `go_package`,
   `go_third_party_package`, `go_mod` — arg names + defaults).
2. `go_mod ownership root` inference: given a `pkg:golang/*`
   component's `source_path` (which typically points at a
   go.sum file), how do we walk up to find the enclosing
   `go_mod`-declaring BUILD file?
3. Main-module attribution for `go_binary(main=...)` targets:
   what makes waybill's main-module Go component identifiable
   (the m053 main-module emission), and how do we correlate
   `main="./cmd/foo"` paths to it?
4. C145 broadening: doc-only description update vs bumping the
   row to C146 — decision confirmed at plan time based on
   extractor architecture.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–225; no nightly required).
**Primary Dependencies**: Existing only — `regex = "1"` (workspace;
reuses m225 pants_shell's regex-based DSL extractor patterns),
`toml = "0.8"` (`pants.toml` parsing; workspace), `serde` /
`serde_json` (annotation values), `tracing` (INFO / WARN
diagnostics), `anyhow` / `thiserror` (error propagation). **Zero
new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan.
**Testing**: `cargo test --workspace` per Constitution Principle VII.
New test binary `waybill-cli/tests/pants_go_reader.rs`. Per-module
`#[cfg(test)]` blocks for DSL parser + resolver + enrichment
unit tests. Synthetic fixtures under
`waybill-cli/tests/fixtures/pants_go/` — synthetic module names
per `feedback_fixture_synthetic_package_names`
(`github.com/waybill-fixture/*`).
**Target Platform**: Linux + macOS + Windows.
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**:
- Enrichment pass < 100 ms on 100 BUILD files × 500 `pkg:golang/*`
  components (NFR-001).
- Zero cost on repos without any Pants BUILD files AND no
  `pants.toml` `[golang]` section (NFR-002 — early return).
**Constraints**:
- Byte-identical golden output when no Pants BUILD files
  declaring Go targets AND no `pants.toml` `[golang]` (SC-003 /
  FR-011).
- Fail-open at per-file AND per-target grain per FR-009 / SC-005.
- No shell-out to `pants` binary.
- **Zero new parity-catalog rows** — C145 broadened via a doc
  update (extractor macros already ecosystem-agnostic).
- BUILD-DSL parsing is regex-based (Constitution Principle I: no
  embedded Python interpreter).
- **Zero fabrication of `pkg:golang/*` components** (FR-012 /
  Principle IX): enrichment only, never synthetic emission.
**Scale/Scope**: 3 user stories (P1/P2/P3), 12 functional
requirements, 6 success criteria. Estimated diff: ~400 LOC
production (BUILD walker + regex extractor for Go targets +
`go_mod` root inference + enrichment pass + tool-pin emission)
+ ~300 LOC tests + 5–7 synthetic fixtures.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | Zero new Cargo dependencies. BUILD-DSL parsing is regex-based — no embedded Python interpreter, no PyO3. Verified post-implementation by rerunning `no_c_dependencies_in_tree` regression test. |
| II. eBPF-Only Observation | ➖ | N/A | User-space enrichment; `waybill-ebpf` untouched. |
| III. Fail Closed | ✅ | PASS | Fail-open at per-file AND per-target grain per FR-009 (matches m225 posture). Scan-wide-halt on one bad BUILD file would break polyglot repos. |
| IV. Type-Driven Correctness | ✅ | PASS | Introduces typed `GoTargetKind` enum + `GoTargetDeclaration` + `GoOwnershipIndex` struct. Uses existing `waybill_common::types::purl::Purl` newtype and `waybill_common::resolution::ResolvedComponent`. `#[cfg_attr(test, allow(clippy::unwrap_used))]` at test-mod level per existing convention. |
| V. Specification Compliance | ✅ | PASS with C145 broadening | Native-fields-first (Principle V bullet 5): the `waybill:pants-target` semantic is identical to m225's — target address(es) owning the component. Extractor macros already match on annotation key only, so no new row is needed. Doc-only broadening of C145's description to include `pkg:golang/*` scope. Rejected native alternatives audit is unchanged from m225 (CDX `evidence.identity[].technique` is per-parse, not build-system-target). |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli/src/scan_fs/package_db/pants_go/` + one new call site in `scan_fs/mod.rs`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Enrichment runs without root/CAP_BPF. Integration tests use synthetic fixtures. No network access. |
| VIII. Completeness | ✅ | PASS | Coverage delta: adds Pants target attribution for Go components (currently invisible on Pants Go monorepos). |
| IX. Accuracy | ✅ | PASS | **Zero fabrication** per FR-012: pants_go emits no `pkg:golang/*` components of its own. Only enriches components the existing Go reader already produced from authoritative go.sum entries. Toolchain-pin emission carries `sbom_tier="design"` (operator-declared, not built) — honest tier tagging. |
| X. Transparency | ✅ | PASS | FR-010 INFO log records `build_files_discovered=N build_files_parsed_ok=N build_files_skipped_corrupt=N go_targets_found=N components_annotated=N toolchain_component_emitted=<0\|1>` per scan. WARN diagnostics on per-file / per-target corruption name the offending file + line range. INFO diagnostic when a `go_third_party_package(import_path=X)` names a dep with no matching go.sum entry. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature; no online enrichment. |
| XII. External Data Source Enrichment | ➖ | N/A | No external data source. Enrichment is purely filesystem-local. |

**Result**: PASS on all 12 principles. Zero new parity rows.
Zero fabrication. No entries required in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/226-pants-go-reader/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   ├── go-build-dsl-schema.md                 # Pants Go BUILD target grammar + enrichment contract
│   └── c145-broadening.md                     # C145 semantic-broadening description update
├── checklists/
│   └── requirements.md                        # /speckit.specify output (16/16 PASS)
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── scan_fs/
│   │   ├── mod.rs                             # +pants_go enrichment call site (line ~1001, after m191 reconciler)
│   │   └── package_db/
│   │       ├── mod.rs                         # +pub mod pants_go; + tool-pin call in read_all
│   │       └── pants_go/                      # NEW module directory
│   │           ├── mod.rs                     # Public read() (tool-pin emit) + enrich() (post-read_all pass) entries
│   │           ├── build_dsl.rs               # Pants Go target-declaration regex extractor
│   │           ├── ownership_index.rs         # go_mod_root_dir → address + import_path → addresses maps
│   │           ├── config.rs                  # pants.toml [golang] expected_version parser
│   │           └── enrichment.rs              # inject `waybill:pants-target` onto pkg:golang/* components
├── tests/
│   ├── pants_go_reader.rs                     # NEW integration test file
│   └── fixtures/
│       └── pants_go/                          # NEW synthetic fixtures directory
│           ├── minimal_3rdparty_go/           # US1 baseline (go_mod + go.sum with 3 entries)
│           │   ├── 3rdparty/go/BUILD
│           │   ├── 3rdparty/go/go.mod
│           │   └── 3rdparty/go/go.sum
│           ├── explicit_third_party_targets/  # US1 scenario 2: go_third_party_package
│           │   ├── 3rdparty/go/BUILD
│           │   ├── 3rdparty/go/go.mod
│           │   └── 3rdparty/go/go.sum
│           ├── go_binary_first_party/         # US3: go_binary + go_package (first-party)
│           │   ├── go.mod
│           │   ├── go.sum
│           │   ├── cmd/frontend/BUILD
│           │   └── cmd/frontend/main.go
│           ├── with_toolchain_pin/            # US2: pants.toml [golang] expected_version
│           │   ├── pants.toml
│           │   ├── 3rdparty/go/BUILD
│           │   └── 3rdparty/go/go.sum
│           ├── missing_import_path/           # FR-012 edge: go_third_party_package w/ no go.sum entry
│           │   ├── 3rdparty/go/BUILD
│           │   └── 3rdparty/go/go.sum
│           └── malformed_build_partial/       # FR-009 edge: 3 valid + 1 broken target
│               └── 3rdparty/go/BUILD
```

**Changes to `docs/reference/sbom-format-mapping.md`**:
- Broaden C145 description to include `pkg:golang/*` scope
  emitted by the milestone-226 `pants_go` enrichment pass. Add
  an "Also emitted by m226 on Go modules" sentence to the
  existing row.

**Structure Decision**: Module-directory layout
(`package_db/pants_go/`) matches m225 (`pants_shell/`) verbatim.
Naming: `pants_go` (not `pants_golang`) mirrors the upstream
Pants backend module name (`pants.backend.experimental.go`).

Reader-surface contract:
- `pub fn read(scan_root: &Path, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>`
  at `pants_go/mod.rs`, called from `read_all` (same slot as m225).
  Emits ONLY the design-tier `pkg:generic/go@<version>` component
  when `pants.toml` `[golang] expected_version` is set. Returns
  `Vec::new()` when absent.
- `pub fn enrich(scan_root: &Path, exclude_set: &ExclusionSet, components: &mut Vec<ResolvedComponent>)`
  at `pants_go/mod.rs`, called from `scan_fs/mod.rs` at line
  ~1001 (immediately after m191 reconciler). Walks BUILD files,
  builds ownership index, iterates `components` in place,
  injects `waybill:pants-target` on matching `pkg:golang/*`
  entries. Returns nothing.
- Both entry points check for zero BUILD files + no
  `pants.toml` upfront and early-return silently — FR-011 /
  SC-003 byte-identity guarantee.

Zero fabrication contract (FR-012 / Principle IX): `enrich`
NEVER pushes new components into the vector. Only mutates
`extra_annotations` on existing entries.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated (5 items resolved)
- [x] Phase 1: data-model.md, contracts/*, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS on all 12 principles + C145 doc-only broadening

## Follow-ups (out-of-scope for this branch)

- **`go_source` / `go_test` file-level targets** — Pants prefers
  `go_package`; deferred until operator demand emerges.
- **`min_dot_version` from `pants.toml` `[golang]`** — distinct
  semantic (version-guard lower bound vs pinned toolchain); v1
  emits only `expected_version`.
- **`pants.toml` `[go-test]` / `[go-vet]` / other Go-adjacent
  subsystem sections** — v1 only handles `[golang]`.
- **Plugin-registered custom Go target types** — v1 recognizes
  only the 4 built-in types.
- **Cross-repo Pants workspaces** — per-scan-root scope.
- **Promote BUILD walker + regex extractor to a shared
  `pants_common/` module** — YAGNI now that 2 readers (m225 +
  m226) both use it. If m227 adds Pants Docker or Kotlin, that's
  the trigger to refactor.
