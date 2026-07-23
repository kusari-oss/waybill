# Implementation Plan: Split-mode grouping strategies

**Branch**: `219-split-modes` | **Date**: 2026-07-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/219-split-modes/spec.md`

## Summary

Extend `waybill sbom scan --split` with an optional value argument (`--split[=<mode>]`) accepting `workspace` (default; current m215 behavior) or `directory` (NEW; group all main-modules whose canonicalized source dirs match into ONE sub-SBOM per dir). Internal grouping abstraction is an enum with a `group_key(&SubprojectRoot) -> String` method — adding a future variant (`Ecosystem`, `Owner`, `Custom`) touches only the enum, one match arm, docs, and one test.

Multi-member group emissions land as `<dir-slug>.multi.<format-ext>` filenames (locked at Q2 clarification). `split-manifest.json` gains an additive optional `members: [{purl, source_dir}]` field on each `SplitEntry` — omitted for single-member groups (SC-005 byte-identity for m215 wire shape), present sorted-lex when a group covers ≥2 members (locked at Q1 clarification). Schema URL unchanged.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–218; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `clap` (extend the existing `--split` arg from bool to enum-with-optional-value via `ValueEnum` + `default_missing_value`), `serde` / `serde_json` (SplitManifest additive field with `#[serde(skip_serializing_if = "Option::is_none")]`), `sha2` + `data-encoding` (m215's existing `sha8_hex` helper for slug hashing if needed for collision-safety), `tracing` (FR-010 INFO log). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan. The grouping happens post-`enumerate_workspace_roots`, pre-BFS-projection; the extra grouping table is a `BTreeMap<String, Vec<SubprojectRoot>>` local to `emit_split`.
**Testing**: `cargo +stable test --workspace` — unit tests for the `SplitMode::group_key` method + filename computation + the additive-manifest serde round-trip; integration tests extending `waybill-cli/tests/split_*.rs` with new fixtures (polyglot dir with npm + cargo main-modules; two-dir polyglot fixture; single-member-in-dir compat fixture).
**Target Platform**: linux-x86_64 + macOS + Windows (all three CI lanes; matches every user-space milestone since m100 Windows-host-build). No eBPF surface touched.
**Project Type**: CLI-flag extension + resolver-shape addition + split-manifest schema evolution. Single crate touched (`waybill-cli`); split-manifest lives in `waybill-cli/src/generate/split_manifest.rs`.
**Performance Goals**: Grouping is O(N) over `Vec<SubprojectRoot>` where N is typically ≤ ~50 for the largest monorepos (m215's `enumerate_workspace_roots` output). Grouping cost is trivially small against the existing BFS-projection cost (dominated by `all_relationships` iteration at ~O(N × E)).
**Constraints**: SC-005 byte-identity contract MUST hold — bare `--split` and `--split=workspace` produce output byte-identical to alpha.67 on every existing m215 test fixture. FR-006 filename `<dir-slug>.multi.<format-ext>` for multi-member groups (Q2 lock). FR-005 additive-optional `members` field, omitted for single-member (Q1 lock).
**Scale/Scope**: New CLI shape: 1 flag with 2 accepted values (`workspace`, `directory`). New enum: `SplitMode` with 2 variants. New emission path: multi-member group merge + `.multi` filename. New schema field: `SplitEntry.members: Option<Vec<SplitMember>>`. New fixture: 1-3 polyglot dirs.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Waybill Constitution v2.0.0 principles evaluated against this milestone:

- **I. Pure Rust, Zero C** — ✅ No C. New code is stdlib + workspace deps.
- **II. eBPF-Only Observation** — ✅ N/A. This is a post-resolve emit-time transformation over already-observed component sets.
- **III. Fail Closed** — ✅ Invalid `--split=<mode>` value → CLI parse error → non-zero exit (per FR-008). Zero-boundaries fallback (FR-009) preserves the m215 WARN-and-emit-one contract.
- **IV. Type-Driven Correctness** — ✅ `SplitMode` is a new enum (not stringly-typed); `clap::ValueEnum` derive validates at parse time. No `.unwrap()` in production; test `.unwrap()` guarded per convention.
- **V. Specification Compliance** — ✅ Standards-native audit N/A — this milestone changes only waybill-internal artifacts (CLI flag + `split-manifest.json` shape); neither introduces a new `waybill:*` annotation nor touches CDX/SPDX 2.3/SPDX 3 emission paths.
- **VI. Three-Crate Architecture** — ✅ Only `waybill-cli` touched. `waybill-common` untouched. `waybill-ebpf` untouched.
- **VII. Test Isolation** — ✅ New tests are unprivileged (no eBPF, no root). Runs under `cargo test --workspace` in every CI lane.
- **VIII. Completeness** — ✅ Directory-mode grouping doesn't drop components or edges; it MERGES per-member projections into a union per group. FR-004 dedup preserves every component and relationship the m215 workspace-mode would have emitted (just co-located instead of split).
- **IX. Accuracy** — ✅ No new inference. Grouping is a pure post-hoc rearrangement of already-resolved components.
- **X. Transparency** — ✅ FR-010 INFO log emits `mode=<mode> groups=<N> total_main_modules=<M>` at split-driver exit. `split-manifest.json`'s `members[]` field is the operator-facing signal for multi-member groups.
- **XI. Enrichment** — ✅ N/A (no external data source).
- **XII. External Data Source Enrichment** — ✅ N/A (in-process rearrangement of scan-local data).

**Constitution check result: PASS.** No violations. No amendment required.

## Project Structure

### Documentation (this feature)

```text
specs/219-split-modes/
├── plan.md              # This file
├── spec.md              # Feature specification (committed 52a577b + f4bbe21)
├── research.md          # Phase 0 output (this /speckit-plan pass)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── split-mode-flag.md          # CLI flag surface + clap ValueEnum shape
│   ├── grouping-strategy.md        # SplitMode enum + group_key() contract (extensibility)
│   ├── multi-member-filename.md    # `<dir-slug>.multi.<format-ext>` convention
│   └── manifest-additive-members.md # SplitEntry.members additive-optional schema
├── checklists/
│   └── requirements.md              # Spec quality checklist (committed 52a577b)
└── tasks.md             # Phase 2 output — /speckit-tasks
```

### Source Code (repository root)

Single-crate (`waybill-cli`) touch; `waybill-common` + `waybill-ebpf` stay byte-identical.

```text
waybill-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs                                          # extend --split from `bool` to `Option<SplitMode>` (ValueEnum + default_missing_value); update Default impl
│   ├── generate/
│   │   ├── split.rs                                             # + SplitMode enum + group_key(); NEW GroupedProjection type; refactor emit_split to group before BFS; extend filename_for for `.multi` shape
│   │   └── split_manifest.rs                                    # + SplitMember { purl, source_dir } struct; + SplitEntry.members: Option<Vec<SplitMember>> (additive-optional via #[serde(skip_serializing_if)])
│   └── (nothing else)
└── tests/
    ├── split_*.rs                                               # SC-005 byte-identity guard — existing m215 tests MUST pass unchanged
    ├── split_modes.rs                                           # NEW — 5+ scenarios per US1/US2 + SC-005/SC-006/SC-007
    └── fixtures/
        └── split_modes/                                         # NEW — polyglot fixtures
            ├── two_dir_polyglot/                                #   services/api/{Cargo.toml, package.json} + services/worker/{go.mod}
            └── single_dir_polyglot/                             #   <root>/{Gemfile, package.json} (proves empty-source_dir merge)

docs/
└── reference/
    └── split-modes.md                                          # NEW page — 6 sections per R3 template (matches m218 shape); linked from README
```

**Structure Decision**: Extend `waybill-cli/src/generate/split.rs` inline with the `SplitMode` enum + `GroupedProjection` type + a new `group_roots(&[SubprojectRoot], SplitMode) -> Vec<GroupedProjection>` helper. `emit_split` refactors: group → merge-per-group BFS-projections → emit per group. `SplitEntry` schema evolution lives in `split_manifest.rs` as an additive-optional field. Filename computation extends `filename_for` with a new branch that fires when a group covers ≥2 members. CLI flag rewrite in `scan_cmd.rs` uses `clap::ValueEnum` with `default_missing_value` so `--split` bare parses as `Workspace`.

## Complexity Tracking

> No constitution violations. Complexity table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
