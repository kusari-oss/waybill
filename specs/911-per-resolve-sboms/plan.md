# Implementation Plan: Per-resolve SBOMs for Pants monorepos

**Branch**: `911-per-resolve-sboms` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/911-per-resolve-sboms/spec.md`
**Issue**: [#902](https://github.com/kusari-oss/waybill/issues/902) items 1, 3, 4

## Summary

A Pants repository is several dependency-resolution boundaries wearing one
coat. m868 anchored declared resolves; m910 stopped edges crossing between
them. Three things still block a per-resolve SBOM.

1. **Membership is singular and does not survive dedup.** The relation is
   many-to-many, the annotation holds one name, and dedup keeps the winner's
   value. On the reported monorepo at least 1,147 claims were dropped and four
   resolves ended up empty — invisibly, because empty looks like empty.
2. **A convention-only repository cannot be told from a declaring one.** The
   document counts unanchored lockfiles but does not name them.
3. **Neither split mode partitions a Pants repository**, so every consumer
   reimplements the walk.

The fix: membership becomes a lex-sorted JSON array that unions at dedup; the
document-scope ownership annotation names resolves instead of only counting
them; and `--split=resolve` partitions by **membership filter** rather than by
graph walk.

That last choice is the plan's one real discovery. See Phase 0 R1.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain inherited. No nightly. `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only — `serde`/`serde_json` (array-valued annotations), `std::collections::{BTreeSet, HashMap}` (sorted union, projection index), `clap` (extend the existing `--split` `ValueEnum`), `tracing` (FR-012 diagnostic). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM.
**Testing**: `cargo +stable test --workspace`. Crate-local Pants fixtures for the reader/dedup behaviour; a multi-resolve fixture for the split. Corpus goldens are CI-generated and refreshed once at the end.
**Target Platform**: unchanged — every platform waybill already ships.
**Project Type**: CLI / SBOM emitter. Consumer-visible wire change.
**Performance Goals**: none. A union at dedup and a filter at emit are both linear in components already walked; no new I/O, no subprocess, no network.
**Constraints**: the annotation key is retained, not retired — the catalogue row widens. m868's refusal to anchor glob-discovered lockfiles is preserved. `--split=workspace` and `--split=directory` stay byte-identical.
**Scale/Scope**: three readers emit the annotation (Pex, coursier/JVM, uv-as-Pants-backend — R3); 72 corpus components carry it; one dedup merge site; one new split mode.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| **I. Pure Rust, Statically Linked** | No new dependencies, no C, no FFI. **Pass.** |
| **II. eBPF-Only Observation** | Not engaged — this changes how already-discovered components are described and partitioned, not what is discovered. **N/A.** |
| **III. Fail Closed** | Engaged at FR-012: an unpartitionable repository gets a stated outcome and the single-document fallback, never an empty directory and exit zero. **Pass.** |
| **IV. Type-Driven Correctness** | Membership becomes a genuinely plural value; a sorted set is the type that makes FR-003's determinism and the no-duplicates rule structural rather than test-enforced. **Pass.** |
| **V. Standards-native first** | C143 and C161 are both **KEEP-NO-NATIVE** rows with a recorded audit: no CDX/SPDX carrier exists for "which build-tool resolve owns this". Widening an existing annotation's value does not re-open that audit — no native construct has appeared for a Pants-specific grouping. **Pass.** |
| **VI. Three-Crate Architecture** | No crate boundary moves. **Pass.** |
| **VII. Test Isolation** | Fixtures are crate-local and self-contained; no shared mutable state. **Pass.** |
| **VIII. Completeness** | Directly served. The defect is *silent* incompleteness — four resolves reporting empty that are not. **Pass — this is the principle the feature exists for.** |
| **IX. Accuracy** | Serves it in one direction and must be watched in another: naming discovered resolves is accurate, anchoring them would assert ownership the repository never declared. FR-009 holds the line. **Pass.** |
| **X. Transparency** | FR-007's declared-vs-discovered statement is exactly this principle — telling a consumer what the document can and cannot support. **Pass.** |
| **XI / XII. Enrichment** | Not engaged. **N/A.** |

**No violations. Complexity Tracking omitted.**

One judgement worth recording rather than burying: this change will be
**mis-parsed, not rejected**, by an existing reader of `waybill:pants-resolve`
— it gets an array where it expected a string and carries on. For a downstream
security tool partitioning on that value, a silent mis-parse is worse than a
loud failure. The clarify step accepted that cost on the grounds that this repo
is pre-1.0 and shipped `pkg:generic` → `pkg:pypi` the same way in 0.8.0, but
FR-006b exists so it is announced rather than discovered.

## Project Structure

### Documentation (this feature)

```text
specs/911-per-resolve-sboms/
├── plan.md                        # This file
├── spec.md                        # 18 FRs, 10 SCs, 4 clarifications
├── research.md                    # Phase 0 — R1..R7
├── data-model.md                  # Phase 1 — 5 entities
├── quickstart.md                  # Phase 1 — verification, one section per SC
├── checklists/requirements.md     # 16/16
├── contracts/
│   └── resolve-membership.md      # Phase 1 — C-1..C-6
└── tasks.md                       # Phase 2 — NOT created by /speckit.plan
```

### Source (repository root)

```text
waybill-cli/src/
├── scan_fs/package_db/
│   ├── pants/{lockfile,mod,resolve_classifier}.rs    # writes C143 + C161
│   ├── pants_jvm/{lockfile,resolve_classifier}.rs    # writes C143
│   └── pip/uv_lock.rs                                # writes C143 (Pants backend)
├── resolve/deduplicator.rs                           # per-key union policy
├── generate/
│   └── split.rs                                      # SplitMode::Resolve + filter projection
├── cli/scan_cmd.rs                                   # --split value surface
└── parity/extractors/{mod,cdx,spdx2,spdx3}.rs        # C143/C161 grammar

docs/reference/sbom-format-mapping.md                 # C143 + C161 row text
waybill-cli/tests/fixtures/pants_*/                    # fixtures
```

**Structure Decision**: the work splits cleanly into three layers that map
onto the three user stories — reader/dedup (membership), reader/doc-scope
(provenance), emit (split). The only cross-layer coupling is that the split
consumes membership, which is why US3 depends on US1 and the tasks order
accordingly.

## Phase ordering

1. **US1 (P1) — membership is plural and survives dedup.** The array encoding
   at all three readers, the per-key union at dedup, the catalogue row and
   extractors. Gate: two resolves pinning one package both name it, and two
   scans with perturbed read order agree.
2. **US2 (P2) — declared vs discovered is named.** Extends C161. Independent
   of US1 in code; ordered second because it is worth less on its own.
3. **US3 (P3) — `--split=resolve`.** Depends on US1: the filter has nothing
   correct to filter on until membership is complete. Splitting on today's
   membership would produce a confidently wrong partition.
4. **Golden refresh, once**, after all three. Third consecutive feature to
   churn Pants goldens; per-story refreshes would triple the cost.

## Post-design Constitution re-check

Re-evaluated after Phase 1. No new violations. The one design decision that
arrived during research — partitioning by membership filter rather than by
graph walk (R1) — strengthens Principle VIII rather than straining it: a
filter cannot silently under-report the way a walk from a missing seed can.

The decision that a synthesised split root exists **only inside split output**
(R2, C-5a) is what keeps Principle IX intact; emitting it into the unsplit
document would anchor discovered resolves by the back door and contradict
FR-009.
