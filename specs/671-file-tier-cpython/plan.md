# Implementation Plan: File-tier surfacing for source-heavy trees (SC-003 follow-up)

**Branch**: `671-file-tier-cpython` | **Date**: 2026-09-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/671-file-tier-cpython/spec.md`

## Summary

Milestone 670 met SC-001 (markitdown) + SC-002 (OctoPrint) but left SC-003 (cpython ≥ 50 pypi components) unmet — cpython legitimately consumes ~11 unique PyPI deps in principle. This milestone reframes SC-003 as `cpython ≥ 100 file-tier components` and delivers it by extending the existing m133 `--file-inventory=<mode>` flag with a new `source-tree` value that surfaces source-code file extensions (`.py`, `.c`, `.h`, etc.) as file-tier components. Backward-compat is a hard invariant: the DEFAULT `--file-inventory=orphan` path stays byte-identical to v0.5.0, verified by the existing 6 golden test suites.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–670; no nightly required for this user-space-only work).

**Primary Dependencies**:

- `globset = "0.4"` — already a direct workspace dep since m113; reused verbatim for path-exclusion + extension-restriction matching
- `sha2` — already pervasive; unchanged
- `serde` / `serde_json` — annotation values
- `clap` — the two new flag surfaces (`--file-inventory=source-tree` value + `--file-inventory-source-shapes=<list>` companion) via `Args`-derive
- `tracing` — the extended `file_tier walker complete` log line + a new mode-activation diagnostic
- `anyhow` / `thiserror` — CLI-parse-fail path for FR-009 loud-fail on unknown extension

**Zero new Cargo dependencies.** The mechanism reuses m133's existing walker + hasher + dedupe + `ContentShape::classify` verbatim; only the extension-hard-exclusion list needs a mode-gated bypass.

**Storage**: N/A — all state in-process per scan; matches every ecosystem-reader milestone since 002.

**Testing**:
- `cargo test --workspace` — existing convention
- 4 new inline unit tests under `content_shape.rs::tests` (source-tree classification with subset restriction + FR-009 CLI-parse-fail)
- 3 new integration tests under `waybill-cli/tests/scan_file_tier_source_tree_m671.rs`: cpython-shape synthetic fixture (`.py` + `.c` + `.h` mix), Python-only restriction, default-mode byte-identity guard
- 6 existing golden test suites (cdx_regression, spdx_regression, spdx3_regression, pkg_alias_binding_us1, oci_pull_backward_compat, optional_dep_classification) MUST pass without regen (SC-004)
- 21-fixture kusari-sandbox sweep regression: default-mode component-counts within ± 1% of v0.5.0 baseline (SC-003)

**Target Platform**: Linux + macOS + Windows (matches milestone-100 host-portability posture); no filesystem semantics diverge across hosts. Extension matching is case-insensitive (matches m133's existing behavior for `EXCLUDED_EXTENSIONS`).

**Project Type**: CLI tool — single Cargo workspace (`waybill-cli` / `waybill-common` / `waybill-ebpf`) per constitutional Three-Crate Architecture (Principle VI). This milestone touches only `waybill-cli/src/scan_fs/file_tier/` + `waybill-cli/src/cli/scan_cmd.rs` (CLI flag wiring) + `waybill-cli/src/parity/extractors/` (one new catalog row).

**Performance Goals**:
- cpython scan wall-clock ≤ 1160 ms under new mode (2× the 580 ms v0.5.0 default-mode baseline; SC-005). Additional cost is SHA-256 over ~3400 previously-shape-skipped files.
- Default-mode wall-clock unchanged (± noise). Only the mode-gated code path adds work.

**Constraints**:
- **Principle VIII (Completeness)** — motivates the milestone. Unattributed source content should surface under an operator-opt-in signal.
- **Principle V (Specification Compliance)** — one new `waybill:file-inventory-source-shapes-active` annotation. Native-alternative audit: CDX `metadata.properties[]` is the standards-native carrier for scan-mode metadata (same as C153 `waybill:binary-scan-suppressed`); this catalog row follows the C153 pattern verbatim.
- **Principle IV** — no `.unwrap()` in production; CLI-parse-fail path uses `thiserror` + `anyhow` boundary.
- **Strict Boundary #5** — file-tier default-mode MUST NOT introduce duplicate components. This milestone EXTENDS SB#5 to "default-mode MUST NOT introduce inflation either." FR-007 makes this a hard invariant; SC-003 + SC-004 gate it in test.
- No network access at scan time.

**Scale/Scope**:
- 13 functional requirements across 3 user stories
- ~150–200 LoC in `content_shape.rs` + a new `source_shape.rs` sibling module + CLI-arg parser
- 1 new parity-catalog row (C156)
- 7 new tests (4 unit + 3 integration)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Reviewed against all 12 principles + 5 Strict Boundaries at Waybill Constitution v2.1.0:

| Principle | Status | Notes |
|-----------|--------|-------|
| I — Pure Rust, Zero C | **PASS** | No new C code, no FFI. Extension matching is stdlib string ops. |
| II — eBPF-Only Observation | **DIVERGENT** (inherited) | This is the filesystem-scan reader path — same rationale as m670 Complexity Tracking; the `sbom scan` command family has diverged from strict eBPF-only observation since m002. |
| III — Fail Closed | **PASS** | FR-009 fails LOUDLY on invalid extension (parse-time error). No silent-skip fallback. |
| IV — Type-Driven Correctness | **PASS** | New `SourceShape` enum + `FileInventoryMode::SourceTree { restriction: Option<SourceShapeSet> }` variant. Extension matching via typed `SourceShape` values. No raw `String` passing across boundaries for source shapes. No `.unwrap()` in production. |
| V — Specification Compliance | **PASS** | One new `waybill:file-inventory-source-shapes-active` annotation (C156). Principle V bullet-5 audit: no standards-native carrier for "which source-shape restriction subset was active during scan" — CDX `metadata.properties[]` is the vehicle, matches C153 `waybill:binary-scan-suppressed` shape verbatim. |
| VI — Three-Crate Architecture | **PASS** | Changes confined to `waybill-cli`. No new crates. |
| VII — Test Isolation | **PASS** | All new tests unit + integration under `cargo test --workspace`; no eBPF privileges required. |
| VIII — Completeness | **PASS** — directly addresses the SC-003 completeness gap. The new mode is an operator-opt-in expansion of the orphan-fallback surface. Unattributed source content becomes emittable. |
| IX — Accuracy | **PASS** | No new components fabricated. Every file-tier component still traces to a real file on disk with a real SHA-256 hash + observable path. |
| X — Transparency | **PASS** | New `waybill:file-inventory-source-shapes-active` doc-scope annotation names the mode + any restriction. FR-011 log line unchanged. |
| XI — Enrichment | **N/A** | This milestone is discovery-side; no enrichment concerns. |
| XII — External Data Source Enrichment | **N/A** | No external data sources touched. |
| **SB#1** — No lockfile-based dependency discovery | **PASS** | File-tier components carry no PURL; not dependency-discovery. |
| SB#2 — No MITM proxy | **PASS** | No network activity. |
| SB#3 — No C code | **PASS** | Pure Rust. |
| SB#4 — No `.unwrap()` in production | **PASS** | See Principle IV. |
| SB#5 — No file-tier duplicates in default mode | **PASS** | FR-007 + SC-003 + SC-004 make default-mode byte-identity a hard invariant. Existing m133 FR-011 hybrid dedupe applied verbatim to the new mode; new mode is opt-in only. |

**Divergence flagged (Principle II)** — inherited from m670 Complexity Tracking; unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/671-file-tier-cpython/
├── plan.md                # This file
├── research.md            # Phase 0 output
├── data-model.md          # Phase 1 output
├── quickstart.md          # Phase 1 output
├── contracts/             # Phase 1 output
│   ├── README.md
│   ├── file_inventory_mode.md
│   └── source_shape_restriction.md
├── checklists/
│   └── requirements.md    # Existing (3/3 clarifications complete)
└── tasks.md               # Deferred to /speckit.tasks
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs                # existing; add `--file-inventory-source-shapes` arg + FR-001 wiring
│   ├── scan_fs/
│   │   └── file_tier/
│   │       ├── mod.rs                 # existing; extend `FileInventoryMode` enum with SourceTree variant
│   │       ├── content_shape.rs       # existing; extend `classify()` with mode-gated bypass for FR-002 shapes
│   │       ├── walker.rs              # existing; pass-through the mode to `classify()`
│   │       └── source_shape.rs        # NEW — `SourceShape` enum + `parse_shape_restriction()` (FR-009 loud-fail)
│   └── parity/
│       └── extractors/                # existing; add C156 row across cdx/spdx2/spdx3
├── tests/
│   ├── scan_file_tier_source_tree_m671.rs   # NEW — 3 integration tests (SC-001, SC-004, SC-006)
│   └── fixtures/                            # NO new fixtures on disk — tests use tempfile::tempdir() with inline `.py`/`.c`/`.h` files (matches m670 T007 synthetic-fixture posture)
docs/reference/
├── sbom-format-mapping.md             # existing; add C156 row after C155
└── component-tiers.md                 # existing; document the new SourceTree mode + shape list
```

**Structure Decision**: The mode is added as a new variant on the existing `FileInventoryMode` enum in `waybill-cli/src/scan_fs/file_tier/mod.rs` — matches m133's established pattern. The 21-extension FR-002 allowlist lives in a new `source_shape.rs` sibling module so it stays reviewable independently of the existing `EXCLUDED_EXTENSIONS` list at `content_shape.rs:92`. The FR-009 CLI-parse-fail path uses a `thiserror` error type inside `source_shape::parse_restriction()` and surfaces via a `clap` value-parser fail on the `--file-inventory-source-shapes` argument.

## Complexity Tracking

Inherited from milestone 670 — Principle II divergence for the `sbom scan` command family. **No NEW divergences introduced by this milestone.**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle II — eBPF-Only Observation | Filesystem-scan is the ONLY viable path for `sbom scan`. File-tier emission on unattributed source content is a Principle-VIII completeness signal, not eBPF-observed dependency discovery. | Requiring eBPF-only would eliminate the entire `sbom scan` command family (m002+). Not a viable alternative. |
