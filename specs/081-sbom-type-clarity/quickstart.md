# Quickstart — milestone 081 SBOM-type signaling clarity

Five operator-facing recipes covering CISA SBOM Type identification across all three formats, the new `--sbom-type` operator-assert flag, and the mixed-tier transparent-presentation rule.

## Recipe 1 — Identify the CISA SBOM Type from a mikebom-emitted document

The same conceptual SBOM type lives at three different field positions across the formats. Pick the right `jq` query for your format:

**CDX 1.6**:
```bash
jq '.metadata.lifecycles[] | .phase' out.cdx.json
# "pre-build"   ← maps to CISA Source per the equivalence table
```

**SPDX 2.3**:
```bash
jq -r '.creationInfo.comment' out.spdx.json
# "Scope: manifest (...). Observed lifecycle phases: pre-build."
# Parse the "Observed lifecycle phases:" suffix; map each CDX phase via the equivalence table.
```

**SPDX 3** (post-milestone-081):
```bash
jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# [
#   "spdx:Software/SbomType/source"   ← maps directly to CISA Source
# ]
```

The SPDX 3 path is the cleanest — direct enum value, no string parsing.

## Recipe 2 — CISA Type ↔ mikebom tier ↔ CDX phase ↔ SPDX 3 SbomType equivalence reference

Use this table to translate any of the format-native vocabularies to/from the CISA SBOM Types canonical names:

| CISA SBOM Type | mikebom tier | CDX 1.6 phase | SPDX 3 SbomType IRI |
|---|---|---|---|
| **Design** | `design` | `design` | `spdx:Software/SbomType/design` |
| **Source** | `source` | `pre-build` | `spdx:Software/SbomType/source` |
| **Build** | `build` | `build` | `spdx:Software/SbomType/build` |
| **Analyzed** | `analyzed` | `post-build` | `spdx:Software/SbomType/analyzed` |
| **Deployed** | `deployed` | `operations` | `spdx:Software/SbomType/deployed` |
| **Runtime** | `runtime` *(operator-asserted only)* | `operations` *(closest CDX)* | `spdx:Software/SbomType/runtime` |

Note: CDX defines two additional phases (`discovery`, `decommission`) that don't map to the CISA framework; mikebom doesn't auto-emit them.

## Recipe 3 — Operator self-assertion via `--sbom-type`

When your pipeline knows the SBOM should be classified as a single CISA type regardless of mikebom's per-component auto-detection (e.g., a CI/CD pipeline producing a Build SBOM where you want downstream policy tools to classify the entire document as Build):

```bash
mikebom sbom scan --path . --sbom-type build \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=out.cdx.json \
  --output spdx-2.3-json=out.spdx.json \
  --output spdx-3-json=out.spdx3.json

# Verify single-type assertion in each format
jq '.metadata.lifecycles' out.cdx.json
# [{"phase": "build"}]   ← single element

jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# ["spdx:Software/SbomType/build"]   ← single element
```

**Important**: the operator override is **document-level only**. Per-component `mikebom:sbom-tier` annotations preserve auto-detected values:

```bash
jq '.["@graph"][] | select(.type == "software_Package") | .annotations[]?.statement' out.spdx3.json
# Per-component tiers may still show "source" / "build" / etc. per actual data lineage.
# The operator's --sbom-type build assertion is a CLAIM about the document's primary type
# for downstream-consumer classification; it does NOT rewrite per-component data.
```

Vocab: exactly `{design, source, build, analyzed, deployed, runtime}` (case-sensitive).

## Recipe 4 — Interpret a Mixed-type SBOM (per the 2026-05-07 Q1 clarification)

A polyglot scan may produce some components tagged `source` (manifest-derived) AND others tagged `build` (artifact-derived). mikebom presents this transparently:

```bash
mikebom sbom scan --path /path/with/mixed-tier-components --output out.spdx3.json --format spdx-3-json

jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# [
#   "spdx:Software/SbomType/build",
#   "spdx:Software/SbomType/source"
# ]
```

This is a **Mixed-type SBOM** — it spans multiple CISA types simultaneously. mikebom does NOT invent a "dominant tier" heuristic to collapse this to a single type.

If your downstream pipeline requires a single SBOM type (regulatory dashboards expecting one CISA type per document, compliance tools that hard-fail on multi-type lifecycles), use `--sbom-type <type>` (Recipe 3) to assert the operator-facing primary type. The per-component data lineage stays accurate.

## Recipe 5 — Pre-PR gate behavior

Identical to milestone 078 / 079 / 080 — no new local-dev workflow:

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# >>> cargo +stable clippy --workspace --all-targets -- -D warnings
# >>> cargo +stable test --workspace
# (during cargo test, sbom_type_signaling runs ~11 tests covering
#  every flag and edge case; the milestone-078 conformance gate
#  continues to verify SPDX 3 SBOMs with the new software_sbomType[]
#  pass spdx3-validate zero-violation)
# >>> all pre-PR checks passed.
```

## What's NOT changed by this milestone

- **CDX 1.6 emission**: unchanged from milestone 047. Native `metadata.lifecycles[]` already wired.
- **SPDX 2.3 emission**: unchanged from milestone 047. No native SBOM-type enum exists in the spec.
- **CDX 1.6 + SPDX 2.3 byte-identity goldens**: stay byte-identical (no emission change for those formats).
- **mikebom's auto-detection logic**: unchanged. Per-component `mikebom:sbom-tier` annotations are still derived from the same milestone-047 logic.
- **`runtime` tier auto-detection**: deferred. mikebom's eBPF observes builds, not runtime; auto-emitting `runtime` requires a future runtime-observation feature.
- **Validator pin**: `spdx3-validate==0.0.5` per milestone 078. No bump.
