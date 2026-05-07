# Contract — milestone 081 SBOM-type signaling clarity

The milestone's only contract.

## CLI surface

**One new flag** on both `mikebom sbom scan` and `mikebom trace run`:

| Flag | Type | Repeatable | Default |
|---|---|---|---|
| `--sbom-type <type>` | enum (1 of 6 CISA types) | no (single-valued) | none (auto-detection) |

**Vocab validation**: `<type>` ∈ `{design, source, build, analyzed, deployed, runtime}` (case-sensitive). Any other value fails parsing with `"--sbom-type 'X' is not a valid CISA SBOM type; valid values are design/source/build/analyzed/deployed/runtime"`.

## Library surface (`mikebom-cli` crate)

**No new public Rust API.** All new items are `pub(crate)` extensions to the existing `lifecycle_phases.rs`:
- `SbomType` enum + `as_spdx3_iri` / `as_str` / `parse_str` methods
- `tier_to_spdx3_sbomtype_iri(tier: &str) -> Option<&'static str>` helper
- `aggregate_spdx3_sbom_types(components, override_assertion: Option<SbomType>) -> Vec<&'static str>` helper

## Wire-format contract (per format)

### CDX 1.6 — UNCHANGED

Per research §1: native `metadata.lifecycles[]` already wired by milestone 047. No emission change in this milestone.

When `--sbom-type` is asserted: `aggregate_phases` (extended with the new override-assertion parameter) returns a single-element Vec, and `metadata.lifecycles[]` becomes `[{phase: "<asserted-cdx-phase>"}]` per the equivalence table.

### SPDX 2.3 — UNCHANGED structurally

Per research §1: no native single-document-type enum; `creationInfo.comment` continues to carry the aggregated phase set. No emission shape change.

When `--sbom-type` is asserted: the `comment` aggregator returns single-element output, and `creationInfo.comment` reflects the operator-asserted phase only.

### SPDX 3 — NEW `software_Sbom.software_sbomType[]` field

```json
{
  "type": "software_Sbom",
  "spdxId": "...",
  "rootElement": [...],
  "name": "...",
  "comment": "...",
  "software_sbomType": [
    "spdx:Software/SbomType/<one-of-6-types>",
    "spdx:Software/SbomType/<one-of-6-types>"
  ]
}
```

Field shape per the SPDX 3 schema (`software_Sbom_props.properties.software_sbomType`): array of IRIs from the 6-value enum.

When `--sbom-type` is asserted: single-element array.
When auto-detected from per-component tiers: aggregated multi-element array.
When no components carry tiers (empty SBOM, etc.): field OMITTED entirely.

Sort: lexicographic per the existing milestone-047 `BTreeSet`-backed pattern.

## Equivalence table (the operator-facing centerpiece)

| CISA SBOM Type | mikebom tier | CDX 1.6 phase | SPDX 3 SbomType IRI |
|---|---|---|---|
| Design | `design` | `design` | `spdx:Software/SbomType/design` |
| Source | `source` | `pre-build` | `spdx:Software/SbomType/source` |
| Build | `build` | `build` | `spdx:Software/SbomType/build` |
| Analyzed | `analyzed` | `post-build` | `spdx:Software/SbomType/analyzed` |
| Deployed | `deployed` | `operations` | `spdx:Software/SbomType/deployed` |
| Runtime | `runtime` *(not auto-detected)* | `operations` *(closest CDX equivalent)* | `spdx:Software/SbomType/runtime` |

## Determinism contract

- Same flag inputs + same scan inputs → byte-identical SBOMs across re-runs.
- `aggregate_spdx3_sbom_types` uses `BTreeSet`-backed lexicographic ordering (matches the existing `aggregate_phases` contract).
- Single source-of-truth aggregation: CDX `metadata.lifecycles[]` and SPDX 3 `software_sbomType[]` aggregate from the same `mikebom:sbom-tier` per-component values via the same helper module.

## Test contract

A new file `mikebom-cli/tests/sbom_type_signaling.rs` MUST cover:

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `spdx3_sbomtype_emitted_natively_for_source_tier` | US1 §1 | FR-002 + native SPDX 3 wiring |
| `spdx3_sbomtype_emitted_natively_for_build_tier` | US1 §2 | FR-002 + native SPDX 3 wiring |
| `spdx3_sbomtype_aggregates_mixed_tiers` | US1 §3, Q1 mixed-tier | FR-002 + research §5 determinism |
| `cdx_lifecycles_unchanged_from_milestone_047` | (regression) | FR-006 (CDX byte-identity) |
| `spdx2_comment_aggregation_unchanged` | (regression) | FR-006 (SPDX 2.3 byte-identity) |
| `sbom_type_flag_overrides_spdx3_native` | US3 §1, SC-004 | FR-003 + FR-004 |
| `sbom_type_flag_overrides_cdx_lifecycles` | US3 §1 (CDX path) | FR-003 + FR-004 |
| `sbom_type_flag_preserves_per_component_tiers` | US3 §2, SC-005 | FR-005 + research §4 override semantics |
| `sbom_type_invalid_value_fails_parse` | US3 §3, SC-006 | VR-081-001 |
| `spdx3_conformance_with_native_sbomtype` | (cross-cutting) | FR-010 (milestone-078 SHACL gate) |
| `schema_validation_passes_per_format` | (cross-cutting) | FR-010 (CDX 1.6 + SPDX 2.3 + SPDX 3 schemas) |

## Performance contract

- `tier_to_spdx3_sbomtype_iri`: O(1) match expression. Negligible.
- `aggregate_spdx3_sbom_types`: O(N_components) — same as existing `aggregate_phases`. The 6-value vocab keeps the result Vec bounded to 6 elements.
- Integration test wall-time: ~10s for the new file (most tests are fast emission + JSON-parse assertions; `spdx3_conformance_with_native_sbomtype` is the only one that shells out to `spdx3-validate`).
- Determinism: re-running tests with same flag inputs produces byte-identical results.

## Observable contract

### Pre-fix: SPDX 3 SBOM-type signaling reachable only via comment grep

```bash
$ jq '.["@graph"][] | select(.type == "software_Sbom") | {comment, software_sbomType}' out.spdx3.json
{
  "comment": "Scope: manifest (...). Observed lifecycle phases: pre-build.",
  "software_sbomType": null    # ← native field not emitted
}
```

### Post-fix: SPDX 3 native field emitted alongside comment

```bash
$ jq '.["@graph"][] | select(.type == "software_Sbom") | {comment, software_sbomType}' out.spdx3.json
{
  "comment": "Scope: manifest (...). Observed lifecycle phases: pre-build.",
  "software_sbomType": [
    "spdx:Software/SbomType/source"   # ← native field emitted per Principle V
  ]
}
```

### Operator override

```bash
$ mikebom sbom scan --path . --sbom-type build --output out.spdx3.json --format spdx-3-json
$ jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
[
  "spdx:Software/SbomType/build"   # ← single-element override regardless of per-component tiers
]
```
