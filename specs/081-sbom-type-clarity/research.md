# Research — milestone 081 SBOM-type signaling clarity

Five implementation-level findings, validated against the actual format specs at /speckit.plan time. The audit IS the research deliverable.

## §1 — Per-format native-field audit (definitive)

**Decision matrix**:

| Format | Native field for document-level SBOM type | mikebom emits today | Action this milestone |
|---|---|---|---|
| CDX 1.6 | `metadata.lifecycles[].phase` (enum: `design`/`pre-build`/`build`/`post-build`/`operations`/`discovery`/`decommission`) | YES — milestone 047 wires `mikebom:sbom-tier` → CDX phase via `lifecycle_phases.rs::tier_to_phase` and emits `metadata.lifecycles[]` natively at `cyclonedx/metadata.rs:73`+ | **No code change.** Documented in `docs/reference/sbom-types.md`. |
| SPDX 2.3 | NO native single-document-type enum. The spec offers `creationInfo.comment` (free-text) and per-component `Package.relationship` types but no document-level SBOM-type enum. | mikebom uses `creationInfo.comment` carrying the milestone-047 aggregated phase set | **No code change.** Principle V escape clause appropriate (no native field exists to promote to). Documented as such in `docs/reference/sbom-format-mapping.md`. |
| SPDX 3 | **`software_Sbom.software_sbomType` — array property, items from enum `{spdx:Software/SbomType/analyzed, build, deployed, design, runtime, source}`** (per `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json::software_Sbom_props`) | NO — mikebom only emits `comment` aggregation at the SpdxDocument level | **CODE CHANGE.** Wire `software_Sbom.software_sbomType[]` from the same aggregated tier set already used for CDX `metadata.lifecycles[]`. Per Constitution Principle V's standards-native-precedence requirement. |

**Evidence**:
- CDX 1.6 schema (https://cyclonedx.org/schema/bom-1.6.schema.json) `phase` enum: `["design", "pre-build", "build", "post-build", "operations", "discovery", "decommission"]` with documented mapping to OBOM/HBOM concepts.
- SPDX 2.3 spec (`spdx-spec/v2.3/`) — no document-level SBOM-type enum. The closest is `Document.creationInfo.creators[]` Tool entries (which signal "what produced this SBOM" not "what type of SBOM this is").
- SPDX 3 schema (`mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json` — vendored milestone 078): `software_SbomType_derived` enumerates `{analyzed, build, deployed, design, runtime, source}` — exactly 6 values, exactly the 6 CISA SBOM Types.

**Rationale**: Native-first per Constitution Principle V. The audit reveals one actionable gap (SPDX 3) and confirms the other two formats are clean. The fix is bounded: 3-file diff, ~80 LOC total.

**Alternatives considered**:
- Skip the SPDX 3 native field promotion — Rejected: violates Principle V's standards-native-precedence requirement when a perfect 1:1 native field exists. The audit-record requirement means a future maintainer would re-discover this gap and ask why it wasn't fixed.
- Promote SPDX 2.3 emission via `mikebom:sbom-type` annotation — Rejected: SPDX 2.3 has no native field, so a `mikebom:` annotation is permitted per Principle V's escape clause, BUT the existing `comment` aggregation already serves the operator-facing function. Adding a redundant annotation would duplicate without value.

## §2 — CISA SBOM Types ↔ mikebom tier ↔ CDX phase ↔ SPDX 3 SbomType equivalence table

**Decision**: Definitive four-column equivalence table, validated against each spec's published vocabulary.

| CISA SBOM Type | mikebom tier (`mikebom:sbom-tier`) | CDX 1.6 phase (`metadata.lifecycles[].phase`) | SPDX 3 SbomType (`software_Sbom.software_sbomType[]`) |
|---|---|---|---|
| Design | `design` | `design` | `spdx:Software/SbomType/design` |
| Source | `source` | `pre-build` | `spdx:Software/SbomType/source` |
| Build | `build` | `build` | `spdx:Software/SbomType/build` |
| Analyzed | `analyzed` | `post-build` | `spdx:Software/SbomType/analyzed` |
| Deployed | `deployed` | `operations` | `spdx:Software/SbomType/deployed` |
| Runtime | `runtime` *(not auto-detected this milestone; accepted by `--sbom-type` flag)* | `operations` *(CDX has no `runtime` phase; closest match per OBOM narrative)* | `spdx:Software/SbomType/runtime` |

**Notes**:
- CDX has TWO additional phases (`discovery`, `decommission`) that don't map to CISA SBOM Types. Out of scope; documented as CDX-specific.
- SPDX 3 SbomType uses lowercase IRIs matching mikebom's lowercase tier vocabulary (consistent across the equivalence chain).
- The `runtime` row is the only chain where CDX lacks an exact native equivalent — operators wanting to express Runtime SBOMs in CDX get `metadata.lifecycles[{phase: "operations"}]` (CDX's closest semantic per OBOM definition). SPDX 3 expresses Runtime cleanly.

**Rationale**: A definitive equivalence table is the operator-facing deliverable's centerpiece. Validates per-format claims with primary sources.

**Alternatives considered**:
- Map mikebom `deployed` → CDX `operations` AND SPDX 3 `deployed` (current proposal) vs map `deployed` → CDX `operations` AND SPDX 3 `operations` (would require an `operations` SbomType variant) — Rejected: SPDX 3 doesn't define `operations` in its SbomType enum; `deployed` is the spec-correct value.

## §3 — `runtime` tier auto-detection scope decision

**Decision**: `runtime` is NOT auto-detected by mikebom in any current scan mode. The `--sbom-type` flag (US3) accepts `runtime` as a valid vocab value (per the 6-value CISA spec); auto-detection adding `runtime` to per-component `mikebom:sbom-tier` annotations is **deferred to a separate milestone**.

**Rationale**:
- mikebom's eBPF trace path observes the BUILD process producing artifacts. Per the CISA SBOM Types document (April 2023, p. 4): "Runtime: SBOM created from instrumentation of the system running the software (e.g., to capture only components loaded into memory and external call-outs to other systems)." mikebom's eBPF observes builds, not the runtime of the artifacts those builds produce. A "Runtime SBOM" per CISA requires runtime-introspection capability mikebom doesn't have today.
- An operator running mikebom in their own runtime-instrumentation pipeline (e.g., a wrapper that runs the artifact + collects observed dependencies via syscall trace) CAN use `--sbom-type runtime` to assert the result is a Runtime SBOM, even if mikebom's auto-detection wouldn't tag the components as `runtime`.
- Adding `runtime` auto-detection to mikebom requires a new feature: a real runtime-observation mode separate from the build-tier eBPF trace. That's a multi-milestone effort, NOT a small extension of milestone 047's tier vocabulary.
- File a follow-up GitHub issue: "mikebom runtime-observation mode for native Runtime SBOM emission" — captures the deferred work.

**Alternatives considered**:
- Add `runtime` to the `mikebom:sbom-tier` enum + auto-detect via existing eBPF trace — Rejected: violates CISA Runtime semantics (see above). Would produce factually wrong SBOMs.
- Skip `runtime` from the `--sbom-type` flag's vocab entirely — Rejected: operators with their own runtime pipelines lose the standards-native landing for their assertions. Better to accept the value at the flag layer + leave auto-detection for a separate milestone.

## §4 — `--sbom-type` operator-assert flag implementation

**Decision**: Single-flag, single-value, optional, on both `mikebom sbom scan` and `mikebom trace run`. Vocab: `{design, source, build, analyzed, deployed, runtime}` (the 6 CISA types).

**Override semantics** (per spec US3 §2 + Q1):
- **Document-level override only.** When `--sbom-type build` is asserted:
  - CDX `metadata.lifecycles[]` becomes a single-element array with the operator-asserted CDX-phase mapping (`{phase: "build"}`).
  - SPDX 2.3 `creationInfo.comment` includes the operator-asserted phase verbatim (single-element).
  - SPDX 3 `software_Sbom.software_sbomType[]` becomes a single-element array with the operator-asserted IRI (`["spdx:Software/SbomType/build"]`).
- **Per-component `mikebom:sbom-tier` annotations are PRESERVED from auto-detection** — the operator's document-level assertion does NOT back-propagate. If auto-detection tagged some components as `source` and others as `build`, those per-component values stay even when the operator asserts `--sbom-type build` at the document level. Rationale: the operator's assertion is a CLAIM about the SBOM's primary type for downstream-consumer classification; the per-component data lineage stays accurate.

**Validation**:
- Vocab membership: `{design, source, build, analyzed, deployed, runtime}` (case-sensitive). Any other value fails parsing with `"--sbom-type 'X' is not a valid CISA SBOM type; valid values are design/source/build/analyzed/deployed/runtime"`.
- Mutual exclusion: none. The flag has no conflicting siblings.

**Implementation pattern**: clap derive with custom `value_parser` reusing `SbomType::parse_str` from the new helper. Same pattern as milestone-077's `--root-name` validation + milestone-080's `--creator` parsing.

**Rationale**: Smallest operator-facing surface that closes the gap. The 6-value vocab matches both CISA + SPDX 3 SbomType verbatim. Override semantics preserve transparency (per-component data stays accurate; document-level claim is the operator's responsibility).

**Alternatives considered**:
- Multi-value `--sbom-type build,source` to assert mixed-type — Rejected: operators wanting mixed-type can simply NOT pass the flag (auto-detection produces mixed-type from per-component aggregation). The flag's purpose is single-type assertion.
- Back-propagate operator assertion to per-component annotations — Rejected: violates Principle X (Transparency); would silently overwrite auto-detected per-component data with the operator's blanket claim.

## §5 — Determinism contract for SPDX 3 emission

**Decision**: SPDX 3 `software_Sbom.software_sbomType[]` array uses lexicographic sort order matching the existing milestone-047 `aggregate_phases` `BTreeSet`-backed ordering. Same source-of-truth helper (`lifecycle_phases.rs`) means CDX + SPDX 3 emit values in the same deterministic order.

**Backward compatibility**:
- All existing milestone-047 byte-identity goldens for CDX 1.6 + SPDX 2.3 stay byte-identical (no emission change for those formats).
- All 9 SPDX 3 byte-identity goldens regenerate as the expected operator-visible change of the milestone (each fixture gains the new `software_sbomType[]` field on its SpdxDocument element). Per-fixture diff: +1 array on the SpdxDocument element. No other structural changes.
- The SPDX 3 SHACL conformance gate from milestone 078 continues to pass (the new field is spec-conformant per the schema audit).

**Rationale**: Reuses the proven milestone-047 aggregation logic. Single source-of-truth for both CDX + SPDX 3 emission means there's no chance of CDX + SPDX 3 disagreeing on what the SBOM type set is.

**Alternatives considered**:
- Sort by CISA's documented type-progression order (Design < Source < Build < Analyzed < Deployed < Runtime) — Rejected: lexicographic is already the milestone-047 contract for `aggregate_phases`; introducing a different order for SPDX 3 would split the source of truth and risk drift.
- Cache the aggregated set across CDX + SPDX 3 emission — Rejected: each format builder calls the helper independently; caching adds complexity for negligible gain (the aggregation is O(N_components × small_constant)).
