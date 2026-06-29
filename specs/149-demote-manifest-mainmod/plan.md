# Implementation Plan: preserve manifest-derived main-module as demoted library entry when `--root-name` overrides it

**Branch**: `149-demote-manifest-mainmod` | **Date**: 2026-06-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/149-demote-manifest-mainmod/spec.md`

## Summary

Add a new opt-in CLI flag `--preserve-manifest-main-module` (default `false`) that, when set together with milestone-077's root-override flags (`--root-name` / `--root-version` / `--root-purl`), preserves the manifest-derived main-module identity as a `library`-typed component in `components[]` rather than dropping it per the existing clean-replacement semantic. The demoted entry carries a new `mikebom:demoted-from-main-module = "true"` parity-bridging annotation per Constitution V audit (none of CDX 1.6 `component.type`, SPDX 2.3 `primaryPackagePurpose`, or SPDX 3 `software_softwarePurpose` expresses demote-provenance).

Phase 0 research (§A through §D below) confirmed:
1. **Three symmetric drop sites exist** at `cyclonedx/builder.rs:325-347`, `spdx/document.rs:262-282`, and `spdx/v3_document.rs:57-75`. Each emitter today checks `override_active` + the `mikebom:component-role: main-module` annotation to filter out the main-module from `components[]` / `packages[]` / `software_Package` elements. The 3 sites also populate a parallel `dropped_main_module_purls: Vec<String>` used for relationship re-anchoring (milestone 084).
2. **The fix should be a single shared helper** in `mikebom-cli/src/generate/root_selector.rs` consuming the existing `RootComponentOverride` + a new `preserve_main_module` boolean and producing `{ effective_components: Vec<ResolvedComponent>, redirected_main_module_purls: Vec<String> }`. All three emitters call the helper instead of duplicating the filter logic — `~25 LOC × 3` duplicated drop loops collapse into one ~40 LOC helper.
3. **Per US1 clarification Option A** (recorded 2026-06-29): the demoted entry's `dependsOn` edges live ONLY on the operator-override root. Implementation requirement: even when `preserve_main_module = true`, the main-module's PURL MUST still be added to the "redirected" Vec so relationship re-anchoring (milestone 084 logic) fires; the demoted entry has no outbound edges in the wire output despite being kept in `components[]`.
4. **The demote transformation is purely metadata-level**: remove the `mikebom:component-role: main-module` annotation (so the entry is no longer treated as the project's main-module), add the `mikebom:demoted-from-main-module: "true"` annotation, and let downstream type-derivation produce `type: "library"` automatically (no main-module role tag → falls through to default library type via existing `binary_role_to_cdx_type` / SPDX equivalents).

Total code surface estimate: ~40 LOC for the new helper in `root_selector.rs`, ~10 LOC of CLI flag wiring in `cli/scan_cmd.rs` + plumbing through to the emitter call sites, ~25 LOC removed from each of three emitters (consolidated into the helper call) = net ~−35 LOC across the three emitters + 40 LOC added in the helper + 10 LOC flag wiring ≈ +15 LOC net. Plus ~150 LOC of unit tests + a 60–80 LOC integration test covering three ecosystems (Cargo + npm + Go per SC-004). Zero new Cargo dependencies. Three ecosystem fixtures already exist for milestones 064 / 066 / 053; reuse them.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–148; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `clap` for the new boolean flag (via `Args`-derive — already used pervasively for milestones 077 / 081 / 119 / 134 flags), `serde`/`serde_json` for the annotation value, `tracing` for the INFO-level diagnostics on the no-op edge cases. `mikebom_common::resolution::ResolvedComponent` is the existing workspace type; `mikebom_common::types::purl::Purl` for the demoted entry's PURL (unchanged from pre-demote). **No new Cargo dependencies.**
**Storage**: N/A — purely in-process per-scan transformation; no persistence.
**Testing**: `cargo +stable test --workspace`. Unit tests in `mikebom-cli/src/generate/root_selector.rs#mod tests` (the new helper's unit test surface). Integration test in `mikebom-cli/tests/demote_manifest_mainmod_md149.rs` (NEW) exercising Cargo + npm + Go fixtures with `--preserve-manifest-main-module`. Plus the new parity-catalog row (C102 — verify next available number) gets exercised by the existing `cross_format_byte_identity` + `holistic_parity` CI tests when the goldens refresh.
**Target Platform**: Linux x86_64 + macOS arm64 + Windows (CI lanes). Pure-Rust pure-data-transform; no platform-specific behavior.
**Project Type**: Library/post-processor — touches `mikebom-cli`'s root-selection + per-format emission pipelines. Downstream consumers (CDX/SPDX 2.3/SPDX 3 wire output) get the new annotation transparently.
**Performance Goals**: One-pass O(N) iteration over the post-dedup `Vec<ResolvedComponent>` (the same Vec the existing drop loops walk). The helper IS the existing loop, just with one extra branch. Negligible perf delta.
**Constraints**:
- **Constitution V (standards-native > `mikebom:*`)**: The new `mikebom:demoted-from-main-module` annotation is permitted per Principle V's parity-bridging carve-out because no CDX 1.6 / SPDX 2.3 / SPDX 3 native field expresses demote-provenance (spec Constitution V audit enumerates 6 rejected alternatives). The annotation MUST be documented in `docs/reference/sbom-format-mapping.md` per Principle V's documentation requirement (spec FR-011).
- **Constitution IV (Type-Driven Correctness)**: The new helper operates on already-typed values (`ResolvedComponent`, `Purl`, the existing `RootComponentOverride` struct, plus the new boolean). No new `.unwrap()` in production paths; tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per existing convention.
- **No subprocess calls**: pure-Rust filter + mutate.
- **Pre-PR gate**: `./scripts/pre-pr.sh` MUST exit 0 before PR open (modulo documented pre-existing `sbomqs_parity` env-only failure per milestone-144 T001 note).
**Scale/Scope**: Affects every emitted SBOM when the new flag is set. The default (flag unset) path is byte-identical to milestone 077 (regression protection via existing milestone-077 goldens — SC-002). The pre-149-default path (no override flags at all) is byte-identical to pre-149 (regression protection via the existing milestones 053/064-070 byte-identity tests — SC-003).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ PASS | No new Cargo dependencies. |
| II | eBPF-Only Observation | ✅ N/A | Pure metadata transform at emission time; eBPF discovery untouched. |
| III | Fail Closed | ✅ PASS | No change to scan-failure semantics. The flag has explicit no-op behavior in Edge Cases 1 + 4 with INFO-level diagnostics. |
| IV | Type-Driven Correctness | ✅ PASS | Operates on existing typed values; no new `.unwrap()` in production paths. |
| V | Specification Compliance | ⚠️ **Parity-bridging carve-out** | New `mikebom:demoted-from-main-module` annotation permitted per Principle V's parity-bridging clause. Audit recorded in spec (dedicated Constitution V section enumerating 6 rejected native-field alternatives across all three formats) AND will be documented in `docs/reference/sbom-format-mapping.md` per FR-011. |
| VI | Three-Crate Architecture | ✅ PASS | All change in `mikebom-cli`. |
| VII | Test Isolation | ✅ PASS | New tests are pure-logic unit tests + an integration test that runs the binary against existing per-ecosystem fixtures; no eBPF privilege requirements. |
| VIII | Completeness | ✅ IMPROVES | The clean-replacement default discards the manifest identity entirely; the demote path preserves it as queryable provenance, increasing the operator-facing completeness signal. |
| IX | Accuracy | ✅ PASS | The demoted entry carries the manifest's verified PURL + license + hash metadata — accurate provenance. The new annotation signals the operator-override origin transparently. |
| X | Transparency | ✅ PASS | The new annotation IS the transparency signal — consumers can identify demoted entries vs natural library deps without parsing the override context. |
| XI | Enrichment | ✅ N/A | No enrichment-source changes. |
| XII | External Data Source Enrichment | ✅ N/A | No external sources consulted. |
| SB-1 | No lockfile-based discovery | ✅ N/A | The demoted entry is from the manifest-derived main-module (already discovered pre-149); no new component-introduction. |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ PASS | New tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | Demoted entry is package-tier, not file-tier. |

**All gates pass.** Principle V parity-bridging audit is the only carve-out and is justified + documented per FR-011.

## Project Structure

### Documentation (this feature)

```text
specs/149-demote-manifest-mainmod/
├── plan.md              # This file
├── research.md          # Phase 0 — placement decision + clarification linkage + cross-ecosystem coverage audit + parity-catalog row scoping
├── data-model.md        # Phase 1 — describes the existing ResolvedComponent + new annotation contract + pre/post behavior table
├── quickstart.md        # Phase 1 — operator-facing verification (3 scenarios: opt-in demote, regression-no-flag, regression-no-override)
├── contracts/
│   └── demote-helper.md # Pure-function contract for the new root_selector helper
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify (all items ✅)
```

### Source Code (repository root)

Touched files (narrow scope):

```text
mikebom-cli/
├── (no Cargo.toml change)
└── src/
    ├── cli/
    │   └── scan_cmd.rs                          # ADD new --preserve-manifest-main-module flag (clap Args-derive); plumb through to ScanRequest / ScanArtifacts struct
    ├── generate/
    │   ├── mod.rs                               # Plumb the new flag into ScanArtifacts (existing struct) so the three emitters can read it
    │   ├── root_selector.rs                     # NEW helper apply_main_module_drop_or_demote() consolidating the duplicated drop logic + the new preserve branch
    │   ├── cyclonedx/
    │   │   └── builder.rs                       # Lines 325-347 replaced with a call to the new root_selector helper
    │   └── spdx/
    │       ├── document.rs                      # Lines 262-282 replaced with a call to the same helper
    │       └── v3_document.rs                   # Lines 57-75 replaced with a call to the same helper
    └── parity/
        └── extractors/
            ├── mod.rs                           # ADD C102 row (verify next available number) for the new annotation
            ├── cdx.rs                           # ADD c102_cdx extractor
            ├── spdx2.rs                         # ADD c102_spdx23 extractor
            └── spdx3.rs                         # ADD c102_spdx3 extractor
docs/
└── reference/
    ├── sbom-format-mapping.md                   # ADD C102 row with Principle V audit trail per FR-011
    └── identifiers.md                           # ADD section documenting --preserve-manifest-main-module + interaction with milestone-077 flags per SC-007
mikebom-cli/
└── tests/
    ├── demote_manifest_mainmod_md149.rs        # NEW — SC-004 in-tree integration test covering Cargo + npm + Go fixtures with the new flag
    └── fixtures/
        └── golden/                              # Existing Cargo/npm/Go goldens may refresh with the new annotation when scanned with the new flag set
```

**Structure Decision**: Single-pass refactor — extract the duplicated drop logic from three emitters into one shared helper in `root_selector.rs`, then add the preserve branch + CLI flag. The refactor is net-LOC-neutral on the emitter side and adds ~40 LOC of new helper code + ~10 LOC flag wiring + ~150 LOC unit tests + ~60–80 LOC integration test + new parity-catalog row + docs.

## Complexity Tracking

*Not applicable.* All Constitution gates pass cleanly. Principle V parity-bridging is the only carve-out and is justified by the absence of a native field expressing demote-provenance across all three SBOM formats — the audit table in the spec enumerates 6 rejected native-field alternatives (CDX `component.type`, CDX `component.scope`, SPDX 2.3 `primaryPackagePurpose`, SPDX 2.3 typed-relationship enum, SPDX 3 `software_softwarePurpose`, SPDX 3 `LifecycleScopedRelationship.scope`).
