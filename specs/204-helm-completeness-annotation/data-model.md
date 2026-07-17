# Data Model: `mikebom:image-extraction-completeness` Document-Scope Annotation

**Date**: 2026-07-17
**Purpose**: Document the 1 new inherent method on an existing enum + 1 new field on each of two existing structs + 4 new emitter branches + 1 new parity catalog row. No new types.

## E1: `HelmExtractionMode::as_wire_str` inherent method (NEW method on existing enum)

**Location**: `mikebom-cli/src/scan_fs/package_db/mod.rs` (impl block adjacent to the existing enum at line 427).

**Signature**: `pub fn as_wire_str(&self) -> &'static str`

**Body**:

```rust
impl HelmExtractionMode {
    /// Wire-format string for the `mikebom:image-extraction-completeness`
    /// document-scope annotation across CDX 1.6 / SPDX 2.3 / SPDX 3.
    /// Milestone 204 (#554). Determinism-critical — MUST match the
    /// values in the m071 parity catalog row C123 exactly.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            HelmExtractionMode::Unrendered => "partial",
            HelmExtractionMode::Rendered => "full",
        }
    }
}
```

**Side effect**: Drop the `#[allow(dead_code)]` on `HelmExtractionMode::Rendered` at line 435 — m204 makes it live in every emitter path.

**Validation rules**:
- Exactly two output strings: `"partial"` and `"full"`. No other values valid.
- Return type is `&'static str` so callers can pass directly into `json!()` without allocation.
- Unit-testable in isolation (one test per variant + one test per wire value).

## E2: `ScanResult.helm_extraction_mode` (NEW struct field)

**Location**: `mikebom-cli/src/scan_fs/mod.rs` (struct definition at line 76+; add field alongside the existing `go_workspace_mode` at line 131).

**Field**: `pub helm_extraction_mode: Option<crate::scan_fs::package_db::HelmExtractionMode>`

**Owner semantics**: `ScanResult` OWNS the value (not a borrow) so it can flow across the `scan_fs::scan_path` return boundary. The `scan_cmd.rs` destructure then borrows via `.as_ref()` at the `ScanArtifacts` construction site.

**Assignment site**: `mikebom-cli/src/scan_fs/mod.rs` around line 352, immediately after the `go_workspace_mode = scan_result.diagnostics.go_workspace_mode.clone();` line:

```rust
// Milestone 204 (#554): mirror helm-extraction-mode from
// ScanDiagnostics into the local for the ScanResult return.
// `Option<HelmExtractionMode>` where `HelmExtractionMode` is
// `Copy` — so `.clone()` is cheap and consistent with the
// pattern used for the sibling Go fields.
helm_extraction_mode = scan_result.diagnostics.helm_extraction_mode;
```

Also add a `let mut helm_extraction_mode: Option<...> = None;` binding at the top of `scan_path` around line 313 (alongside `go_workspace_mode`).

**Validation rules**:
- `Option::None` when no helm reader ran during the scan (byte-identity guarantee for non-Helm scans per FR-004).
- `Option::Some(HelmExtractionMode::Unrendered)` when the reader ran with default (US2 line-based) extraction.
- `Option::Some(HelmExtractionMode::Rendered)` when the reader ran with `--helm-render` and helm succeeded.
- `HelmExtractionMode` is `Copy` (per m188's `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` at line 426) — no `.clone()` semantic concern.

## E3: `ScanArtifacts.helm_extraction_mode` (NEW struct field)

**Location**: `mikebom-cli/src/generate/mod.rs` (struct definition at line 51+; add field alongside the existing `go_workspace_mode` at line 106).

**Field**: `pub helm_extraction_mode: Option<&'a crate::scan_fs::package_db::HelmExtractionMode>`

**Owner semantics**: borrows from the caller's `ScanResult.helm_extraction_mode: Option<HelmExtractionMode>`. Caller uses `.as_ref()` to convert `Option<T>` → `Option<&T>`.

**Doc-comment structure** (mirrors the m161 `go_workspace_mode` doc comment):

```rust
/// Milestone 204 (#554): document-scope Helm image-extraction-mode
/// signal driving the C123 `mikebom:image-extraction-completeness`
/// annotation. `None` when no helm reader ran during the scan
/// (byte-identity per FR-004). `Some(Unrendered)` → wire value
/// `"partial"`. `Some(Rendered)` → wire value `"full"`.
pub helm_extraction_mode: Option<&'a crate::scan_fs::package_db::HelmExtractionMode>,
```

**Validation rules**:
- Every field in `ScanArtifacts` today follows the `Option<&…>` borrow pattern for Go signals; m204 matches.
- No default value needed at the struct level — every callsite explicitly sets the field.

## E4: `build_metadata` new argument (MODIFIED function signature)

**Location**: `mikebom-cli/src/generate/cyclonedx/metadata.rs:45` (`pub fn build_metadata`).

**New argument**: `helm_extraction_mode: Option<&crate::scan_fs::package_db::HelmExtractionMode>` — added at the end of the argument list (after `go_cache_warming` at line 105).

**Emit branch** (new): immediately after the `go_workspace_mode` C112 branch at line 569, add:

```rust
// Milestone 204 (#554): doc-scope helm image-extraction completeness
// annotation. C123. Emitted iff `helm_extraction_mode` is `Some(_)`
// (helm reader ran). Wire values `"partial"` for Unrendered
// (default US2 line-based extraction) and `"full"` for Rendered
// (m203 `--helm-render` success). Byte-identity preserved for
// non-Helm scans per FR-004 (annotation absent when `None`).
if let Some(mode) = helm_extraction_mode {
    properties.push(json!({
        "name": "mikebom:image-extraction-completeness",
        "value": mode.as_wire_str(),
    }));
}
```

**Callsite updates**: ~7 test callsites in the same file that pass every argument literally to `build_metadata`. Each gets one more `, None` at the end (or `Some(&HelmExtractionMode::Unrendered)` for tests specifically exercising the branch). Verified test callsites: lines 1198, 1219, 1228, 1240, 1251, 1260, 1296.

**Production callsite**: `mikebom-cli/src/generate/cyclonedx/builder.rs` (verified via `grep build_metadata builder.rs`) — one call, thread through `scan_artifacts.helm_extraction_mode`.

## E5: SPDX 2.3 `annotate_document` emit branch (MODIFIED)

**Location**: `mikebom-cli/src/generate/spdx/annotations.rs::annotate_document` at line 395+ (the C112 emit branch is at line 630-639).

**New branch** (immediately after the C112 block, before the C100 collisions block at line 648):

```rust
// Milestone 204 (#554): C123 doc-scope helm image-extraction
// completeness annotation. Emitted iff helm reader ran. Wire
// value derived from HelmExtractionMode::as_wire_str().
if let Some(mode) = artifacts.helm_extraction_mode {
    push(&mut out, "mikebom:image-extraction-completeness", json!(mode.as_wire_str()));
}
```

**Function signature**: unchanged — `annotate_document` takes `&ScanArtifacts<'_>`, and the new field is already accessible via `artifacts.helm_extraction_mode` after E3 lands.

## E6: SPDX 3 emit branch in `v3_annotations.rs` (MODIFIED)

**Location**: `mikebom-cli/src/generate/spdx/v3_annotations.rs` (analog to E5; C112 emit at line 599-601).

**New branch** (immediately after the C112 block):

```rust
// Milestone 204 (#554): C123 doc-scope helm image-extraction
// completeness annotation (SPDX 3). Same emission semantics as
// the CDX + SPDX 2.3 emitters (E4, E5).
if let Some(mode) = artifacts.helm_extraction_mode {
    push(out, "mikebom:image-extraction-completeness", json!(mode.as_wire_str()));
}
```

**Function signature**: unchanged.

## E7: Parity catalog row C123 (NEW entry)

**Locations**: 4 files —

1. **`mikebom-cli/src/parity/extractors/cdx.rs`** (after the C112 `cdx_anno!` at line 814):
   ```rust
   cdx_anno!(c123_cdx, "mikebom:image-extraction-completeness", document);
   ```
2. **`mikebom-cli/src/parity/extractors/spdx2.rs`** (after C112 line 583):
   ```rust
   spdx23_anno!(c123_spdx23, "mikebom:image-extraction-completeness", document);
   ```
3. **`mikebom-cli/src/parity/extractors/spdx3.rs`** (after C112 line 643):
   ```rust
   spdx3_anno!(c123_spdx3, "mikebom:image-extraction-completeness", document);
   ```
4. **`mikebom-cli/src/parity/extractors/mod.rs`** (after C112 line 472):
   ```rust
   ParityExtractor {
       row_id: "C123",
       label: "mikebom:image-extraction-completeness",
       cdx: c123_cdx,
       spdx23: c123_spdx23,
       spdx3: c123_spdx3,
       directional: Directionality::SymmetricEqual,
       order_sensitive: false,
   },
   ```

**Validation rules**:
- `Directionality::SymmetricEqual` requires exact byte-equal value across all three formats — matches E1's `&'static str` return, which guarantees equality.
- `order_sensitive: false` because it's a single scalar (no array positions to compare).
- Row ID `C123` verified as next free via `grep -oE 'row_id: "C1[0-9]+"'` — C122 is highest used.

## E8: `scan_cmd.rs` destructure + thread (MODIFIED)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs`.

**Destructure update** at line 2507-2517 (add `helm_extraction_mode,` to the destructure list alongside `go_workspace_mode,`):

```rust
let scan_fs::ScanResult {
    mut components,
    mut relationships,
    complete_ecosystems,
    os_release_missing_fields,
    go_transitive_coverage,
    go_transitive_fallback_count,
    go_cache_warming,
    go_workspace_mode,
    helm_extraction_mode,           // NEW
    scan_target_coord,
    divergence_records,
} = scan_fs::scan_path(...);
```

**ScanArtifacts thread** at line 3187 (add alongside `go_workspace_mode`):

```rust
go_workspace_mode: go_workspace_mode.as_ref(),
// Milestone 204 (#554): doc-scope helm image-extraction-mode
// signal for the C123 annotation.
helm_extraction_mode: helm_extraction_mode.as_ref(),
```

Also update the `attestation`-flow `ScanArtifacts` construction sites in `scan_cmd.rs` (search for other `ScanArtifacts { ... }` literals in the file — expected 1-2 additional sites) with `helm_extraction_mode: None,` (attestation-generated SBOMs don't invoke helm reader; explicit None preserves byte-identity).

## Cross-cutting: FR-004 non-Helm-scan byte-identity guarantee

**Guarantee**: The four emit branches (E4-E6) all pattern-match `if let Some(mode) = <artifacts.>helm_extraction_mode`. When the helm reader didn't run during the scan, `ScanDiagnostics.helm_extraction_mode` stays `None`, flows through as `None` to `ScanResult` (E2), to `ScanArtifacts` (E3), to each emitter. Each emit branch short-circuits — no bytes emitted.

**Enforcement**: Post-implementation `git diff --stat mikebom-cli/tests/fixtures/` MUST show drift ONLY on helm-chart-scanning goldens. Non-Helm fixtures are byte-identical.

**Test coverage** (US3): `m204_us3_annotation_absent_on_non_helm_scan` scans a directory with only `readme.txt` (no `Chart.yaml`) and asserts the string `mikebom:image-extraction-completeness` does not appear in any of the three emitted JSON outputs.
