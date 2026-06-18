# Phase 1: Data Model — Smarter root component selection

## Entity: `is_workspace_root` field on main-module annotations

**Location**: `ResolvedComponent.extra_annotations` (existing `BTreeMap<String, serde_json::Value>` at `mikebom-common/src/resolution.rs:218`).

**Key**: `mikebom:is-workspace-root`

**Value**: `serde_json::Value::Bool(true)` when the component's defining manifest file's parent directory canonicalizes to the scan's `--path`; `Bool(false)` otherwise. Set by every per-ecosystem main-module emitter:

- `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs::build_main_module_entry` (Go)
- `mikebom-cli/src/scan_fs/package_db/cargo.rs::build_cargo_main_module_entry` (Cargo)
- `mikebom-cli/src/scan_fs/package_db/npm/walk.rs` (npm)
- `mikebom-cli/src/scan_fs/package_db/pip/` (pip)
- `mikebom-cli/src/scan_fs/package_db/gem.rs` (gem)
- `mikebom-cli/src/scan_fs/package_db/maven.rs` (maven)

**Visibility**: Internal-only. NOT emitted in serialized SBOM output (filtered out at the per-format serializer per the same convention used for other `mikebom:*` annotations that exist only to drive selection logic).

**Validation**: Always present on a `mikebom:component-role: "main-module"` component. A main-module without `is_workspace_root` is a contract violation — `root_selector.rs` MUST `assert!()` and fail loudly in debug builds; in release builds, treat as `false` (degrades gracefully to the next ladder branch).

**Why an annotation and not a typed field on `ResolvedComponent`**: The other `mikebom:*` annotations on `extra_annotations` already use this exact pattern (`mikebom:component-role`, `mikebom:produces-binaries`, `mikebom:sbom-tier`, etc.). Adding a typed field on `ResolvedComponent` would touch a shared workspace-crate boundary (mikebom-common) AND require a SerDe migration; the annotation channel is the right abstraction level.

## Entity: `RootSelectionHeuristic` enum

**Location**: `mikebom-cli/src/generate/root_selector.rs` (NEW module).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSelectionHeuristic {
    /// FR-002 — exactly one main-module has `is_workspace_root == true`.
    /// Confidence: 0.95
    RepoRoot,
    /// FR-003 — multiple main-modules have `is_workspace_root == true`;
    /// resolved by the fixed ecosystem-priority order.
    /// Confidence: 0.70
    EcosystemPriority,
    /// FR-004 — no main-module has `is_workspace_root == true`; the
    /// longest common path prefix matches exactly one main-module's
    /// manifest path.
    /// Confidence: 0.80
    LongestCommonPrefix,
    /// Fallback to the existing Maven JAR-walker `scan_target_coord`
    /// branch. Reached only after FR-002/003/004 all fail to pick.
    /// Confidence: 0.60
    MavenScanTargetCoord,
    /// Fallback to the existing `pkg:generic/<target>@0.0.0` placeholder.
    /// Reached only when no main-module exists AND no `scan_target_coord`.
    /// Confidence: 0.30
    SyntheticPlaceholder,
}

impl RootSelectionHeuristic {
    /// Stable string emitted in the annotation `heuristic` field.
    pub fn name(&self) -> &'static str { ... }
    /// Fixed confidence per heuristic. Returns a value in `[0.0, 1.0]`.
    pub fn confidence(&self) -> f64 { ... }
}
```

**Two implicit cases NOT in the enum** (per FR-006):

- `single-main-module` (existing count==1 fast path): no annotation emitted; conceptual confidence 1.0.
- `operator-override` (existing milestone-077 path): no annotation emitted; conceptual confidence 1.0. The milestone-077 override audit channel is the right place.

This keeps the enum tightly scoped to "cases where the new heuristic actually fired and the annotation is emitted."

## Entity: `mikebom:root-selection-heuristic` document-scope annotation

**Catalog row**: C-row TBD at PR review per R7; working assumption C69.

**Per-format projection**:

| Format | Surface |
|---|---|
| CycloneDX 1.6 | `metadata.properties[]` entry. `name: "mikebom:root-selection-heuristic"`, `value: <stringified JSON object>`. Same pattern as the existing `mikebom:supplement-cdx` document-scope annotation (milestone 119). |
| SPDX 2.3 | Document-level `annotations[]` entry. `annotationType: "OTHER"`, `comment: <stringified JSON object>`, `annotator: "Tool: mikebom-<version>"`. Same pattern as milestone 011's document-scope annotations. |
| SPDX 3.0.1 | Top-level `annotations[]` entry on the SBOM element. Same pattern as milestone 080's user-creator/annotator metadata. |

**JSON object schema**:

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "mikebom:root-selection-heuristic",
  "value": {
    "heuristic": "repo-root-main-module" | "ecosystem-priority" | "longest-common-prefix" | "maven-scan-target-coord" | "synthetic-placeholder",
    "confidence": 0.95 | 0.70 | 0.80 | 0.60 | 0.30
  }
}
```

**Emission gating**: the annotation is emitted ONLY when one of the five enum variants fired (i.e., the count==1 fast path did NOT fire AND no operator override was set). This is the byte-identity preservation lever for SC-003.

## Entity: Ecosystem priority order

**Location**: `mikebom-cli/src/generate/root_selector.rs`. Compile-time constant.

```rust
/// FR-003 — fixed ecosystem-priority order. Operators wanting a
/// different order must use --root-name/--root-purl-type.
const ECOSYSTEM_PRIORITY: &[&str] = &[
    "golang",
    "cargo",
    "maven",
    "npm",
    "pip",
    "gem",
    "generic",
];
```

**Resolution**: each main-module-tagged `ResolvedComponent` has a `purl` field of type `Purl`. `Purl::ecosystem()` already exists (used by milestone-005 for the parity catalog and by the binding module at `binding/source_inputs.rs:346`). We compare PURL ecosystem strings against this slice. A main-module whose ecosystem string is NOT in the slice (hypothetical future ecosystem) sorts AFTER `"generic"` deterministically by ecosystem string lexicographic order — ensures deterministic behavior in the unlikely "the catalog grew faster than this constant" case.

## Entity: `SourceDocumentBinding` envelope subject (FR-011 wiring)

**Existing struct**: `mikebom-cli/src/binding/source_inputs.rs::SourceDocumentBinding` (milestone 072).

**Change**: the binding's `subject` field MUST be populated from the same root-component selection as the metadata.component / documentDescribes / rootElement emitters use. Currently the binding is wired off the milestone-077 `RootComponentOverride` + falls back to the JAR-walker `scan_target_coord` — this is the bug surface for argo-workflows binding scripts.

**Implementation**: `cli/scan_cmd.rs` at the `--bind-to-source` wire-up point passes the `RootSelectionResult` (a small `{ subject: Purl, heuristic: Option<RootSelectionHeuristic> }` struct) from the new selector into the binding constructor. No new field added to `SourceDocumentBinding` itself — the binding only sees the `Purl`, exactly as today.

## Relationships

```text
ResolvedComponent
  └── extra_annotations: BTreeMap<String, serde_json::Value>
        ├── "mikebom:component-role" → "main-module"           (existing)
        ├── "mikebom:is-workspace-root" → bool                 (NEW, internal-only)
        └── ... (other existing annotations)

RootSelector (NEW, generate/root_selector.rs)
  ├── input: &[ResolvedComponent] + &RootComponentOverride + Option<ScanTargetCoord>
  ├── apply_ladder() → RootSelectionResult
  └── output: RootSelectionResult { subject: Purl|String|None,
                                    heuristic: Option<RootSelectionHeuristic>,
                                    losers: Vec<Purl> }   // for FR-007 warning

RootSelectionResult
  └── consumed by:
        ├── generate/cyclonedx/metadata.rs            (CDX metadata.component + heuristic annotation)
        ├── generate/spdx/document.rs                  (SPDX 2.3 documentDescribes + heuristic annotation)
        ├── generate/spdx/v3_document.rs               (SPDX 3 rootElement + heuristic annotation)
        ├── cli/scan_cmd.rs::bind_source_ctx           (FR-011 — --bind-to-source envelope subject)
        └── cli/scan_cmd.rs::scan_end_warnings         (FR-007 warning emission)
```

## State transitions

None. Selection is a pure function of `(components, root_override, scan_target_coord, scan_root_path)` evaluated once per scan after all readers complete and before any emitter runs.

## Validation rules summary

| Rule | Source | Validation |
|---|---|---|
| `mikebom:is-workspace-root` is always present on main-module components | Per-ecosystem readers | `assert!()` in debug; degrade-to-false in release |
| `RootSelectionHeuristic` confidence values are immutable | This module | `confidence()` returns a `match` over the enum; no I/O, no runtime config |
| Selection is byte-identical when `count == 1` AND no override | `root_selector::apply_ladder` | Direct unit test asserting no annotation emitted in the fast-path branch |
| Cross-format consistency (FR-005, SC-005) | All three emitters call into `root_selector::apply_ladder` with the same inputs | Integration test diffs the subject PURL across CDX/SPDX-2.3/SPDX-3 from a single scan |
| FR-007 warning text shape | `cli/scan_cmd.rs::scan_end_warnings` | Integration test captures stderr and matches the expected substring |
