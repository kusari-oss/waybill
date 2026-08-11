# Contract: `--tier=<mode>` CLI flag

**Feature**: 232-tier-filter-flag
**Phase**: 1
**Audience**: Operators using `waybill sbom scan` and downstream consumers building CI pipelines around waybill's output.

Documents the CLI surface, valid values, defaults, and downstream-observable behavior for the `--tier` flag.

## Flag surface

```
--tier=<MODE>    Filter emitted components by sbom_tier.

Modes:
  all                 (default) Emit all resolved components regardless
                      of tier. Byte-identical to pre-232 emission for
                      any given scan input.

  source-only         Emit only components tagged sbom_tier: "source".
                      Recommended for vulnerability-scanner pipelines
                      (Trivy, Grype, Snyk) that want resolved versions
                      only and treat versionless PURLs as noise.

  design-only         Emit only components tagged sbom_tier: "design".
                      Recommended for compliance-attribution pipelines
                      that want the developer-declared graph without
                      resolver-tier probes.

  source-and-binary   Emit components tagged sbom_tier: "source" OR
                      "binary". Recommended for container-artifact
                      pipelines that want everything "actually shipped"
                      but not "declared but not resolved".
```

## Valid inputs

Case-insensitive at parse time (clap normalizes kebab-case). Rejected values (e.g., `--tier=SourceOnly` with camelCase, `--tier=all,source-only` with a comma) fail with the standard clap `--help`-style error.

## Composition with other flags

Per spec Clarifications §2, no CLI-parse-level mutual exclusions are enforced. Every combination is accepted; degenerate combinations produce a WARN log line and continue.

Concrete compositions:

| `--tier` + `<other>` | Behavior |
|---|---|
| `--tier=source-only --sbom-type=application` | Both flags apply. If the filter drops the main-module component the SBOM subject is derived from, the resulting SBOM emits with subject fields set to fallback values and a WARN log line notes the outcome. |
| `--tier=design-only --sign-key <path>` | Signing runs over the filtered output. Signature verification post-scan will reflect the filtered content — that's what the operator asked for. |
| `--tier=<any> --split` | Filter runs on each split boundary's component slice INDEPENDENTLY. A split whose components are all filtered out still emits its manifest entry with an empty components array. |
| `--tier=<any> --offline` | Orthogonal; offline flag governs data fetching, tier flag governs emission. |
| `--tier=<any> --supplement-cdx <path>` | Supplement is merged BEFORE the tier filter runs; supplement components are subject to the same filter as scan-emitted components. |

## Downstream-observable behavior

### FR-002 byte-parity guarantee (default mode)

When `--tier=all` (or the flag is omitted), the emitted SBOM is byte-identical to the pre-232 emission for the same scan input. Testable by:

```bash
waybill sbom scan --path <input> --output pre.cdx.json      # pre-232 binary
waybill sbom scan --path <input> --output post.cdx.json     # post-232 binary
waybill sbom scan --path <input> --tier=all --output post-explicit.cdx.json
diff <(mask-nondeterministic pre.cdx.json) <(mask-nondeterministic post.cdx.json)
diff <(mask-nondeterministic post.cdx.json) <(mask-nondeterministic post-explicit.cdx.json)
# Both diffs expected empty.
```

### Non-default modes

Each non-default mode produces an SBOM whose `components[]` is a strict subset of the `--tier=all` output. Every dropped component's PURL disappears from:

- `components[]`
- `dependencies[]` `ref` and `dependsOn` (CDX)
- `relationships[]` `spdxElementId` and `relatedSpdxElement` (SPDX 2.3)
- `Relationship` `from` and `to` (SPDX 3)

### Document-scope annotation re-evaluation

Every document-scope annotation whose value depends on iterating `components` MUST re-evaluate against the filtered set. Concrete list:

| Annotation | Emitted by | Post-filter behavior |
|---|---|---|
| `waybill:graph-completeness` | CDX + SPDX 2.3 + SPDX 3 | Recomputed via `compute_graph_completeness(filtered_components, filtered_relationships)`. Values may change (e.g., pre-filter "partial" → post-filter "complete" when design-tier orphans were the only orphans). |
| `waybill:graph-completeness-reason` | Same three | Recomputed alongside. Reason codes may disappear (e.g., `multi-ecosystem-partial-root: X` drops if the filter removed every component of ecosystem X). |
| `waybill:workspaces-detected` | Metadata | Recomputed. Workspaces whose main-modules are all filtered out drop from the list. |
| `waybill:cisa-2026-lifecycle` | Metadata | Recomputed. |

### Empty-result path (FR-008)

When the filter drops every component, the scan:

1. Emits an SBOM with empty `components[]` and empty `dependencies[]`.
2. Logs a WARN line: `"tier filter dropped all components; emitting empty SBOM. mode=<mode>"`.
3. Exits 0 (not an error).

This mirrors the `--exclude-scope` filter's existing behavior.

## Backward compatibility

- Pre-232 scans that don't use `--tier` continue to work identically (FR-002).
- Existing tests that don't set `--tier` continue to pass unmodified.
- Existing goldens do not need regeneration (default mode's output is byte-identical).

## Verification recipes

For each mode, the following jq assertion holds on the emitted CDX:

```bash
# source-only
jq -r '.components[] | select(.properties // [] | any(.name == "waybill:sbom-tier" and .value != "source")) | .purl' out.cdx.json
# Expect: empty
```

```bash
# design-only
jq -r '.components[] | select(.properties // [] | any(.name == "waybill:sbom-tier" and .value != "design")) | .purl' out.cdx.json
# Expect: empty
```

```bash
# source-and-binary
jq -r '.components[] | select(.properties // [] | all((.name != "waybill:sbom-tier") or (.value != "source" and .value != "binary"))) | .purl' out.cdx.json
# Expect: empty
```
