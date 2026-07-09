# Phase 1 Data Model: Workspace-member visibility (m176)

**Feature**: 176-workspace-visibility
**Date**: 2026-07-08

Three entities. Zero new types on `mikebom-common` (per Constitution VI — this is a `mikebom-cli`-only milestone). Two new wire annotations; one new helper module.

## Entity 1 — `derive_workspace_root` helper function

**Location**: `mikebom-cli/src/scan_fs/workspace_root.rs` (new file, module-scope).

**Shape**:

```rust
/// Derive a workspace root path from a `source_file_paths` entry
/// (per `ResolutionEvidence.source_file_paths`). Returns `None` when
/// the input is malformed or unattributable — the caller then omits
/// the `mikebom:workspace-member` annotation per FR-002 / Q1.
///
/// Handles two shapes per research §R2:
/// 1. **Relative root-relative path** (`"official/requirements.txt"`,
///    `"src/frontend/package.json"`, `"Cargo.toml"`): returns
///    `Some(parent.to_string())`. If parent is empty (root-level
///    manifest), returns `Some(".".to_string())`.
/// 2. **`path+file://<absolute>` URI** (pip main-modules per
///    `pip/mod.rs:521`): strips prefix, then attempts to strip the
///    scan_root_abs to get root-relative form. If path is not under
///    scan_root_abs (malformed evidence), returns `None`.
///
/// Forward-slash normalization is applied on Windows per FR-010.
///
/// Pure function — no I/O, no allocation beyond the returned String.
pub(crate) fn derive_workspace_root(
    source_file_path: &str,
    scan_root_abs: &std::path::Path,
) -> Option<String>;
```

**Contract**:
- **Pure function** — no I/O, deterministic on the two inputs.
- **Idempotent** — repeated calls with the same inputs return the same value.
- **UTF-8 in, UTF-8 out** — the input is already `&str`; output is `String` guaranteed valid UTF-8.
- **Forward-slash separator only** — output on Windows has any backslashes normalized (per FR-010).
- **`None` never becomes the empty string** — the caller MUST distinguish "no annotation" (`None`) from "workspace is scan-root" (`Some(".".to_string())`).

**Test cases** (unit tests inline in the same module):

1. `derive_root_level_manifest_returns_dot` — input `"Cargo.toml"`, expect `Some(".".to_string())`.
2. `derive_subdir_manifest_returns_dir_path` — input `"src/frontend/package.json"`, expect `Some("src/frontend".to_string())`.
3. `derive_pip_uri_main_module_returns_relative` — input `"path+file:///abs/to/scan-root/src/lfx"`, scan_root_abs `/abs/to/scan-root`, expect `Some("src/lfx".to_string())`.
4. `derive_pip_uri_outside_scan_root_returns_none` — input `"path+file:///unrelated/path"`, scan_root_abs `/abs/to/scan-root`, expect `None`.
5. `derive_empty_string_returns_none` — input `""`, expect `None`.
6. `derive_backslash_windows_normalized` — input `"src\\frontend\\package.json"` (Windows shape), expect `Some("src/frontend".to_string())`.

## Entity 2 — CDX 1.6 wire shapes

**Location**: emitted CDX SBOM, `.components[].properties[]` (per-component) and `.metadata.properties[]` (doc-scope).

### C120 — `mikebom:workspace-member` (per-component)

**Shape**:

```json
{
  "name": "mikebom:workspace-member",
  "value": "[\"src/frontend\",\"src/lfx\"]"
}
```

Value is `serde_json::to_string(&sorted_dedup_vec)`. Encoding matches m134 `mikebom:purl-collisions-detected` / m147 `mikebom:peer-edge-targets` / m173 `mikebom:go-cache-warming-failed` — JSON-encoded array inside a string.

**Emitted iff**: at least one entry in `evidence.source_file_paths` yielded `Some(...)` from `derive_workspace_root`. File-tier components and any component with empty derived-set → annotation ABSENT per FR-002 / Q1.

**Value regex**: `^\[(\"[^\"]+\"(,\"[^\"]+\")*)?\]$` — bracketed JSON array of strings.

### C121 — `mikebom:workspaces-detected` (doc-scope)

**Shape**:

```json
{
  "name": "mikebom:workspaces-detected",
  "value": "[\".\",\"src/frontend\",\"src/lfx\"]"
}
```

Same encoding: alphabetically-sorted deduplicated JSON array in a string.

**Emitted iff**: the union of all per-component derived workspace paths is non-empty. When zero workspaces detected (bare filesystem, no manifests), the annotation is ABSENT per FR-003.

## Entity 3 — SPDX 2.3 wire shapes

**Location**: document-scope `annotations[]` (C121) + per-Package `annotations[]` (C120), using the standing `MikebomAnnotationCommentV1` envelope from m080.

### C120 envelope

```json
{
  "annotationType": "OTHER",
  "annotator": "Tool: mikebom-0.1.0-alpha.NN",
  "annotationDate": "1970-01-01T00:00:00Z",
  "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:workspace-member\",\"value\":\"[\\\"src/frontend\\\",\\\"src/lfx\\\"]\"}"
}
```

### C121 envelope

Same shape but at document scope with `field = "mikebom:workspaces-detected"`.

## Entity 4 — SPDX 3.0.1 wire shapes

**Location**: `@graph[]` typed `Annotation` elements — one per emission site.

### C120 shape

```json
{
  "type": "Annotation",
  "spdxId": "urn:mikebom:annotation:<hash>",
  "subject": "<Package-IRI>",
  "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:workspace-member\",\"value\":\"[\\\"src/frontend\\\"]\"}",
  "annotationType": "other",
  "creationInfo": "_:creationInfo0"
}
```

### C121 shape

Same but `subject` is the `SpdxDocument` root IRI + `field = "mikebom:workspaces-detected"`.

## Entity 5 — Parity catalog rows

**Location**: `mikebom-cli/src/parity/extractors/mod.rs` EXTRACTORS table + per-format extractor helpers.

### mod.rs entries

```rust
// Milestone 176: C120 per-component + C121 document-scope workspace
// visibility. Both SymmetricEqual per FR-011.
ParityExtractor { row_id: "C120", label: "mikebom:workspace-member",         cdx: c120_cdx, spdx23: c120_spdx23, spdx3: c120_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
ParityExtractor { row_id: "C121", label: "mikebom:workspaces-detected",      cdx: c121_cdx, spdx23: c121_spdx23, spdx3: c121_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

Insert alphabetically after C119 `mikebom:go-cache-warming-failed`.

### Per-format helpers

```rust
// cdx.rs — insert after c119_cdx
cdx_anno!(c120_cdx, "mikebom:workspace-member",         component);
cdx_anno!(c121_cdx, "mikebom:workspaces-detected",      document);

// spdx2.rs — insert after c119_spdx23
spdx23_anno!(c120_spdx23, "mikebom:workspace-member",   component);
spdx23_anno!(c121_spdx23, "mikebom:workspaces-detected", document);

// spdx3.rs — insert after c119_spdx3
spdx3_anno!(c120_spdx3, "mikebom:workspace-member",     component);
spdx3_anno!(c121_spdx3, "mikebom:workspaces-detected",  document);
```

## Entity 6 — Advisory-log context

**Location**: `mikebom-cli/src/cli/scan_cmd.rs` — at the same emission-tail site m173 FR-004 uses.

```rust
// Milestone 176 FR-004: monorepo-shape advisory. Fires exactly once
// when >1 workspace root detected AND the scan produced ≥1 component.
// No offline gating (per spec Assumptions — remediation is
// consumer-side jq slicing which needs no network).
if workspaces_detected.len() > 1 && !components.is_empty() {
    let workspace_list = workspaces_detected.iter().cloned().collect::<Vec<_>>().join(", ");
    tracing::info!(
        "monorepo shape detected: {} workspaces ({}). Downstream consumers can \
         filter per-workspace via `mikebom:workspace-member`; see \
         docs/reference/monorepos.md for jq recipes.",
        workspaces_detected.len(),
        workspace_list,
    );
}
```

The stable grep substring is `"monorepo shape detected: "` — CI dashboards can `grep -F` this to detect monorepo scans.

## Cross-entity invariants (post-176)

1. **`mikebom:workspaces-detected` value == union of all `mikebom:workspace-member` values**: structurally guaranteed by construction (both computed from the same `derive_workspace_root` output stream). Verified by SC-007 jq cross-check invariant in the integration test.
2. **File-tier components have neither annotation**: FR-002 / Q1 — file-tier's `evidence.source_file_paths` doesn't survive the derive filter (paths like `dev.start.sh` have no parent manifest); absent annotation is the wire signal.
3. **Non-monorepo scans (N=1) get a 1-element array**: consumer jq filters work uniformly across N=1 and N>1 cases (SC-008 byte-identity gate).
4. **No new SBOM wire fields beyond the two annotations**: FR-008 explicit — components[], dependencies[], metadata.component all unchanged.

## State transitions

None. Pure emission-time derivation over per-component evidence. No lifecycle events.
