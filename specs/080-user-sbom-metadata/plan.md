# Implementation Plan: User-provided SBOM metadata

**Branch**: `080-user-sbom-metadata` | **Date**: 2026-05-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/080-user-sbom-metadata/spec.md`

## Summary

Replace the fragile `jq` post-processing recipe operators currently use to inject their own identity + context into mikebom-emitted SBOMs with five native CLI flags landing symmetrically across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1. The flags — `--scan-target-name`, `--creator <Type: Name>` (repeatable), `--annotator <Type: Name>` + `--annotation-comment <text>` (positionally-paired per Q1), `--metadata-comment <text>`, `--metadata-file <path.json>` (sidecar) — attach to both `mikebom sbom scan` and `mikebom trace run`. All flags are additive: they augment mikebom's auto-populated fields rather than replacing them.

The deliberate scope: SBOM-document-level metadata only. Per-component metadata is already handled by `mikebom sbom enrich`'s JSON Patch path. CDX 1.6 fallback strategy is locked per Q2: native `bom.annotations[]` if the schema audit confirms support (likely), otherwise `mikebom:` namespaced parity bridges in `metadata.properties[]` with each emitted bridge documented in `docs/reference/sbom-format-mapping.md` per Constitution Principle V's escape clause.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–079; no nightly).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (JSON-LD round-tripping; the existing CDX emission path uses `serde_json::Value` directly, not structured types from the `cyclonedx-bom` crate, so adding new fields is purely additive JSON construction), `tracing`, `anyhow`, `clap` (the five new flags via derive — `--creator` repeatable via `ArgAction::Append`; `--annotator`/`--annotation-comment` via two parallel `Vec<String>` fields with post-validation per research §3), `chrono` (annotation timestamp deterministic per scan-emission time, same source as `creationInfo.created`), `thiserror` (parser error enum). Reuses milestone 078's `spdx3-validate==0.0.5` as the SPDX 3 conformance gate. **No new Cargo dependencies.**
**Storage**: N/A — pure metadata transform on the SBOM emission code paths; no caches, no persistence. The `--metadata-file` JSON sidecar is a one-shot read; mikebom holds the parsed values in process for the duration of the scan.
**Testing**: `cargo +stable test --workspace` continues as the primary gate. Adds new integration tests in `mikebom-cli/tests/sbom_user_metadata.rs` covering: per-flag native field landings across CDX/SPDX 2.3/SPDX 3, multi-annotation positional pairing, `--metadata-file` schema validation, conflict resolution between file and CLI flags, schema validation post-emission for all three formats, and the milestone-078 `spdx3-validate` conformance gate continues to pass with the new fields populated.
**Target Platform**: Linux (CI primary), macOS (developer workstations). The flags are pure user-space CLI surface; no platform-specific code.
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Negligible per-emission overhead. Each new flag adds O(1) metadata insertion at the per-format builder; the JSON output grows by the literal flag-value bytes. Total emission wall-time impact: <5ms regardless of flag count, well within milestone-079's <30s validator gate envelope.
**Constraints**: Determinism per FR-009 (same flag inputs + same scan inputs → byte-identical SBOMs across re-runs; sort order alphabetical-by-key in JSON, stable insertion order for repeatable arrays). All emitted SBOMs MUST continue to pass schema validation per FR-010 (CDX 1.6 schema, SPDX 2.3 schema, SPDX 3 schema, AND milestone 078's `spdx3-validate` SHACL gate). Native-first per Constitution Principle V — Phase 0 §1 audits each format's native fields before falling back to `mikebom:` parity bridges.
**Scale/Scope**: Medium-scoped feature. ~150–250 LOC for CLI flag definitions + parsing logic. ~80 LOC at each of the three format-emission call sites (CDX `metadata.rs`, SPDX 2.3 `document.rs`, SPDX 3 `v3_document.rs`). ~150 LOC `--metadata-file` JSON parser + schema validator. ~400 LOC integration tests. Plus all 27 existing byte-identity goldens regenerate as the expected operator-visible change of the milestone (analogous to milestone-077 + milestone-078 patterns; per-format diff size varies — CDX likely smallest, SPDX 3 likely largest because new graph elements get added per Tool/Organization/Person/Annotation).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All 12 principles + 4 strict boundaries reviewed:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | All Rust changes inside `mikebom-cli`. No C, no FFI. No new dependencies. |
| II. eBPF-Only Observation | ✅ Pass / N/A | This milestone touches CLI surface + SBOM-emission metadata; the eBPF trace path is unchanged. |
| III. Fail Closed | ✅ Pass | Validation failures (invalid `Type:` prefix in `--creator`, missing `--annotation-comment` after `--annotator`, malformed `--metadata-file` JSON, unknown top-level field, file/flag conflict on single-valued fields) ALL produce non-zero CLI exit with clear error messages. No silent fallback to "best guess." |
| IV. Type-Driven Correctness | ✅ Pass | New types: `Creator { kind: CreatorKind, name: String }` newtype with `kind` ∈ enum `{Tool, Organization, Person}`; `Annotation { annotator: Creator, comment: String, timestamp: DateTime<Utc> }`; `MetadataFile` deserialized via `#[derive(Deserialize)]` with `#[serde(deny_unknown_fields)]` for FR-005's unknown-field rejection. No raw-`String` boundary crossings beyond CLI input parsing. Production code uses `anyhow::Result` for application errors + `thiserror` for the parser error enum. Test code uses `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md. |
| V. Specification Compliance | ✅ Pass | **Native-first audit explicit (per the v1.4.0 amendment requirement):** every flag MUST land at the standards-native field position. SPDX 2.3 has `creationInfo.creators[]` + `creationInfo.comment` + `annotations[]` natively (FR-001/FR-002/FR-003 land there). SPDX 3 has `Tool`/`Organization`/`Person` Agent classes + `Annotation` element + `software_Sbom.name` natively. CDX 1.6's annotation surface is audited at Phase 0 §1 — the spec presumes `bom.annotations[]` is native (added in CDX 1.6 per the changelog) and the Q2 fallback is the `mikebom:invocation-comment` / `mikebom:annotation` parity-bridge in `metadata.properties[]` IF audit reveals insufficient native support. Each parity bridge actually emitted MUST be recorded in `docs/reference/sbom-format-mapping.md` per the Principle V escape clause. Spec authors cite the audit result in the FR text (FR-002, FR-003, FR-008); reviewers can verify the audit outcome. |
| VI. Three-Crate Architecture | ✅ Pass | All Rust changes inside `mikebom-cli`. No new crates. |
| VII. Test Isolation | ✅ Pass | All tests run without elevated privileges. No eBPF code touched. Reuses milestone 078's graceful-skip + CI strict-mode pattern for the SPDX 3 validator gate. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | The `--metadata-file` schema with `#[serde(deny_unknown_fields)]` rejects malformed input rather than silently dropping fields. Format-specific creator-prefix routing (per Edge Cases) ensures CDX `tools[]` / `manufacturer` / `authors[]` get only the creator types appropriate for them, not all creators dumped into `tools[]` regardless of type. |
| X. Transparency | ✅ Pass | All operator-supplied metadata appears at standards-native locations (or documented parity bridges); downstream tooling reading by spec-defined paths discovers it without mikebom-aware logic. The provenance of "who supplied this field" (operator vs auto-populated) is implicit in field-position semantics — mikebom's auto-populated entry in `tools[]` always has `name == "mikebom"`; user-supplied entries have whatever name the operator passed. |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment — operator-supplied metadata, not external-source data. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | Same — operator-supplied, not external. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — extending production code that already complies; tests use the standard guard |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed. Principle V audit explicitly cited per the v1.4.0 amendment requirement: native fields are the primary landing for every flag; `mikebom:` parity bridges are the documented fallback only when CDX audit reveals insufficient native support.

## Project Structure

### Documentation (this feature)

```text
specs/080-user-sbom-metadata/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify + /speckit.clarify output (Q1 + Q2 integrated)
├── research.md                     # Phase 0 — CDX 1.6 audit + per-format call-site map + schema design
├── data-model.md                   # Phase 1 — Creator / Annotation / MetadataFile / UserMetadata types
├── quickstart.md                   # Phase 1 — operator-facing recipes covering all 5 flags
├── contracts/
│   └── user-sbom-metadata.md       # Phase 1 — wire-format contract per format + JSON-file schema
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches the CLI definitions + the three format-emission code paths + adds new types + new integration tests. No CI workflow changes; reuses milestone-078 conformance gate.

```text
mikebom-cli/
├── src/
│   ├── cli/
│   │   ├── scan_cmd.rs                        # MODIFY (~80 LOC) — add 5 new clap flags via derive
│   │   │                                        # on the Scan struct: creator (Vec<String>,
│   │   │                                        # ArgAction::Append), metadata_comment, scan_target_name,
│   │   │                                        # metadata_file (Option<PathBuf>), plus the
│   │   │                                        # positional-pair --annotator / --annotation-comment
│   │   │                                        # parsing per research §3 (two parallel Vec<String>
│   │   │                                        # fields with post-validation).
│   │   └── run.rs                             # MODIFY (~80 LOC) — symmetric flag set on the
│   │                                            # trace run subcommand. Both delegate to the same
│   │                                            # parsing helper in binding/user_metadata.
│   ├── binding/
│   │   └── user_metadata/                     # NEW MODULE (~250 LOC)
│   │       ├── mod.rs                         # Public surface: UserMetadata struct,
│   │       │                                    # parse_creator_flag, parse_annotator_pairs,
│   │       │                                    # merge_file_and_flags.
│   │       ├── creator.rs                     # Creator { kind: CreatorKind, name: String }
│   │       │                                    # newtype + parse_creator_str("Type: Name").
│   │       ├── annotation.rs                  # Annotation { annotator: Creator, comment: String,
│   │       │                                    # timestamp: DateTime<Utc> }.
│   │       └── metadata_file.rs               # MetadataFile struct with
│   │                                            # #[serde(deny_unknown_fields)]; load + merge
│   │                                            # + conflict-detection logic.
│   └── generate/
│       ├── cyclonedx/
│       │   └── metadata.rs                    # MODIFY (~60 LOC) — extend build_metadata to accept
│       │                                        # &UserMetadata. Emit additional metadata.tools[]
│       │                                        # entries (Tools), append to metadata.authors[]
│       │                                        # (Persons), set metadata.manufacturer
│       │                                        # (Organizations — first one only; CDX permits
│       │                                        # exactly one). Append bom.annotations[] per Q2
│       │                                        # audit (if native) OR metadata.properties[]
│       │                                        # parity bridge.
│       ├── spdx/
│       │   ├── document.rs                    # MODIFY (~50 LOC) — SPDX 2.3 emission. Append
│       │   │                                    # creationInfo.creators[], set creationInfo.comment,
│       │   │                                    # append annotations[] array on the SpdxDocument.
│       │   │                                    # Override name field via --scan-target-name.
│       │   └── v3_document.rs                 # MODIFY (~80 LOC) — SPDX 3 emission. Add new
│       │                                        # Tool/Organization/Person Agent elements to
│       │                                        # @graph; reference them from CreationInfo.createdBy[]
│       │                                        # / createdUsing[] per the milestone-078 wire shape;
│       │                                        # add Annotation elements; override software_Sbom.name.
└── tests/
    ├── sbom_user_metadata.rs                  # NEW (~400 LOC) — 17 integration tests covering
                                                  # all 4 user stories + edge cases (see contract).
    └── spdx3_conformance.rs                   # MAY MODIFY — extend with one test verifying SPDX 3
                                                  # SBOMs with full metadata-flag set still pass
                                                  # spdx3-validate zero-violation per SC-008.

mikebom-cli/tests/fixtures/golden/                  # MODIFY — all 27 byte-identity goldens
├── cyclonedx/                                       # regenerate as the expected operator-visible
├── spdx-2.3/                                        # change of the milestone. Per-format diff is
└── spdx-3/                                          # bounded to the new metadata fields.

docs/reference/
├── sbom-format-mapping.md                          # MAYBE MODIFY (Q2 fallback only) — if Phase 0
│                                                    # §1 audit reveals CDX 1.6 lacks native support,
│                                                    # add new B-row or M-row documenting the
│                                                    # mikebom:invocation-comment / mikebom:annotation
│                                                    # parity bridges with justification clauses.
└── identifiers.md                                  # MAYBE MODIFY (small) — link from the
                                                     # milestone-077 root-name section to the new
                                                     # --scan-target-name flag for cross-reference.
```

**Structure Decision**: Single project. Adds one new module (`mikebom-cli/src/binding/user_metadata/` with 4 files) + extends the CLI definitions + the three format-emission code paths. No new crates; no new modules outside `binding/`; no new dependencies. The placement under `binding/` matches the milestone-073/074/075/076/077 identifier-binding structure, since user-supplied metadata is conceptually similar to user-supplied identifiers — both are operator inputs that get attached to the emitted SBOM at well-defined slots.

## Phase 0 — Research questions

Six implementation-level decisions to pin in `research.md`. The two highest-impact decisions (Q1 multi-annotation parsing; Q2 CDX fallback strategy) were locked during /speckit.clarify; this phase documents the per-format native field map, the CDX 1.6 audit result, and the parser implementation strategy.

1. **CDX 1.6 native annotations audit (per Q2 plan-time deferral)** — Verify against the actual CDX 1.6 JSON schema that `bom.annotations[]` exists at the document level and that its sub-fields (`subjects`, `annotator`, `timestamp`, `text`) accommodate what FR-002 and FR-003 need. Also audit `metadata.tools[]` / `metadata.authors[]` / `metadata.manufacturer` shapes for FR-001's creator routing. **The output of this section IS the per-flag native-vs-bridge decision matrix the implementation encodes.** If `bom.annotations[]` is confirmed native, the Q2 fallback is unused. If it's missing or insufficient, the parity bridge fires per Q2 with the exact `mikebom:invocation-comment` / `mikebom:annotation` keys named in the spec. **Audit method**: fetch the CDX 1.6 JSON schema (https://cyclonedx.org/schema/bom-1.6.schema.json) into `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6.json` (mirrors the SPDX schema fixtures pattern from milestones 011 + 012); grep the schema for `annotations`, `metadata.properties`, `metadata.tools`, `metadata.authors`, `metadata.manufacturer`; document each field's type + cardinality; cross-check against the OWASP CycloneDX 1.6 changelog at https://github.com/CycloneDX/specification/blob/master/CHANGELOG.md.

2. **Per-format creator-prefix routing table** — Define the definitive routing for each `Type: Name` prefix in each format:
   - **`Tool: <name>`** — CDX `metadata.tools[]` (append entry with `name`); SPDX 2.3 `creationInfo.creators[]` (append `Tool: <name>` string verbatim); SPDX 3 add `Tool` element to `@graph` with `creationInfo: _:creation-info` + `name: <name>`.
   - **`Organization: <name>`** — CDX `metadata.manufacturer` (set if not already; CDX 1.6 permits exactly one — first-wins with stderr warning if multiple); SPDX 2.3 `creationInfo.creators[]` (append `Organization: <name>` verbatim); SPDX 3 add `Organization` element to `@graph` (Agent subclass per milestone 078's pattern; symmetric with the existing `mikebom contributors` Organization).
   - **`Person: <name>`** — CDX `metadata.authors[]` (append `{name: <name>}`); SPDX 2.3 `creationInfo.creators[]` (append `Person: <name>` verbatim); SPDX 3 add `Person` element to `@graph`.

3. **Positional-pair clap parsing strategy** — clap's derive macro doesn't natively support pair-positional arguments. Decide between three implementation options: (a) define `--annotator` and `--annotation-comment` as two parallel `Vec<String>` fields and post-validate that the lengths match — clap preserves CLI insertion order for `ArgAction::Append`, so element-N of one vector pairs with element-N of the other; (b) define a single `Vec<(String, String)>` field with a custom `value_parser` that consumes both args from the raw arg-vector at parse time; (c) bypass clap derive for these two flags and use `clap::Arg::new` builder API for fine-grained control. **Recommend (a)** for simplicity and CLAUDE.md alignment with the existing `--component-id` flag style — clap preserves order, post-validation catches all ambiguity cases, and it stays within the derive macro for consistency. The validation logic asserts `creator.len() == annotation_comment.len()` and that they were interleaved 1:1 in CLI order (verifiable via `std::env::args()` walk if precise out-of-order detection is needed; otherwise trust clap's order preservation).

4. **`--metadata-file` JSON schema design** — Pin the exact field names + types:
   ```json
   {
     "creators": ["Tool: T1", "Person: Alice"],
     "annotators": [
       {"type_name": "Tool: reviewer", "comment": "Approved 2026-05-07"},
       {"type_name": "Organization: SecOps", "comment": "PCI scan complete"}
     ],
     "metadata_comment": "Release v1.0.0",
     "scan_target_name": "myproject"
   }
   ```
   Field naming: snake_case (matches Python-pipeline conventions; matches mikebom's existing `mikebom sbom enrich` JSON-input convention if applicable; verify at audit time). All fields optional. Unknown top-level fields rejected via `#[serde(deny_unknown_fields)]`. The `annotators[]` array shape (objects with `type_name` + `comment` keys) avoids the CLI flag-pair ambiguity entirely — explicit pairing in JSON.

5. **`--scan-target-name` interaction with milestone-077 `--root-name`** — Pin the precedence per spec FR-004:
   - **CDX 1.6**: `--scan-target-name` and `--root-name` BOTH target `metadata.component.name` per milestone 077's emission shape. When both are passed: `--root-name` takes precedence (it's the more-specific flag introduced explicitly for that field). Document this in `--scan-target-name`'s clap help text + emit a stderr warning when both are set.
   - **SPDX 2.3**: `--scan-target-name` sets the document `name` field (top-level on `SPDXDocument`). `--root-name` sets the root `Package` element's `name`. They target DIFFERENT fields — both honored independently when both passed.
   - **SPDX 3**: `--scan-target-name` sets `software_Sbom.name`. `--root-name` sets the root `software_Package.name`. Different fields again — both honored.

6. **Determinism contract for repeatable arrays** — Multiple `--creator` flags must produce a stable insertion order in the emitted JSON. clap preserves CLI insertion order; the per-format builders MUST iterate `UserMetadata.creators` in that order without sorting. Internal arrays (file-supplied creators merged with flag-supplied creators per FR-006) concat as `file_creators + flag_creators` for stable interleaving (file-supplied first, then flag-supplied). Document the order so operators can predict the output.

## Phase 1 — Design & contracts

### data-model.md

Three new internal types in `mikebom-cli/src/binding/user_metadata/`:
- `Creator { kind: CreatorKind, name: String }` — `CreatorKind` enum is `{Tool, Organization, Person}`.
- `Annotation { annotator: Creator, comment: String, timestamp: DateTime<Utc> }` — timestamp is the SBOM emission timestamp, deterministic per scan inputs (matches existing `creationInfo.created` semantics).
- `MetadataFile { creators: Vec<String>, annotators: Vec<{type_name, comment}>, metadata_comment: Option<String>, scan_target_name: Option<String> }` — `#[serde(deny_unknown_fields)]`.

Plus an aggregator `UserMetadata { creators: Vec<Creator>, annotations: Vec<Annotation>, metadata_comment: Option<String>, scan_target_name: Option<String> }` that the CLI parser populates from the merged file-and-flag inputs and the per-format builders consume.

### contracts/

One contract: `user-sbom-metadata.md`. Documents:
- The CLI surface (5 flags on both `mikebom sbom scan` and `mikebom trace run`).
- The per-format wire-format expectations from research §1 + §2.
- The `--metadata-file` JSON schema from research §4 (with examples).
- The `--root-name` interaction matrix from research §5.
- The 17-test integration matrix (each test mapped to user-story acceptance scenarios + FRs/SCs).

### quickstart.md

Operator-facing recipes:
1. **Replace the CNCF-style `jq` recipe** — before/after diff showing the `jq` invocation from issue #94 alongside the equivalent native-flag invocation.
2. **Add a single creator** — `mikebom sbom scan --path . --creator "Tool: my-pipeline" --format cyclonedx-json,spdx-2.3-json,spdx-3-json`.
3. **Add multi-annotation context** — `--annotator "Tool: reviewer" --annotation-comment "X" --annotator "Tool: scanner" --annotation-comment "Y"`.
4. **Use a sidecar metadata file** — example `meta.json` + the `--metadata-file meta.json` invocation.
5. **Inspect the post-fix wire shape** — `jq` queries showing where each flag's value lands in CDX vs SPDX 2.3 vs SPDX 3.
6. **Pre-PR gate behavior** — unchanged from milestone 078; the new fields auto-validate as part of the existing gate.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. `/speckit.tasks` consumes plan.md + spec.md + Phase 1 docs and emits `tasks.md`. Estimated task count: **~16–20** — larger than 078/079 because the milestone touches CLI surface + three format-emission code paths + a sidecar JSON-schema parser + 17 integration tests, but smaller than 077 because the operator-facing UX is simpler (no PURL-aware selectors).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all 12 principles + 4 strict boundaries with zero violations. Principle V audit explicitly cited per the v1.4.0 amendment requirement: native fields are the primary landing for every flag; `mikebom:` parity bridges are the documented Q2 fallback only when CDX 1.6 audit reveals insufficient native support.
