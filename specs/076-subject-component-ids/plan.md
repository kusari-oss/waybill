# Implementation Plan: Subject identifier scheme + per-component user-defined identifiers

**Branch**: `076-subject-component-ids` | **Date**: 2026-05-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/076-subject-component-ids/spec.md`

## Summary

Closes the cross-tier content-addressable correlation chain that milestones 072–075 began. Two deliverables in one milestone, both small additive changes on top of milestone 073's identifier substrate:

1. **Document-level `subject:` identifier scheme.** Adds `subject:<algo>:<hex>` as a fifth `BuiltinScheme` variant (alongside `Repo`, `Git`, `Image`, `Attestation`). Build-tier auto-detects from the in-toto attestation envelope's already-captured subject set. Source-tier and image-tier accept manual `--subject-hash` flags. The cross-tier handshake becomes automatic when the build's `subject:sha256:X` matches the image scan's `image:` digest portion — pure string match by external tools.

2. **Per-component user-defined identifiers.** Adds a new `--component-id <PURL>=<scheme>:<value>` repeatable flag that attaches operator-supplied identifiers to specific components. Identifiers ride standards-native per-component carriers (CDX `components[].properties[]`, SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`, SPDX 3 `Element.externalIdentifier[]`) — Constitution Principle V audit confirms zero new `mikebom:*` annotations. Built-in scheme names rejected at CLI parse time on `--component-id` (reserved for future native-field usage).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–075; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `tracing`, `anyhow`, `clap` (the two new flags via derive), `thiserror`. The build-tier subject extraction reuses the in-toto witness-v0.1 subject set the existing trace pipeline already collects in-process; no new subprocess calls, no new I/O, no new crates. **No additions to the dependency tree at the lockfile level.**
**Storage**: N/A — identifier emission is a pure metadata transform; no caches, no persistence.
**Testing**: `cargo +stable test --workspace`. New integration tests in `mikebom-cli/tests/identifiers_subject_and_component.rs` exercise the document-level `subject:` emission across all three formats and the per-component user-defined identifier emission across all three formats.
**Target Platform**: Linux (CI primary), macOS (developer workstations). Logic is OS-agnostic; depends only on existing in-process attestation subject set and string manipulation.
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Subject identifier emission adds <1ms per build-tier scan (one in-process subject-set read, one identifier-construction loop). Per-component identifier matching adds <5ms even for thousand-component SBOMs (linear scan against a hash-set of supplied PURLs).
**Constraints**: Determinism per FR-012 (same input → byte-identical output, including identifier order). Soft-fail per FR-005 (validation failure routes to milestone 073's `UserDefined` path). No regression on existing milestone-073/074/075 byte-identity goldens per FR-013/SC-005.
**Scale/Scope**: One new `BuiltinScheme::Subject` variant + value validator (~50 LOC); one helper to extract subjects from in-toto state (~40 LOC); two new CLI flags on `ScanArgs` and `RunArgs` (~20 LOC each); per-component identifier emission in 3 format emitters (~50 LOC each); one new integration-test file (~400 LOC, ~14 tests across 4 user stories); one docs update.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All 12 principles + 4 strict boundaries:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | Pure-Rust extension of existing pure-Rust types. |
| II. eBPF-Only Observation | ✅ Pass / N/A | This milestone touches identifier metadata, not dependency discovery. The eBPF trace is unchanged; the in-toto subject set is consumed at SBOM-emit time, not as a discovery source. |
| III. Fail Closed | ✅ Pass | Soft-fail per FR-005 routes through milestone 073's `UserDefined` rule for malformed values. The trace + scan pipeline that produces the SBOM continues to fail closed exactly as before. |
| IV. Type-Driven Correctness | ✅ Pass | Reuses existing `Identifier`, `SchemeName`, `IdentifierValue`, `BuiltinScheme`, `IdentifierKind` newtypes from milestone 073. New `BuiltinScheme::Subject` variant, new `validate_subject` validator following the existing pattern. Production code uses `anyhow::Result`/`IdentifierError`. Tests guard `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the established convention. No new raw `String` boundary crossings. |
| V. Specification Compliance | ✅ Pass | **Native-first audit (constitution v1.4.0 5th bullet):** Both deliverables ride standards-native carriers. Document-level `subject:` rides CDX `metadata.component.externalReferences[]` (Phase 0 research §1 pins which existing CDX 1.6 enum value), SPDX 2.3 `Package.externalRefs[].referenceCategory = PERSISTENT-ID`, SPDX 3 `Element.externalIdentifier[]` — same per-document carrier set milestone 073 established. Per-component user-defined identifiers ride CDX `components[].properties[]`, SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`, SPDX 3 `Element.externalIdentifier[]` — all existing standards-native fields per format (Phase 0 research §2 confirms). **Zero new `mikebom:*` annotations introduced.** |
| VI. Three-Crate Architecture | ✅ Pass | All changes inside `mikebom-cli`. No new crates. |
| VII. Test Isolation | ✅ Pass | Identifier emission and validation are pure user-space logic; tests need no privilege. New integration tests use the same tempdir-based fixture pattern from milestones 073/074/075. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | The 073 soft-fail-to-`UserDefined` rule applies (FR-005): malformed `subject:` values downgrade to user-defined classification rather than producing a falsely-`Builtin` emission. Per-component identifiers are user-defined-only (no built-in classification possible at the per-component layer in this milestone). |
| X. Transparency | ✅ Pass | Both deliverables fire audibly: `subject:` auto-detect emits one info-level log per detected subject (and one info-level log per skipped non-sha256 subject, per the 2026-05-06 clarification); `--component-id` selectors that match zero components log `tracing::warn!` per FR-010. The auto-detected `subject:` `source_label` documents the origin per milestone 073 conventions. |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | The in-toto attestation subject set is in-process state from the trace pipeline, not an external data source. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — extending production code that already complies; tests use the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/076-subject-component-ids/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify output (with /speckit.clarify integration)
├── research.md                     # Phase 0 output
├── data-model.md                   # Phase 1 output
├── quickstart.md                   # Phase 1 output
├── contracts/
│   ├── subject-identifier.md       # Phase 1 — document-level `subject:` contract
│   └── per-component-id.md         # Phase 1 — per-component identifier contract
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches the identifier module + 3 format emitters + 2 CLI entry points. No new modules, no new crates (one tiny new submodule `component_id.rs` for the flag-value parser).

```text
mikebom-cli/
├── src/
│   ├── binding/
│   │   └── identifiers/
│   │       ├── mod.rs                          # MODIFY — add `BuiltinScheme::Subject`
│   │       │                                   # variant; extend cdx/spdx
│   │       │                                   # type-mapping methods.
│   │       ├── validators.rs                   # MODIFY — add `validate_subject`
│   │       │                                   # for `<algo>:<hex>` form.
│   │       ├── auto_detect.rs                  # MODIFY — add new helper
│   │       │                                   # `subject_identifiers_from_
│   │       │                                   # attestation_subjects(...)`
│   │       │                                   # called by build-tier flow.
│   │       └── component_id.rs                 # NEW — types + parser for
│   │                                           # --component-id flag values
│   │                                           # (PURL=scheme:value).
│   ├── cli/
│   │   ├── scan_cmd.rs                         # MODIFY — add --subject-hash
│   │   │                                       # and --component-id flags to
│   │   │                                       # ScanArgs; thread through to
│   │   │                                       # ScanArtifacts.
│   │   └── run.rs                              # MODIFY — same flags on
│   │                                           # RunArgs + wire build-tier
│   │                                           # subject auto-detect from the
│   │                                           # trace's collected subjects.
│   ├── generate/
│   │   ├── mod.rs                              # MODIFY — ScanArtifacts gains
│   │   │                                       # `component_identifiers:
│   │   │                                       # Vec<ComponentIdentifierFlag>`
│   │   │                                       # field threaded to emitters.
│   │   ├── cyclonedx/
│   │   │   ├── metadata.rs                     # MODIFY — emit `subject:`
│   │   │   │                                   # under externalReferences.
│   │   │   └── components.rs (or equiv.)       # MODIFY — emit per-component
│   │   │                                       # identifiers under properties[].
│   │   └── spdx/
│   │       ├── packages.rs                     # MODIFY — emit `subject:` and
│   │       │                                   # per-component user-defined
│   │       │                                   # identifiers under
│   │       │                                   # externalRefs[PERSISTENT-ID].
│   │       └── v3_*.rs                         # MODIFY — emit both via
│   │                                           # Element.externalIdentifier[].
│   └── parity/
│       └── extractors/                          # MODIFY — new catalog row(s)
│                                                # for subject: and per-component
│                                                # identifier extraction. Per-tier
│                                                # symmetric carrier mapping.
└── tests/
    └── identifiers_subject_and_component.rs     # NEW — integration tests for
                                                  # SC-001..SC-009.

docs/reference/identifiers.md                    # MODIFY — document new
                                                  # scheme + new flag.
```

**Structure Decision**: Single project. Extends `mikebom-cli` with one new tiny module (`component_id.rs` for the flag-value parser). Smallest-possible-surface-change consistent with milestones 074/075.

## Phase 0 — Research questions

Six implementation-level decisions to pin in `research.md` before Phase 1 design.

1. **CDX 1.6 `externalReferences[].type` enum value for document-level `subject:`** — pick the existing enum value that best fits "binary subject hash" semantics. Candidates: `attestation` (reuse 073's attestation: scheme's type), `formulation` (build-output semantic), `evidence`, `other` (semantic-poor fallback). Decide based on (a) downstream tool compatibility, (b) Principle V native-fit, (c) symmetry with milestone 073's existing mappings.
2. **CDX 1.6 carrier choice for per-component user-defined identifiers** — `components[].properties[]` (key-value) vs `components[].externalReferences[]` (URI-shaped). The CDX spec describes `properties[]` as "name-value store … flexibility to include data not officially supported in the standard" — a clean fit for arbitrary `(scheme, value)` pairs. `externalReferences[]` is described as "external references … not included with the BOM" — semantically about external systems/sites, not arbitrary identifiers. Decide and justify.
3. **In-toto subject extraction site** — where in `mikebom trace run`'s flow does the SBOM-emit code read the collected subject set? Currently the subject set is captured by the attestation-builder at trace-completion time. Identify the exact in-process state object and confirm it's accessible at the point where `auto_detect_build_tier_identifiers` runs (or wherever the new `subject_identifiers_from_attestation_subjects` helper plugs in).
4. **`subject:` value validation regex** — exact form `^(sha256|sha512):[0-9a-f]+$` plus length checks (64 chars hex for sha256, 128 chars hex for sha512). Pin the validator's behavior on uppercase hex (reject — RFC 6234 canonical hex is lowercase), on whitespace (reject), on missing algo prefix (reject), on prefix-only-no-hex (reject). Soft-fail-to-`UserDefined` per FR-005.
5. **Per-component identifier matching algorithm** — for `--component-id "PURL=scheme:value"`, exact-PURL-match against `components[].purl`. Decide what "exact" means: byte-equality of the canonical PURL string, or PURL-spec-aware semantic equality (e.g., URL-encoding tolerance)? Recommend byte-equality for determinism + simplicity.
6. **Determinism contract for multi-component matches** — when one `--component-id` selector matches N components (different `bom-ref` values, same PURL), all N receive the identifier (FR-011). Pin the per-component emission ordering: same as the surrounding `components[]` array order, with the per-component identifier appearing at the end of the existing `properties[]` (or equivalent) array, in lexical order by `(scheme, value)` for stable serialization.

## Phase 1 — Design & contracts

### data-model.md

Two new entities:
- `SubjectIdentifier`: a document-level `Identifier` with scheme `subject:` and value `<algo>:<hex>`. Composes `Identifier` + `SchemeName` + `IdentifierValue` + `IdentifierKind` (existing 073 types). Multiple SubjectIdentifiers may attach to one SBOM (multi-output builds).
- `ComponentIdentifierFlag`: an operator-supplied per-component identifier from `--component-id PURL=scheme:value`. Composed of: `selector_purl: String`, `scheme: SchemeName`, `value: IdentifierValue`. Materialized as a `Vec<ComponentIdentifierFlag>` on `ScanArtifacts` / `TraceArtifacts`, consumed by per-format emitters.

Plus one extension to existing `BuiltinScheme` enum (new `Subject` variant) and one extension to `ScanArtifacts` (new field `component_identifiers: Vec<ComponentIdentifierFlag>`).

Validation rules VR-076-001..006 captured per the FR list.

### contracts/

Two contracts:
- `subject-identifier.md` — public API for the new `BuiltinScheme::Subject`, the value validator, the `subject_identifiers_from_attestation_subjects` helper, and the per-format wire mapping (CDX externalReferences type per Phase 0 research §1; SPDX 2.3 PERSISTENT-ID; SPDX 3 externalIdentifier).
- `per-component-id.md` — public CLI contract for `--component-id`, the parser shape, the matching algorithm, the per-format wire mapping (CDX properties[]; SPDX 2.3 externalRefs[PERSISTENT-ID]; SPDX 3 externalIdentifier[]). Contract specifies that built-in scheme names are rejected at parse time.

### quickstart.md

Operator-facing recipes:
1. **Build-tier subject auto-detect** — the headline (no flags needed; `mikebom trace run` produces a build SBOM with `subject:` for each artifact).
2. **Cross-tier digest handshake** — the SC-002 walkthrough: image SBOM's component hash matches build SBOM's `subject:` value, external tool correlates by string match.
3. **Manual `--subject-hash`** — for source-tier or for opting into non-sha256 subjects.
4. **Per-component identifier attachment** — operator pipes `--component-id` flags to mark internal asset IDs.
5. **Reading per-component identifiers in each format** — CDX `properties[]`, SPDX 2.3 `externalRefs[PERSISTENT-ID]`, SPDX 3 `externalIdentifier[]`.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. `/speckit.tasks` consumes plan.md + spec.md + Phase 1 docs and emits `tasks.md`. Estimated task count: ~18-20 (larger than 074/075 because per-component identifier emission flows through 3 format emitters; smaller than 073 because no new module + no new flag for users on the document-level path beyond `--subject-hash`).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all 12 principles + 4 strict boundaries with zero violations.
