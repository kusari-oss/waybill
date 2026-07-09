# Implementation Plan: Design-tier component visibility for operators

**Branch**: `175-design-tier-visibility` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/175-design-tier-visibility/spec.md`

## Summary

**Primary requirement**: surface design-tier components (empty-version components emitted from constraint-only manifests — pip `requirements.txt`, Ruby `Gemfile`, npm root `package.json` without lockfile, etc.) to operators via (a) a new reading-guide subsection explaining the traceability ladder + remediation, (b) an at-scan-time INFO advisory log naming the count + remediation, (c) a new `sbom-format-mapping.md` row explicitly tagged **KEEP-NATIVE-FIRST** documenting why no `mikebom:*` annotation is introduced.

**Technical approach**: **docs + one advisory log, zero SBOM shape changes**. Every ecosystem reader (pip, ruby, npm, cargo, cocoapods, composer, erlang, scala, haskell, maven, dart) already tags constraint-only entries with `sbom_tier = Some("design")`. The doc-scope CDX signal (`metadata.lifecycles[{phase: "design"}]`) is already populated via m047/m081's `lifecycle_phases::tier_to_phase` mapping. No new emission code needed on the wire side.

Three surgical additions:

1. **Advisory log at emission-tail** — `mikebom-cli/src/cli/scan_cmd.rs` gains a new advisory block near the m173/m176 blocks. Predicate: `design_tier_count > 0 && !components.is_empty() && !suppression_set`. Message body carries `"design-tier components detected: "` stable substring + count + remediation string pointing at `docs/reference/reading-a-mikebom-sbom.md#design-tier`. NOT gated on `--offline` (FR-002 explicit — remediation works offline).

2. **Suppression signal** — env-var only per FR-005 evaluation: `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1`. Rationale: matches the milestone-110 `MIKEBOM_NO_DEPRECATION_NOTICE=1` precedent (env-var-driven advisory suppression, no new CLI flag). Env vars keep the CLI-flag surface small and don't require `--help` scaffolding. Follow-up milestones can consolidate into a broader `--no-advisories` flag if the pattern proliferates.

3. **Docs — three files**:
   - **`docs/reference/reading-a-mikebom-sbom.md`** — new subsection under §3.4 (Transparency / completeness gaps) documenting design-tier concept, traceability ladder, native wire signals across 3 formats, operator remediation per ecosystem (pip/npm/cargo/ruby minimum), jq recipes (count design-tier + list PURLs + threshold-check).
   - **`docs/reference/sbom-format-mapping.md`** — new row explicitly tagged **KEEP-NATIVE-FIRST** (new tag polarity for this milestone), documenting empty `version` / `Package.versionInfo` / `software_Package.packageVersion` across all 3 formats + `metadata.lifecycles[design]` in CDX. Rejected `mikebom:design-tier-count` invention named with rationale.
   - **`docs/reference/component-tiers.md`** — light cross-reference to the new reading-guide subsection (existing file already discusses `sbom_tier` values including `"design"`).

**Wire contract unchanged**: SC-006 byte-identity guarantee — every existing golden regression fixture stays bit-for-bit identical. The wire is already correct; this milestone is docs + operator-UX.

**Blast radius**: ~30 lines in `scan_cmd.rs` (advisory block matching m173/m176 pattern), ~150 lines in `reading-a-mikebom-sbom.md` (new subsection with jq recipes), ~5 lines in `sbom-format-mapping.md` (one row), ~10 lines in `component-tiers.md` (cross-ref paragraph). One new integration test file at `mikebom-cli/tests/design_tier_advisory.rs` with 5 test functions (SC-002/SC-003/SC-004/SC-005 + a stability substring check).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–176; no nightly required).

**Primary Dependencies**: Existing only — `tracing` (advisory-log emission at INFO level), `std::env` (env-var read for suppression), no touch of `mikebom-common`. **Zero new Cargo dependencies.** No subprocess calls. No network access. No filesystem writes.

**Storage**: N/A — pure emission-time diagnostic; no persistence.

**Testing**: `cargo test` — 1 new integration test at `mikebom-cli/tests/design_tier_advisory.rs` covering the 5 US2 acceptance predicates (fires-once-on-design-tier, silent-on-zero, silent-on-suppression, fires-under-offline, stable-substring). Existing byte-identity golden regression suite MUST remain untouched (SC-006 gate).

**Target Platform**: All hosts mikebom builds on — Linux, macOS, Windows (m100-experimental).

**Project Type**: cli (mikebom sbom-generation CLI).

**Performance Goals**: N/A — the advisory-emission is `O(N)` over components with a single field check; happens once per scan at emission-tail.

**Constraints**: SC-006 byte-identity gate — no changes to `components[]`, `metadata.component`, `metadata.lifecycles[]`, or any existing per-component annotation. The advisory log is stderr-only; SBOM stdout/file output is unchanged.

**Scale/Scope**: Small. 1 file edit in `scan_cmd.rs`, 3 docs files, 1 new integration test file. Zero golden regeneration.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new Cargo dependencies. Pure Rust addition.
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched.
- **III. Fail Closed**: ✅ Advisory suppression is orthogonal to correctness — the SBOM emission is unchanged; only stderr diagnostic output is affected. Env-var absence defaults to "advisory fires when predicate holds" (fail-open in the operator-communication direction is correct — silencing an at-scan-time diagnostic by default would defeat the milestone's purpose).
- **IV. Type-Driven Correctness**: ✅ Advisory predicate reads `c.sbom_tier == Some("design")` (existing `Option<String>` on `ResolvedComponent`); no new types introduced. No `.unwrap()` in production code.
- **V. Specification Compliance / Standards-native precedence**: ✅ **AUDITED — KEEP-NATIVE-FIRST**. Two audits performed:
  - **Per-component signal**: empty `component.version` (CDX) / empty `Package.versionInfo` (SPDX 2.3) / empty `software_Package.packageVersion` (SPDX 3) is the native carrier. All three formats already emit these correctly today (Constitution Principle IX: accuracy over fabrication). Rejected alternative: `mikebom:design-tier-marker` per-component annotation — would duplicate a signal every format already carries natively. **KEEP-NATIVE-FIRST**.
  - **Doc-scope aggregate**: CDX `metadata.lifecycles[]` containing `{"phase": "design"}` is the native carrier when ≥1 design-tier component exists (m047/m081). Rejected alternative: `mikebom:design-tier-count` doc-scope annotation — would duplicate CDX's `metadata.lifecycles[]` phase presence. Consumers can `jq '[.components[]?.version | select(. == "")] | length'` for exact count in one call. **KEEP-NATIVE-FIRST**.
  - Both audits codified in the new `sbom-format-mapping.md` row per FR-004 with the new tag polarity **KEEP-NATIVE-FIRST**.
- **VI. Three-Crate Architecture**: ✅ Change contained to `mikebom-cli` (advisory-log emission + docs). No `mikebom-common` or `mikebom-ebpf` changes.
- **VII. Test Isolation**: ✅ Integration test uses per-test tempdir + `assert_cmd`; no shared state.
- **VIII. Completeness**: ✅ **Improved**. Post-175 an operator understands the emitted signal and knows the operator-side action to improve tier — the SBOM's completeness metadata (empty version + lifecycle phase) becomes operator-actionable rather than opaque.
- **IX. Accuracy**: ✅ **Preserved verbatim**. The empty-version emission is the accuracy-honest behavior; this milestone documents WHY it's honest and offers a remediation path. Zero change to what mikebom claims to know.
- **X. Transparency**: ✅ **Directly serves Principle X**. The advisory log tells operators when their scan target is constraint-only; the reading guide explains the transparency signal they're already receiving.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A.

**Strict Boundaries check**:
- **New subprocess**: ✅ None.
- **New network access**: ✅ None.
- **New filesystem writes**: ✅ None. Purely emission-time diagnostic to stderr.
- **New `mikebom:*` annotation namespaces**: ✅ **Zero**. This milestone's Constitution Principle V audit is KEEP-NATIVE-FIRST for both scopes — introducing any `mikebom:*` annotation is explicitly rejected per FR-006.
- **New Cargo dependencies**: ✅ Zero.
- **Strict Boundary §5 (file-tier no-duplicates)**: ✅ Preserved. File-tier walker unaffected — design-tier discriminator lives at the reader-emission layer above file-tier.

**Verdict**: All principles pass. Zero violations. Milestone improves Principles VIII/X (Completeness / Transparency). Codifies KEEP-NATIVE-FIRST as a new polarity in the Principle V audit vocabulary.

## Project Structure

### Documentation (this feature)

```text
specs/175-design-tier-visibility/
├── spec.md              # Feature specification (already written)
├── plan.md              # This file
├── research.md          # Phase 0 — advisory-suppression mechanism decision + docs-placement decision
├── data-model.md        # Phase 1 — advisory-predicate + suppression signal + KEEP-NATIVE-FIRST tag definition
├── quickstart.md        # Phase 1 — 3-scenario verification recipe (US1 docs walk, US2 advisory, US3 mapping row grep)
├── contracts/           # Phase 1 — advisory-log wording contract + KEEP-NATIVE-FIRST tag contract
├── checklists/          # Requirements checklist (spec-phase output — 16/16 PASS)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── cli/
│       └── scan_cmd.rs                       # ~30 lines: advisory-log block at emission-tail (after m176 block)
└── tests/
    └── design_tier_advisory.rs               # NEW ~200 lines: 5 US2 integration tests

docs/reference/
├── reading-a-mikebom-sbom.md                 # ~150 lines: new design-tier subsection under §3.4 (Transparency)
├── sbom-format-mapping.md                    # ~5 lines: new KEEP-NATIVE-FIRST-tagged row
└── component-tiers.md                        # ~10 lines: cross-reference paragraph to the new subsection

# NO changes to:
mikebom-cli/tests/fixtures/golden/            # SC-006 byte-identity gate — zero golden touch
mikebom-cli/src/scan_fs/                      # Existing sbom_tier="design" assignment is authoritative; no reader changes
mikebom-cli/src/generate/                     # Existing lifecycle_phases::tier_to_phase mapping is authoritative
mikebom-cli/src/parity/                       # Zero new parity extractors (no new annotation)
mikebom-common/                               # Zero changes; entirely mikebom-cli-contained
```

**Structure Decision**: pure operator-UX addition. One scan_cmd.rs edit + three docs files + one integration test. No emitter changes, no parity infra, no golden regeneration. The wire contract is already correct — this milestone documents it.

## Complexity Tracking

No constitution violations to justify. The plan is a straight-line advisory-log + docs delivery with zero new detection logic, zero SBOM shape changes, and zero new `mikebom:*` annotations.
