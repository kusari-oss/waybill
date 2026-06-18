# Contract: CLI behavior

This feature adds no CLI flags, no env vars, and no commands. The behavior contract is implicit: the *existing* `mikebom sbom scan` command's `metadata.component` / `documentDescribes` / `rootElement` output changes shape under the conditions enumerated in spec FR-001–FR-012.

## Affected command

```text
mikebom sbom scan --path <source-tree> --format <cyclonedx-json|spdx-2.3-json|spdx-3-json> [--output <path>]
```

## Pre-feature behavior (today, alpha.48)

When the scanned tree contains multiple `mikebom:component-role: "main-module"`-tagged components AND no `--root-name` override is set, the metadata.component priority ladder at `mikebom-cli/src/generate/cyclonedx/metadata.rs:269-309`:

1. Override active? → emit override (milestone 077).
2. Exactly one main-module? → promote it. ✓ correct for single-module projects.
3. Maven `scan_target_coord` set? → emit Maven coord. ⚠ wrong for polyglot (#366: argo-workflows picks the Java test client).
4. Default `pkg:generic/<target>@0.0.0` placeholder. ⚠ wrong for multi-module Go workspaces (#367: otel-collector picks the Java... wait, picks a Go submodule — actually the ladder falls through to a different sub-module main-module via the `else { None }` branch at line 256 producing fallthrough, then another main-module wins; the reality is subtle. See the data-model.md path.).

The two bug classes (#366, #367) both arise from this ladder selecting either the Maven coord (#366) or an alphabetic-leaf Go sub-module (#367) instead of the user's intended primary deliverable.

## Post-feature behavior

The same command, same flags, same arguments. The internal ladder gains four new branches between (2) and (3):

1. Override active? → emit override (milestone 077). **unchanged**.
2. Exactly one main-module? → promote it. **unchanged**. SC-003: byte-identical output for all 33 alpha.48 goldens.
3. **NEW**: Exactly one main-module has `is_workspace_root == true`? → promote it. Confidence 0.95.
4. **NEW**: ≥2 main-modules have `is_workspace_root == true`? → ecosystem-priority `[golang, cargo, maven, npm, pip, gem, generic]` first match. Confidence 0.70. Warn naming losers.
5. **NEW**: ≥1 main-module but none at repo root? → longest-common-prefix tiebreaker; exactly one wins. Confidence 0.80. Warn naming losers.
6. **NEW**: LCP has no unique winner OR no main-module is workspace-root AND scan_target_coord set? → Maven coord. Confidence 0.60. Warn naming losers.
7. Default `pkg:generic/<target>@0.0.0` placeholder. Confidence 0.30. Warn naming losers.

## Output contract changes

### CycloneDX 1.6

When ladder branches 3–7 fire, the emitted document gains a single new `metadata.properties[]` entry:

```json
{
  "name": "mikebom:root-selection-heuristic",
  "value": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:root-selection-heuristic\",\"value\":{\"heuristic\":\"repo-root-main-module\",\"confidence\":0.95}}"
}
```

The `metadata.component.purl`, `bom-ref`, `name`, `version`, `supplier`, `cpe`, `properties`, and `externalReferences` fields all carry the *correctly-selected* root's data (currently the wrong-root data).

### SPDX 2.3

When ladder branches 3–7 fire, the emitted document gains a single new document-level entry in `annotations[]`:

```json
{
  "annotator": "Tool: mikebom-<version>",
  "annotationType": "OTHER",
  "annotationDate": "<scan-emission-time>",
  "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:root-selection-heuristic\",\"value\":{\"heuristic\":\"ecosystem-priority\",\"confidence\":0.70}}"
}
```

The `documentDescribes[]` array's first entry refers to the SPDXID of the *correctly-selected* root package.

### SPDX 3.0.1

When ladder branches 3–7 fire, the emitted document gains a top-level annotation:

```json
{
  "type": "Annotation",
  "spdxId": "<deterministic>",
  "annotationType": "other",
  "subject": "<the-root-element-spdxid>",
  "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:root-selection-heuristic\",\"value\":{\"heuristic\":\"longest-common-prefix\",\"confidence\":0.80}}"
}
```

The `rootElement` ref carries the SPDX ID of the *correctly-selected* root element.

## Behavior under override (FR-008)

When the operator passes `--root-name` (and optionally `--root-version`, `--root-purl-type`, or `--no-root-purl`), the override wins over every new heuristic. The annotation is NOT emitted. This is the milestone-077 audit channel taking precedence — exact same behavior as today's override path.

## Behavior on the count==1 fast path (FR-009)

When exactly one main-module exists in the scan output, the fast path fires and the new annotation is NOT emitted. Byte-identical output to alpha.48 for every single-main-module project — verified by SC-003 re-running the cdx_regression / spdx_regression / spdx3_regression suites with NO `MIKEBOM_UPDATE_*` env vars.

## Behavior under `--bind-to-source` (FR-011)

The new heuristic also selects the `SourceDocumentBinding` envelope subject. Operator scripts that today bind to the wrong subject on argo-workflows (Maven coord) or otel-collector (alphabetic-leaf submodule) MUST be updated. Behavior change flagged in the CHANGELOG.

## Stderr behavior (FR-007)

When ladder branches 4 / 5 / 6 / 7 fire AND ≥1 main-module was detected, a `tracing::warn!` log entry is emitted at scan-end with the shape:

```text
WARN  mikebom::generate::root_selector: root-component selected via "<heuristic-name>" heuristic (confidence <value>); operator override recommended for deterministic identity
  selected = <selected-purl>
  losers = [<purl1>, <purl2>, ...]
  hint = "pass --root-name and --root-purl-type to override"
```

Operators using `MIKEBOM_LOG=info` or higher see this; `MIKEBOM_LOG=error` suppresses it (per existing tracing conventions).
