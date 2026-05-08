# Implementation Plan: Cargo workspace-member version-disambiguation fix

**Branch**: `087-fix-cargo-workspace-version` | **Date**: 2026-05-08 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/087-fix-cargo-workspace-version/spec.md`

## Summary

Fix the cargo reader's dep-edge resolution when Cargo.lock has multiple `[[package]]` blocks for the same crate name at different versions. Two-part change in two files:

1. **`mikebom-cli/src/scan_fs/package_db/cargo.rs:132-141`** — `package_to_entry` currently strips the version from each `dependencies = [...]` string with `d.split_whitespace().next()`. Replace with version-preserving parsing: keep `"name [version]"` form (strip only the `(source)` suffix). The dep-name string in `PackageDbEntry.depends` becomes either `"name"` (Cargo.lock single-version case) or `"name version"` (multi-version case) — Cargo.lock controls which form is emitted per-dep.
2. **`mikebom-cli/src/scan_fs/mod.rs:373-379`** — extend the cargo branch of `name_to_purl` to insert a SECOND key under `"name version"` form for every cargo entry, mirroring milestone 085's maven `groupId:artifactId` dual-key pattern. Lookups against the disambiguated `"name version"` key resolve correctly even when multiple same-name same-ecosystem entries exist.

Net: ~5-10 LOC in `cargo.rs` + ~12 LOC in `scan_fs/mod.rs` = ~20 LOC code change. Plus regression test bumps in `transitive_parity_cargo.rs` per quickstart Recipe 3 + cargo SPDX/CDX golden regen.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–086; no nightly).
**Primary Dependencies**: existing only — no new crates. The cargo dep-string parsing uses `str::split_whitespace` which we already have. The dual-key insert in `scan_fs/mod.rs` mirrors milestone 085's existing maven pattern.
**Storage**: N/A — purely emission-time identifier resolution.
**Testing**: existing `cdx_regression` / `spdx_regression` / `spdx3_regression` / `cdx_ref_closure_invariant` / `transitive_parity_cargo` test suites + a deliberate baseline bump in `transitive_parity_cargo` per spec FR-007.
**Target Platform**: Linux + macOS — same as the rest of mikebom.
**Project Type**: CLI extension (existing `mikebom-cli` crate).
**Performance Goals**: emission stays within milestone-082 baseline (one extra HashMap insert per cargo entry; same-shape mirror of maven's milestone-085 pattern with no measured perf regression there).
**Constraints**: cargo SPDX 2.3 + SPDX 3 + CDX 1.6 goldens regenerate with diffs containing only the version-string corrections in dep edges — no other field changes. Other ecosystems' goldens (apk/deb/gem/golang/maven/npm/pip/rpm) stay byte-identical.
**Scale/Scope**: ~20 LOC code change in 2 source files + ~15 LOC test addition + 3 cargo goldens (one per format) regenerated + 1 cargo audit-row in `specs/083-transitive-correctness/research.md` updated.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Compliance |
|---|---|
| **I. Pure Rust, Zero C** | ✅ pass — pure Rust diff in `mikebom-cli/src/scan_fs/package_db/cargo.rs` + `scan_fs/mod.rs` |
| **II. eBPF-Only Observation** | N/A — emission-time identifier resolution, not dep discovery |
| **III. Fail Closed** | N/A |
| **IV. Type-Driven Correctness** | ✅ pass — `entry.depends: Vec<String>` shape preserved; the version is encoded in the string per Cargo.lock's own format. A future cleanup could promote the dep-name representation to a typed `(name, Option<version>)` struct for stronger type discipline, but that's a cross-ecosystem refactor outside this milestone's scope. |
| **V. Specification Compliance** | ✅ pass — fixes a per-ecosystem-reader correctness bug surfaced by milestone-083's audit. No `mikebom:*` property introduced. PURL emission MUST conform to spec (`pkg:cargo/<name>@<version>`); both pre-fix and post-fix versions of the PURL strings emit valid PURLs — the bug is which PURL is the target of which edge, not whether the PURL itself is well-formed. |
| **VI. Three-Crate Architecture** | ✅ pass — no new crates |
| **VII. Test Isolation** | ✅ pass — new tests are pure-logic (existing per-ecosystem regression test bumps + a new edge-resolution unit test); no privileges required |
| **VIII. Completeness** | N/A — identifier resolution, not dep discovery |
| **IX. Accuracy** | ✅✅ pass — *this milestone IS an accuracy fix*. Removes phantom edges (`clap@4.5.21 → clap_builder@4.5.9` was a wrong-version edge, never present in any actual build). |
| **X. Transparency** | N/A |
| **XI. Enrichment** | N/A |
| **XII. External Data Source Enrichment** | N/A |

**Strict Boundaries**: all preserved. No lockfile-based dep DISCOVERY (Cargo.lock was already being read for enrichment per Principle XII; this milestone only fixes how the existing read-result is used). No MITM, no C, no `.unwrap()` in production.

**Pre-PR gate** (CLAUDE.md mandatory): the milestone MUST land with both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero errors AND zero warnings) AND `cargo +stable test --workspace` (`0 failed` per suite) green. SC-004 captures this.

**Gate decision**: PASS. No principle violations, no boundary breaches.

## Project Structure

### Documentation (this feature)

```text
specs/087-fix-cargo-workspace-version/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── cargo-version-disambiguation.md  # Phase 1 output — single contract
├── checklists/
│   └── requirements.md  # spec quality checklist (already created)
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

The change is constrained to two files + test scaffolding + golden regen:

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   ├── mod.rs                       # MODIFY — name_to_purl dual-key insert for cargo (~12 LOC; mirrors milestone-085's maven pattern at the same site)
│   │   └── package_db/
│   │       └── cargo.rs                 # MODIFY — package_to_entry preserves version in dep-name strings (~5-10 LOC at line 132-141)
└── tests/
    ├── transitive_parity_cargo.rs       # MODIFY — bump EXPECTED_MIKEBOM_EDGE_COUNT + EXPECTED_REPRESENTATIVE_EDGES per spec FR-007
    └── fixtures/golden/
        ├── cyclonedx/cargo.cdx.json     # REGEN — dep-edge version strings update
        ├── spdx-2.3/cargo.spdx.json     # REGEN — same
        └── spdx-3/cargo.spdx3.json      # REGEN — same
```

**Structure Decision**: Two-source-file fix in `mikebom-cli`. No workspace-level structure changes. The cargo.rs change is localized to `package_to_entry`'s dep-list construction; the scan_fs/mod.rs change is an additional `if ecosystem == "cargo" { ... }` block at the existing dual-key insert site (mirrors the milestone-085 maven block right above it).

## Complexity Tracking

No violations. This section intentionally empty.
