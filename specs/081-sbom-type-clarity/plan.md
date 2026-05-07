# Implementation Plan: SBOM-type signaling clarity

**Branch**: `081-sbom-type-clarity` | **Date**: 2026-05-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/081-sbom-type-clarity/spec.md`

## Summary

Audit-first milestone exploring whether mikebom's existing milestone-047 lifecycle infrastructure adequately signals "what type of SBOM is being delivered" per the CISA SBOM Types framework (Design / Source / Build / Analyzed / Deployed / Runtime). The Phase 0 audit (already executed during /speckit.plan and consolidated below) reveals:

- **CDX 1.6**: Already clean. Native `metadata.lifecycles[]` wired by milestone 047 from per-component `mikebom:sbom-tier` annotations. No work needed.
- **SPDX 2.3**: Already clean. No native single-document-type enum exists in the spec; mikebom's existing `creationInfo.comment` aggregation is the appropriate Principle V escape clause (no native field to promote to).
- **SPDX 3**: **Real Principle V gap surfaced**. `software_Sbom.software_sbomType` is an array of 6-value enum entries (`{analyzed, build, deployed, design, runtime, source}` — exactly the 6 CISA SBOM Types) which mikebom does NOT currently emit. The aggregated tier set is currently flowing into `comment` only. **This milestone wires the native field** per Constitution Principle V's standards-native-precedence requirement.

Plus operator-facing documentation (`docs/reference/sbom-types.md`) per US1 and the mixed-tier transparent presentation rule per the 2026-05-07 Q1 clarification. Operator self-assertion (US3 `--sbom-type` flag) is included because the audit-confirmed SPDX 3 native field promotion makes the operator-assert use case more concrete (operators producing single-type compliance SBOMs need a clean way to override aggregation). The `runtime` tier auto-detection (original spec US4) is **deferred** — mikebom's eBPF trace observes the BUILD process producing artifacts, not the runtime of those artifacts, so eBPF-traced SBOMs remain `build` tier per CISA semantics. The `--sbom-type` flag accepts `runtime` as a valid value (so operators with their own runtime-instrumentation pipelines can assert it), but mikebom's existing scan modes do NOT auto-detect `runtime` in this milestone.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–080; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (JSON-LD round-tripping), `tracing`, `anyhow`, `clap` (the new `--sbom-type` flag via derive). Reuses milestone-047's `lifecycle_phases.rs::aggregate_phases` helper as the source of truth for tier aggregation. Reuses milestone-078's `spdx3-validate==0.0.5` conformance gate. **No new Cargo dependencies.**
**Storage**: N/A — pure metadata-emission transform. No caches, no persistence.
**Testing**: `cargo +stable test --workspace` continues as the primary gate. Adds new integration tests in `mikebom-cli/tests/sbom_type_signaling.rs` (or extends `triple_format_perf.rs`) covering: SPDX 3 `software_Sbom.software_sbomType[]` emission across all tier permutations; CDX `metadata.lifecycles[]` continues to emit unchanged; operator-assert `--sbom-type` overrides aggregation in all three formats; per-component `mikebom:sbom-tier` annotations preserve auto-detected values when `--sbom-type` is asserted; schema validation passes for all three formats post-emission; SPDX 3 SHACL conformance gate from milestone 078 continues to pass.
**Target Platform**: Linux (CI primary), macOS (developer workstations). Pure user-space CLI + emission code.
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Negligible per-emission overhead. The SPDX 3 native-field wiring adds O(N_phases) string conversions (N_phases is bounded to 6) per emission. Total emission wall-time impact: <1ms.
**Constraints**: Determinism per the existing milestone-047 `BTreeSet`-backed aggregation order (lexicographic). All emitted SBOMs MUST continue to pass schema validation (CDX 1.6 schema, SPDX 2.3 schema, SPDX 3 schema, AND milestone 078's `spdx3-validate` SHACL gate). Per Constitution Principle V's amendment requirement: the audit-record entry in `docs/reference/sbom-format-mapping.md` cites that SPDX 3's `software_Sbom.software_sbomType` was promoted from a `comment`-aggregation parity bridge to a native field in this milestone.
**Scale/Scope**: Small-to-medium milestone. ~30 LOC for the SPDX 3 native-field wiring (extends `lifecycle_phases.rs` with a `tier_to_spdx3_sbomtype_iri` helper + wires it at `mikebom-cli/src/generate/spdx/v3_document.rs::build_spdx_document`'s SpdxDocument construction site). ~80 LOC for the `--sbom-type` flag (CLI definition + override logic in `lifecycle_phases.rs`). ~150 LOC for `mikebom-cli/tests/sbom_type_signaling.rs`. ~150 LOC for `docs/reference/sbom-types.md`. SPDX 3 byte-identity goldens regenerate as the expected operator-visible change of the milestone (analogous to milestones 077/078/079); CDX 1.6 + SPDX 2.3 byte-identity goldens stay byte-identical (no emission change for those formats).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All 12 principles + 4 strict boundaries reviewed:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | All Rust changes inside `mikebom-cli`. No C, no FFI. No new dependencies. |
| II. eBPF-Only Observation | ✅ Pass / N/A | This milestone touches metadata emission and CLI surface; eBPF trace path unchanged. The audit explicitly defers `runtime` tier auto-detection BECAUSE mikebom's eBPF observes builds, not runtime — preserving Principle II's discovery semantic. |
| III. Fail Closed | ✅ Pass | `--sbom-type` validation rejects unknown vocab values at parse time. No silent fallback. |
| IV. Type-Driven Correctness | ✅ Pass | New types: `SbomType` enum with 6 variants (`Design`/`Source`/`Build`/`Analyzed`/`Deployed`/`Runtime`); `tier_to_spdx3_sbomtype_iri` pure function. Production code uses `anyhow::Result`. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md. No raw-`String` boundary crossings beyond CLI input. |
| V. Specification Compliance | ✅ Pass | **THIS MILESTONE IS THE PRINCIPLE V AUDIT** for SBOM-type signaling. The Phase 0 §1 audit cited in the spec confirms: CDX 1.6 native usage already correct (no `mikebom:` bridge for SBOM type); SPDX 2.3 has no native field (escape clause appropriate; documented); **SPDX 3 has `software_Sbom.software_sbomType` natively which mikebom was NOT using** — the milestone fixes this by wiring the native field. The audit-record entry in `docs/reference/sbom-format-mapping.md` documents both the positive outcome (CDX clean) and the correction (SPDX 3 native field promotion from `comment`-only aggregation to native + `comment` for backwards compatibility). Per the v1.4.0 amendment requirement, spec authors cite the audit result in FRs (FR-002 + FR-008); reviewers can verify the audit outcome. |
| VI. Three-Crate Architecture | ✅ Pass | All Rust changes inside `mikebom-cli`. No new crates. |
| VII. Test Isolation | ✅ Pass | All tests run without elevated privileges. No eBPF code touched. Reuses milestone 078's graceful-skip + CI strict-mode pattern. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | The mixed-tier "Mixed-type SBOM" presentation rule (per Q1 clarification) preserves accuracy — no dominant-tier heuristic invented. SPDX 3 native field is a multi-element array, faithfully representing the actual tier mix. |
| X. Transparency | ✅ Pass | The new `docs/reference/sbom-types.md` makes mikebom's SBOM-type signaling discoverable for the first time. Operators can now read the data lineage of any mikebom-emitted SBOM via documented `jq` recipes per FR-001(c). |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | Not external-source data. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — extending production code that already complies; tests use the standard guard |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed. Principle V audit is the milestone's central deliverable; SPDX 3 native field promotion closes the surfaced gap.

## Project Structure

### Documentation (this feature)

```text
specs/081-sbom-type-clarity/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify + /speckit.clarify output (Q1 integrated)
├── research.md                     # Phase 0 — per-format audit + decision matrix
├── data-model.md                   # Phase 1 — SbomType enum + tier_to_spdx3_sbomtype_iri helper
├── quickstart.md                   # Phase 1 — operator-facing recipes (replaces inline source-code grep)
├── contracts/
│   └── sbom-type-signaling.md      # Phase 1 — wire-format contract per format + --sbom-type CLI surface
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches the existing `lifecycle_phases.rs` helper, the SPDX 3 emission path, the CLI definitions on two existing subcommands, the new operator-facing doc, and the audit-record doc. CDX 1.6 + SPDX 2.3 emission paths are NOT touched.

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   ├── scan_cmd.rs                    # MODIFY (~30 LOC) — add `--sbom-type` flag
│   │   │                                    # via clap derive on Scan struct: `sbom_type:
│   │   │                                    # Option<SbomType>` with custom value_parser that
│   │   │                                    # accepts {design,source,build,analyzed,deployed,runtime}
│   │   │                                    # and rejects others with crisp error message.
│   │   └── run.rs                         # MODIFY (~30 LOC) — symmetric flag on Run struct.
│   └── generate/
│       ├── lifecycle_phases.rs            # MODIFY (~50 LOC) — extend with:
│       │                                    # (a) SbomType enum (6 variants matching CISA);
│       │                                    # (b) tier_to_spdx3_sbomtype_iri(tier) -> Option<&'static str>
│       │                                    #     returning IRIs like "spdx:Software/SbomType/source";
│       │                                    # (c) aggregate_spdx3_sbom_types(components) -> Vec<&'static str>
│       │                                    #     mirroring aggregate_phases pattern;
│       │                                    # (d) optional override hook: when UserMetadata or scan-args
│       │                                    #     carries an asserted sbom_type, the aggregator returns
│       │                                    #     a single-element Vec with the asserted IRI instead.
│       └── spdx/
│           └── v3_document.rs             # MODIFY (~20 LOC) — at SpdxDocument construction site,
│                                            # call aggregate_spdx3_sbom_types and add to the
│                                            # software_Sbom element as `software_sbomType:
│                                            # [<iri>, ...]` array. Element ordering matches
│                                            # existing v3_document.rs sort key conventions.
└── tests/
    └── sbom_type_signaling.rs             # NEW (~150 LOC) — integration tests:
                                              # 1. spdx3_sbomtype_emitted_natively_for_source_tier
                                              # 2. spdx3_sbomtype_emitted_natively_for_build_tier
                                              # 3. spdx3_sbomtype_aggregates_mixed_tiers
                                              # 4. cdx_lifecycles_unchanged_from_milestone_047
                                              # 5. spdx2_comment_aggregation_unchanged
                                              # 6. sbom_type_flag_overrides_spdx3_native
                                              # 7. sbom_type_flag_overrides_cdx_lifecycles
                                              # 8. sbom_type_flag_preserves_per_component_tiers
                                              # 9. sbom_type_invalid_value_fails_parse
                                              # 10. spdx3_conformance_with_native_sbomtype
                                              # 11. schema_validation_passes_per_format

mikebom-cli/tests/fixtures/golden/spdx-3/      # MODIFY — all 9 SPDX 3 fixtures regenerate to
                                                # add the new `software_Sbom.software_sbomType[]`
                                                # native field. Per-fixture diff: +1 array on
                                                # the SpdxDocument element. Existing CDX 1.6 +
                                                # SPDX 2.3 goldens stay byte-identical.

docs/reference/
├── sbom-types.md                              # NEW — operator-facing reference per FR-001:
│                                                # CISA framework overview, per-format field-position
│                                                # table, jq recipes, mikebom-tier ↔ CISA-type ↔
│                                                # CDX-phase three-column table, mixed-tier
│                                                # presentation rule per Q1.
├── identifiers.md                             # MODIFY (small) — cross-reference link from
│                                                # the existing identifiers reference to the
│                                                # new sbom-types.md.
└── sbom-format-mapping.md                     # MODIFY (audit-record entry) — Section I (the
                                                  # native-field audit appendix from milestone 080)
                                                  # gains a milestone-081 row documenting (a) CDX 1.6
                                                  # native usage confirmed, (b) SPDX 2.3 escape clause
                                                  # documented (no native field), (c) SPDX 3 native
                                                  # field promotion from `comment`-only aggregation
                                                  # to `software_Sbom.software_sbomType[]` per
                                                  # Principle V.
```

**Structure Decision**: Single project. Extends one existing helper module (`lifecycle_phases.rs`), one existing emission path (`v3_document.rs`), and the two CLI subcommands. Adds one new integration test file + one new operator-facing reference doc + one audit-record entry in the existing audit-record doc. No new modules; no new crates; no new dependencies.

## Phase 0 — Research (already executed during /speckit.plan setup)

Five implementation-level findings, validated against the actual format specs at /speckit.plan time. Detailed in `research.md`.

1. **CDX 1.6 audit**: native `metadata.lifecycles[]` confirmed wired by milestone 047. Phase enum `{design, pre-build, build, post-build, operations, discovery, decommission}` covers 5/6 CISA types (all except Runtime, which CDX maps to `operations`/OBOM per the spec narrative). **No work needed for CDX.**

2. **SPDX 2.3 audit**: no native single-document-type enum exists. mikebom's existing `creationInfo.comment` aggregation per milestone 047 is appropriate per Principle V's escape clause (no native field to promote to). Documented as such; no work needed.

3. **SPDX 3 audit (the gap)**: `software_Sbom.software_sbomType` is defined as an array property (per `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json::software_Sbom_props`) accepting IRIs from a 6-value enum (`spdx:Software/SbomType/{analyzed,build,deployed,design,runtime,source}`) — **exactly matching the 6 CISA SBOM Types**. mikebom currently does NOT emit this field. **Real Principle V gap; this milestone closes it.** Implementation: add `tier_to_spdx3_sbomtype_iri` helper to `lifecycle_phases.rs`, wire it at the SpdxDocument construction site in `v3_document.rs`. The aggregated phase set already exists; one new emission target is the only delta.

4. **`runtime` tier auto-detection scope**: deferred to a separate milestone. mikebom's eBPF trace observes the BUILD process producing artifacts, not the runtime of those artifacts → eBPF-traced SBOMs remain `build` tier per CISA semantics. The `--sbom-type` flag accepts `runtime` as a valid vocab value (so operators with their own runtime-instrumentation pipelines can assert it), but auto-detection adding `runtime` requires a real runtime-observation feature mikebom doesn't have today.

5. **Operator-assert flag scope**: included in this milestone (was P2, audit findings keep it P2). The SPDX 3 native field promotion makes the assert use case more concrete — operators producing single-type compliance SBOMs (regulatory dashboards, CISA-aligned tooling that hard-fails on multi-type lifecycles) need a clean override path. The flag's vocab is the 6 CISA types; the override is document-level only (per-component `mikebom:sbom-tier` annotations preserve auto-detected values per US3 §2 + Q1 mixed-tier transparency rule).

## Phase 1 — Design & contracts

### data-model.md

Two new types in `mikebom-cli/src/generate/lifecycle_phases.rs`:
- `SbomType` enum with 6 variants (`Design`/`Source`/`Build`/`Analyzed`/`Deployed`/`Runtime`) matching the CISA types verbatim. Implements `as_spdx3_iri(&self) -> &'static str` returning `"spdx:Software/SbomType/<lowercase>"`. Implements `parse_str(s: &str) -> Result<Self, ParseSbomTypeError>` for `--sbom-type` CLI parsing.
- New helper `aggregate_spdx3_sbom_types(components, override_assertion: Option<SbomType>) -> Vec<&'static str>` mirroring the existing `aggregate_phases` pattern. When override is `Some`, returns single-element Vec; when `None`, aggregates per-component tiers via the existing dedup-and-sort logic.

### contracts/

One contract: `sbom-type-signaling.md`. Documents:
- The CLI surface (one new flag `--sbom-type` on both `mikebom sbom scan` and `mikebom trace run`).
- Per-format wire-format contract (CDX unchanged from milestone 047; SPDX 2.3 unchanged; SPDX 3 gains `software_Sbom.software_sbomType[]` array).
- The `tier_to_spdx3_sbomtype_iri` mapping table.
- The 11-test integration matrix (each test mapped to user-story acceptance scenarios + FRs/SCs).

### quickstart.md

Operator-facing recipes:
1. **Identify the SBOM type from a mikebom-emitted document** — per-format `jq` recipes pulling the SBOM-type signal from CDX `metadata.lifecycles[]`, SPDX 2.3 `creationInfo.comment`, SPDX 3 `software_Sbom.software_sbomType[]`.
2. **Assert the SBOM type via `--sbom-type`** — operator-supplied single-type override across all three formats.
3. **Interpret a Mixed-type SBOM** — per Q1 clarification: the docs surface presents the SBOM as spanning multiple CISA types; operators wanting single-type assertion use `--sbom-type`.
4. **Cross-reference the mikebom-tier ↔ CISA-type ↔ CDX-phase mapping table** — the canonical equivalence reference operators will return to repeatedly.
5. **Pre-PR gate behavior** — unchanged from milestone 078; the new SPDX 3 emission gets validated by the existing conformance gate.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. `/speckit.tasks` consumes plan.md + spec.md + Phase 1 docs and emits `tasks.md`. Estimated task count: **~12-14** — smaller than 080 because the production code changes are bounded (~80 LOC across 3 files vs 080's ~250 LOC across 4 new files), no new module created, no JSON-file-schema work, no validator-integration work.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all 12 principles + 4 strict boundaries with zero violations. Principle V audit is the milestone's central deliverable; the SPDX 3 native field promotion explicitly closes a Principle V gap that the audit surfaced.
