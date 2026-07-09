# Contract: C120 + C121 wire shapes across CDX / SPDX 2.3 / SPDX 3

**Feature**: 176-workspace-visibility
**Date**: 2026-07-08

Authoritative wire-shape reference. Deviations in emitted output are grounds for review comment.

## C120 — `mikebom:workspace-member` (per-component)

**Scope**: individual `Component` / `Package` / `software_Package` (per format).

**Emitted iff**: `derive_workspace_root(evidence.source_file_paths[i], scan_root_abs)` yielded at least one `Some(_)` for at least one entry in the component's `source_file_paths` — non-empty deduplicated set. File-tier and other unattributable components → annotation ABSENT.

**Value shape**: JSON-encoded array in a string. Records alphabetically sorted, deduplicated. Forward-slash separators (Windows normalized per FR-010).

**Value regex**: `^\[(\"[^\"]+\"(,\"[^\"]+\")*)?\]$`.

### CDX 1.6

```json
{
  "name": "mikebom:workspace-member",
  "value": "[\"src/frontend\",\"src/lfx\"]"
}
```

### SPDX 2.3

Envelope-wrapped in `Package.annotations[]`:

```json
{
  "annotationType": "OTHER",
  "annotator": "Tool: mikebom-0.1.0-alpha.NN",
  "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:workspace-member\",\"value\":\"[\\\"src/frontend\\\"]\"}"
}
```

### SPDX 3.0.1

Typed `Annotation` graph element with `subject` set to the Package IRI.

## C121 — `mikebom:workspaces-detected` (doc-scope)

**Scope**: document-scope (SBOM metadata).

**Emitted iff**: at least one component in the emitted SBOM has a `mikebom:workspace-member` annotation (union non-empty). Absent when zero workspaces detected.

**Value shape**: same JSON-encoded array in a string.

**Cross-annotation invariant**: `C121.value` MUST equal the union of every component's `C120.value`, alphabetically sorted, deduplicated. Verified by SC-007 integration-test cross-check.

### CDX 1.6

```json
{
  "name": "mikebom:workspaces-detected",
  "value": "[\".\",\"src/frontend\",\"src/lfx\"]"
}
```

Located in `metadata.properties[]`.

### SPDX 2.3

Envelope-wrapped in document-scope `annotations[]` on the `SpdxDocument`.

### SPDX 3.0.1

Typed `Annotation` graph element with `subject` set to the `SpdxDocument` root IRI.

## Advisory log line

**Location**: stderr (INFO-level `tracing::info!`).

**Emitted iff**: `workspaces_detected.len() > 1 && !components.is_empty()`. NOT gated on `--offline`.

**Stable grep substring**: `monorepo shape detected: `.

**Full form (subject to authoring refinement)**:

```
monorepo shape detected: 3 workspaces (docs, src/frontend, src/lfx). Downstream consumers can filter per-workspace via `mikebom:workspace-member`; see docs/reference/monorepos.md for jq recipes.
```

## Consumer jq recipes

### Recipe 1 — enumerate all workspaces in an SBOM

```jq
.metadata.properties[]?
| select(.name == "mikebom:workspaces-detected")
| .value | fromjson
```

Returns the workspace list. Missing → not a workspace-containing scan.

### Recipe 2 — filter components by workspace

```jq
.components[]
| select(
    (.properties[]? | select(.name == "mikebom:workspace-member") | .value | fromjson | contains(["src/frontend"]))
  )
| .purl
```

Returns all component PURLs belonging to the `src/frontend` workspace (including cross-workspace shared components).

### Recipe 3 — per-workspace CVE scoping

```jq
# Given CVE affects `pkg:pypi/pyyaml`, which workspaces are affected?
.components[]
| select(.purl == "pkg:pypi/pyyaml")
| .properties[]?
| select(.name == "mikebom:workspace-member")
| .value | fromjson
```

Returns the workspace list this CVE hits — direct answer to "which deployment artifact needs remediation."

### Recipe 4 — verify C120 ↔ C121 invariant

```jq
# Union of all per-component workspace-member values
[
  .components[]?.properties[]?
  | select(.name == "mikebom:workspace-member")
  | .value | fromjson | .[]
] | unique
as $union
| .metadata.properties[]?
| select(.name == "mikebom:workspaces-detected")
| .value | fromjson
| {union: $union, detected: ., match: (. == $union)}
```

`.match` MUST be `true` post-176.

## Non-goals

- No new CLI flags (FR-007).
- No structural SBOM changes (`components[]`, `dependencies[]`, `metadata.component` all unchanged — FR-008).
- No modification to `mikebom:source-files` — it remains as-is; C120 is a derivation from it, not a replacement.
- No per-workspace SBOM emission — that's m178 candidate scope.
- No nested composition via CDX `metadata.component.components[]` — that's m177 candidate scope.
