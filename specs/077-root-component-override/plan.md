# Implementation Plan: Operator-overridable root component name and version

**Branch**: `077-root-component-override` | **Date**: 2026-05-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/077-root-component-override/spec.md`

## Summary

Closes the operator UX gap where source-tier scans of arbitrary directories produce SBOMs with non-meaningful root component names (`filesystem-scan@0.0.0` derived from `path.file_name()` + hardcoded `"0.0.0"`). Adds two new CLI flags:

- `--root-name <NAME>` overrides the auto-derived `metadata.component.name` (CDX) / main-module `Package.name` (SPDX 2.3) / root element name (SPDX 3).
- `--root-version <VERSION>` overrides the corresponding version field.

Derived fields (`bom-ref`, `purl`, `cpe`) automatically follow the overridden name/version through the existing per-format derivation pipeline. Both flags independent. When both unset, behavior byte-identical to alpha.17 (no regression).

Per the 2026-05-06 clarifications: validation is permissive (reject only whitespace / control / `?` / `#`; URL-encode the rest at PURL emission), and override is a clean replacement (manifest-derived main-module dropped entirely from the emitted SBOM, NOT demoted to `components[]` — the demote-to-library follow-up is tracked as GitHub issue #151).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–076; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `clap` (the two new flags via derive). The `url` crate is already a direct workspace dep (promoted in milestone 075) and reused for URL-encoding the override values into the PURL `name` segment per RFC 3986. **No additions to the dependency tree at the lockfile level.**
**Storage**: N/A — purely a metadata transform; no caches, no persistence.
**Testing**: `cargo +stable test --workspace`. New integration tests in `mikebom-cli/tests/identifiers_root_component_override.rs` reuse the tempdir-based fixture pattern from milestones 074/075/076. Validation rejection tests use the milestone-075 negative-test pattern (assert clap exit code + error text).
**Target Platform**: Linux (CI primary), macOS (developer workstations). Logic is OS-agnostic.
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Override adds <1ms per scan (constant-time string substitution + URL-encoding). Negligible against the existing scan/emission cost.
**Constraints**: Determinism per FR-010 (same flag values + same scan target → byte-identical output). No regression on existing milestone-073/074/075/076 byte-identity goldens per FR-009 / SC-002 / SC-010.
**Scale/Scope**: Two new flags on `ScanArgs` + `RunArgs` + `GenerateArgs` (~30 LOC); one new `RootComponentOverride` struct on `ScanArtifacts` (~15 LOC); per-format builder updates (CDX `metadata.component` construction site at `cyclonedx/builder.rs:168-178`; SPDX 2.3 site at `spdx/document.rs:34-56`+; SPDX 3 site at `spdx/v3_document.rs:150,376-390,435`); main-module-drop logic when override is set (~20 LOC in CDX builder + SPDX equivalents); one new integration-test file (~250 LOC, ~12 tests across 3 user stories); one docs update.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All 12 principles + 4 strict boundaries:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | Pure-Rust extension. No FFI, no C, no new toolchain. |
| II. eBPF-Only Observation | ✅ Pass / N/A | Identifier metadata only; eBPF trace unchanged. |
| III. Fail Closed | ✅ Pass | Validation rejection at CLI parse time on bad input — fail before any scan work happens. The scan + emission pipeline is unchanged for the no-flag case. |
| IV. Type-Driven Correctness | ✅ Pass | New `RootComponentOverride { name: Option<String>, version: Option<String> }` shape on `ScanArtifacts` plus a small validator function. Production code uses `anyhow::Result`. Tests retain `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the established convention. No new raw `String` boundary crossings beyond the operator-supplied flag values, which are validated at parse. |
| V. Specification Compliance | ✅ Pass | **Native-first audit (constitution v1.4.0 5th bullet):** Override changes the *value* of an existing standards-native field (`metadata.component.name` / `version` in CDX; equivalents in SPDX 2.3 / SPDX 3). **Zero new `mikebom:*` annotations introduced.** `purl`, `cpe`, `bom-ref` derive through the existing per-format pipeline unchanged. The clean-replacement behavior (manifest-derived main-module dropped) preserves the existing standards-native shape — no new fields, no new property keys. |
| VI. Three-Crate Architecture | ✅ Pass | All changes inside `mikebom-cli`. No new crates. |
| VII. Test Isolation | ✅ Pass | Override + validation are pure user-space logic. Tests need no privilege. Tempdir-based fixture pattern from milestones 074/075/076. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | The operator's override is explicit input; mikebom emits exactly what the operator supplied (no inference, no auto-derivation, no soft-fail). |
| X. Transparency | ✅ Pass | When override fires, mikebom emits a `tracing::info!` recording the override (which fields were overridden, with what values, replacing what auto-derived defaults). When the override drops a manifest-derived main-module per the 2026-05-06 clarification, an additional `tracing::info!` records that, naming the dropped PURL so operators see the trade-off. |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | Operator CLI input is not an external data source. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — extends production code that already complies; tests use the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/077-root-component-override/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify + /speckit.clarify output
├── research.md                     # Phase 0 output
├── data-model.md                   # Phase 1 output
├── quickstart.md                   # Phase 1 output
├── contracts/
│   └── root-component-override.md  # Phase 1 — single CLI/lib contract
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches the CLI argparse + the three per-format builders + adds one struct field. No new modules, no new crates.

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   ├── scan_cmd.rs              # MODIFY — add --root-name/--root-version
│   │   │                            # to ScanArgs; validator function for
│   │   │                            # name/version inputs (rejects
│   │   │                            # whitespace/control/?/# per Q1
│   │   │                            # clarification); thread into
│   │   │                            # ScanArtifacts.root_override.
│   │   ├── run.rs                   # MODIFY — same flags on RunArgs;
│   │   │                            # thread into the build-tier
│   │   │                            # ScanArtifacts construction.
│   │   └── generate.rs              # MODIFY (small) — back-compat only:
│   │                                # update existing ScanArtifacts
│   │                                # construction site to populate the
│   │                                # new `root_override` field with
│   │                                # default. Per research §6, the
│   │                                # `mikebom sbom generate` subcommand
│   │                                # does NOT receive new flags — it
│   │                                # re-emits from a previously-created
│   │                                # attestation, where override would
│   │                                # have ambiguous semantics.
│   ├── generate/
│   │   ├── mod.rs                   # MODIFY — add `root_override:
│   │   │                            # Option<RootComponentOverride>` field
│   │   │                            # to ScanArtifacts. Define the
│   │   │                            # struct in this module.
│   │   ├── cyclonedx/
│   │   │   └── builder.rs           # MODIFY — at builder.rs:168-178,
│   │   │                            # use override values when present;
│   │   │                            # also filter main-module out of
│   │   │                            # `components` array per Q2 clean-
│   │   │                            # replacement clarification when
│   │   │                            # override is set on a manifest-
│   │   │                            # driven scan.
│   │   └── spdx/
│   │       ├── document.rs          # MODIFY — at lines 34-56, override
│   │       │                        # `target_name` consumption with
│   │       │                        # operator-supplied value when
│   │       │                        # present; same hardcoded "0.0.0"
│   │       │                        # site at line 456 needs override too.
│   │       └── v3_document.rs       # MODIFY — at lines 150, 376-390, 435,
│   │                                # use override values when present;
│   │                                # main-module drop equivalent here.
│   └── binding/
│       └── identifiers/
│           └── (no changes)         # 077 doesn't touch the identifier
│                                    # substrate; only the root-component
│                                    # display identity.
└── tests/
    └── identifiers_root_component_override.rs  # NEW — integration tests
                                                  # for SC-001..SC-010 +
                                                  # the manifest-drop
                                                  # clean-replacement test
                                                  # against an existing
                                                  # Cargo fixture.

docs/reference/identifiers.md                    # MODIFY (small) — add
                                                  # subsection on root
                                                  # component override
                                                  # under the existing
                                                  # CLI flag reference.
```

**Structure Decision**: Single project. Extends `mikebom-cli` with no new modules. Smallest-possible-surface-change consistent with milestones 074/075.

## Phase 0 — Research questions

Six implementation-level decisions to pin in `research.md` before Phase 1 design.

1. **PURL emitter URL-encoding behavior** — when constructing `pkg:generic/<NAME>@<VERSION>` for the root component, what URL-encoding does mikebom's existing PURL emission do today for special characters in the name segment? Verify by inspecting the existing PURL construction sites and confirming that npm-scoped names (`@acme/widget-svc`) round-trip correctly through whatever encoding the PURL emitter applies.
2. **`RootComponentOverride` shape** — small struct vs `(Option<String>, Option<String>)` tuple field on `ScanArtifacts`. Recommend struct for clarity + future expansion (e.g., if `--root-vendor` is added later, the struct gains a third field rather than the tuple growing). Pin the field name (`root_override` or `root_component_override`).
3. **Manifest-driven main-module detection + drop site** — at the CDX builder's `build_components` site (around `builder.rs:251-280`+), main-module components are detected via `properties[].name == "mikebom:component-role"` and `value == "main-module"` (per the existing comment at line 286). When override is set, those main-module components MUST be filtered out before `build_components` runs (they're dropped from `components[]` entirely; not demoted, per Q2 clarification). Pin the filter logic placement and verify it doesn't accidentally drop multi-main-module-case components (cargo workspace) — the override-set case is single-target by definition (operator supplied one identity), so the filter applies uniformly.
4. **CLI flag validation regex** — exact rejection rule per Q1 clarification: reject any input containing `\s` (ASCII whitespace), `\x00`–`\x1F` / `\x7F` (ASCII control), `?`, or `#`. Permissive otherwise — accept any non-empty UTF-8. Pin the validator function shape: `fn validate_root_field(value: &str, field_name: &str) -> Result<String, String>` returning the validated string or a clear clap-shaped error.
5. **CPE construction with override values** — `cpe:2.3:a:mikebom:<NAME>:<VERSION>:*:*:*:*:*:*:*`. Verify the existing CPE emitter at `generate/cpe.rs:169` consumes the same name/version pair and produces the right CPE string. The vendor portion stays hardcoded `mikebom` per assumption — no operator override for vendor in this milestone.
6. **`mikebom sbom generate` subcommand reach** — verify whether the `generate` subcommand (post-trace SBOM-from-attestation flow) accepts the same identifier flags as `scan` and `run`. If yes, add `--root-name`/`--root-version` there too (FR-001 says "all subcommands that construct root components"). If no, scope to `scan` + `run` only. Pin during research by reading `cli/generate.rs`.

## Phase 1 — Design & contracts

### data-model.md

One new struct (`RootComponentOverride`) added to the `generate` module. One new field on `ScanArtifacts`. Validation rules VR-077-001..004.

### contracts/

One contract: `root-component-override.md`. Documents:
- The two new CLI flags + their validation rule (per Q1)
- The override field on `ScanArtifacts`
- Per-format wire mapping (CDX `metadata.component`, SPDX 2.3 main-module Package, SPDX 3 root element)
- The main-module drop behavior per Q2 (clean replacement)
- Observable contract: log-line phrasing, expected output shape

### quickstart.md

Five operator-facing recipes:
1. **Override on a generic source-tier scan** (the headline; the user's exact use case)
2. **Override one flag at a time** (just `--root-name` or just `--root-version`)
3. **Override on a manifest-driven Cargo project** (US3; demonstrates clean replacement)
4. **Override on image-tier scan** (US2)
5. **Override + identifier flags together** (orthogonality with milestones 073-076)

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. `/speckit.tasks` consumes plan.md + spec.md + Phase 1 docs and emits `tasks.md`. Estimated task count: ~12-14 (smaller than 075 because no new dep, no new module beyond a tiny struct definition; smaller than 076 because no new identifier scheme + no new built-in pattern).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all 12 principles + 4 strict boundaries with zero violations.
