# Implementation Plan: bun.lock transitive-edge emission

**Branch**: `667-bun-lock-edges` | **Date**: 2026-08-27 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/667-bun-lock-edges/spec.md`
**Closes**: [#723](https://github.com/kusari-oss/waybill/issues/723)

## Summary

Extend the bun.lock reader at `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs` with a **two-pass edge-emission phase** that reads each `packages[K][2]` metadata object, resolves each declared dep-name via a **scope-aware key-path walker** (most-specific-prefix wins, matching bun's node_modules install-chain semantics), and populates each parent entry's `PackageDbEntry.depends` field with disambiguation strings the downstream graph builder already knows how to consume.

**Semantic clarification (surfaced during Phase 0 research)**: the spec's Q1 clarification (2026-08-27) said "populate `depends` with target PURLs directly." The actual codebase convention for npm-family multi-version disambiguation is **`"<name> <version>"` strings** (per m087 issue #172 for cargo, m147 issue #262 for npm). The graph builder at `scan_fs/mod.rs:635-644` builds a secondary `name_to_purl` key `(ecosystem, "<name> <version>")` explicitly for this shape and picks the correct version copy at edge-emission time. This plan **matches that convention exactly** — Option A in spirit (single-mechanism, no new emit-time hook), but populating `<name> <version>` strings rather than raw PURLs. Spec's FR-004 should be tightened by the plan phase; see Phase 0 R1 for the correction.

Everything else in the fix (four dep-section walker, `LifecycleScope::Optional` on optional/optional-peers, `waybill:optional-derivation = "bun-optional-dependencies"` / `"bun-optional-peers"` annotations, warn-and-continue on malformed input) mirrors the m180/m147 patterns 1:1.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–666; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde_json` (already used pervasively; parses the JSONC-stripped `bun.lock`), `std::collections::{HashMap, BTreeMap}` (stdlib; two-pass key-path→disambiguator lookup + deterministic edge-map dedup), `tracing` (FR-011 warn-on-drop), `waybill_common::resolution::{LifecycleScope, RelationshipType}` (workspace types; reused verbatim from m179/m180). **No new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; per-reader HashMap for pass-1 key→disambiguator, per-entry BTreeMap for pass-2 depends-set dedup.
**Testing**: `cargo +stable test -p waybill --bin waybill -- scan_fs::package_db::npm::bun_lock` for reader unit tests. Integration coverage via a checked-in fixture at `waybill-cli/tests/fixtures/bun_lock/` (new subdir; no sibling-repo push required per m665 precedent).
**Target Platform**: All release-lane targets. bun.lock is JSONC and the resolver is pure-Rust; zero platform-specific paths.
**Project Type**: Ecosystem-reader extension (single-file production change + fixture-based integration test + doc caveat).
**Performance Goals**: Parse-time only; no runtime perf impact. Reader pass-2 is `O(N × avg-tree-depth × avg-deps-per-package)` where N ≈ lockfile package count. For the reporter's 1177-package monorepo with avg-depth ~4 and avg-deps ~8: ~38k lookups × ~50 ns per HashMap probe = ~2 ms. Negligible vs the m664 shared-walker traversal cost.
**Constraints**: FR-007 zero regressions on m106 workspace behavior. FR-008 zero net component-set change. FR-009 no touch on `packages`-map component-emission loop (`bun_lock.rs:205-279`). SC-006 workspace-test count MUST equal m665's 5192 baseline + N new (from SC-001/SC-004/SC-005 unit tests + 1 integration test).
**Scale/Scope**: Real fixture is 1177 non-workspace packages / 78 workspace members; the resolver handles bun 1.2+ lockfile shape. Non-hoisted key paths reach ~5 segments deep on the reporter's fixture (201 non-hoisted copies).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

All 12 principles evaluated. Every principle either does not apply to reader-only fixes or is satisfied by this fix.

| # | Principle | Applies? | Verdict |
|---|-----------|----------|---------|
| I | Pure Rust, Zero C | N/A — reader-only, no language-stack changes | PASS |
| II | eBPF-Only Observation | N/A — filesystem-reader path; bun.lock is a manifest/enrichment input, not a discovery source. Per Principle XII, lockfile-derived edges are **enrichment**: they add relationships between components that already exist in the emitted set (FR-008). The reader NEVER synthesizes components from lockfile-only presence. | PASS (fix stays in enrichment lane) |
| III | Fail Closed | N/A — no eBPF trace, no runtime behavior change. Reader FR-010 warn-and-continue posture matches m106's existing shape. | PASS |
| IV | Type-Driven Correctness | Reused types (`LifecycleScope::Optional`, `RelationshipType::OptionalDependsOn`, `PackageDbEntry.depends: Vec<String>`) already are newtype/enum-wrapped domain values. No new types introduced. Production code's `.unwrap()` ban applies; the reader will use `?` propagation on `serde_json::Value` accessor chains and `unwrap_or_default()` on Option-returning `.as_object()` / `.as_str()` calls (same posture as pre-fix `bun_lock.rs`). | PASS |
| V | Specification Compliance | The fix touches no SBOM-emission code paths; it only changes what the reader hands to the emitters. Downstream CDX 1.6 / SPDX 2.3 / SPDX 3 emission machinery already handles `LifecycleScope::Optional` + `RelationshipType::OptionalDependsOn` per m179/m180 (`scope: "optional"` in CDX, `OPTIONAL_DEPENDENCY_OF` in SPDX 2.3, `LifecycleScope::Optional` on SPDX 3 Relationship). Standards-native precedence audit: **the fix uses only pre-existing native-carrier mechanisms**; the m180 pattern the fix mirrors already documents the standards-native-precedence audit trail (see `docs/reference/sbom-format-mapping.md` C42 optional-derivation row + C122 optional-target row). No new `waybill:*` annotation invented. | PASS |
| VI | Three-Crate Architecture | N/A — no crate structure change. Fix lives entirely inside `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`. | PASS |
| VII | Test Isolation | Reader unit tests are `#[cfg(test)]` blocks inside `bun_lock.rs`; no eBPF privilege dependency. The fixture-based integration test lands at `waybill-cli/tests/fixtures/bun_lock/` (new subdir) and runs under standard `cargo test`. | PASS |
| VIII | Completeness | **This principle is the fix's raison d'être.** Pre-fix state VIOLATES Principle VIII: real components ship in the trace (their lockfile presence proves they were fetched during build) but the SBOM omits their relationships, mis-classifying them as `hoisted-unused` and inverting the triage signal. The fix RESTORES conformance by ensuring every lockfile-declared parent→child edge lands in the emitted graph. Zero false negatives on lockfile-declared edges post-fix. | **PASS (fix restores compliance)** |
| IX | Accuracy | The multi-version-integrity requirement (FR-005, SC-004) exists precisely to prevent phantom edges from name-collision misrouting. The scope-aware resolver picks the CORRECT version copy per bun's install-chain semantics; no over-emission of `child-v2` when the parent's install chain actually pulled `child-v1`. | PASS |
| X | Transparency | FR-011 warn-and-drop with reason strings for every unresolvable edge. Operators can grep `grep 'bun.lock edge dropped' scan.log` to triage lockfile inconsistencies without needing SBOM-emitter internals. | PASS |
| XI | Enrichment | The fix IS a completeness-side enrichment — augmenting the edge set on already-observed components. Compatible with Principle XI's "enrich when data is available without violating accuracy" posture. | PASS |
| XII | External Data Source Enrichment | **Directly relevant.** Principle XII bullet 1: "External sources MUST NOT introduce new components." Bullet 2: "Data from external sources MUST be annotated with its provenance." Bullet 3: "External source unavailability MUST NOT prevent SBOM generation." Bullet 4: "External sources provide context, not authority." The fix satisfies all four: (a) FR-008 no new components; (b) `PackageDbEntry.source_path` already carries `<rootfs>/bun.lock` per pre-fix reader → the enriched edges inherit that provenance in the graph builder's `Relationship.provenance` field; (c) FR-010 warn-and-continue on malformed input; (d) lockfile-derived edges DEPENDS_ON between already-emitted components, never creating new discovery. | PASS |

**No constitution violations to justify.** Complexity tracking section stays empty.

## Project Structure

### Documentation (this feature)

```text
specs/667-bun-lock-edges/
├── plan.md                     # This file
├── research.md                 # Phase 0 output — architectural + convention decisions
├── data-model.md               # Phase 1 output — resolver + two-pass shape
├── contracts/
│   └── depends-emission.md     # Phase 1 output — reader→graph-builder contract
├── quickstart.md               # Phase 1 output — "how to reproduce the fix's outcome"
├── checklists/
│   └── requirements.md         # Spec-quality checklist (pre-existing)
└── tasks.md                    # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/npm/
├── bun_lock.rs                 # ← Primary fix site. Two-pass edge emission
│                               #   added after pre-fix line 279 (post-components
│                               #   emission). ~150 LOC net addition (resolver
│                               #   fn + two-pass loop + tests).
├── package_lock.rs             # Unchanged. Provides the convention this fix
│                               #   mirrors (m147 issue #262 `<name> <version>`
│                               #   disambiguation).
├── pnpm_lock.rs                # Unchanged.
├── mod.rs                      # Unchanged. Reader-dispatch entry.
└── walk.rs                     # Unchanged.

waybill-cli/tests/fixtures/bun_lock/    # ← New fixture subdir.
├── minimal_repro/              # Issue #723's 2-file repro verbatim.
│   ├── package.json
│   └── bun.lock
├── multi_version/              # SC-004 fixture — same name at 2 versions
│   │                           #   under different parent key paths.
│   ├── package.json
│   └── bun.lock
├── scoped_name/                # SC-005 fixture — scoped-name resolver walk.
│   ├── package.json
│   └── bun.lock
└── optional_deps/              # US1 scenario 3 fixture — optional +
    │                           #   optional-peers → LifecycleScope::Optional.
    ├── package.json
    └── bun.lock

waybill-cli/tests/
└── bun_lock_edges_us1.rs       # New integration test — runs waybill against
                                #   each fixture, greps emitted CDX for the
                                #   expected edges + orphan-reason absence.

docs/
└── ecosystems.md               # ← FR-012 doc update site. npm-row Dep-graph
                                #   column caveat.
```

**Structure Decision**: Single-production-file change (`bun_lock.rs`) + fixture subdir + integration test + doc caveat. FR-006's "reuse existing types" constraint + FR-007's "no workspace regression" constraint all point at scoping the fix to `bun_lock.rs` alone.

## Complexity Tracking

> No constitution violations to justify. This section is intentionally empty.
