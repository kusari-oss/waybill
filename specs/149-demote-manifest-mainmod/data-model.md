# Data Model — milestone 149

Phase 1 output. No new types introduced; this document describes the existing `ResolvedComponent` shape + the new annotation contract + the pre/post behavior table for affected components.

## Modified field on existing entity

### `mikebom_common::resolution::ResolvedComponent.extra_annotations`

| Aspect | Value |
|---|---|
| Type | `BTreeMap<String, serde_json::Value>` (unchanged) |
| **Removed by demote pass** | `extra_annotations["mikebom:component-role"]` (value was `"main-module"`) — removed in place for the manifest-derived main-module entry when the preserve branch fires. After removal, downstream type-derivation produces `type: "library"` automatically. |
| **Added by demote pass** | `extra_annotations["mikebom:demoted-from-main-module"] = Value::String("true")` — the new parity-bridging transparency annotation. |
| Other fields | Untouched. Every other key in the bag is preserved verbatim (license, hashes, supplier, evidence.*, lifecycle_scope, parent_purl, etc.). |
| Idempotence | The demote transformation IS idempotent — re-running it produces the same end-state (no main-module role tag, demote annotation present). |

## New annotation contract

### `mikebom:demoted-from-main-module`

| Field | Value |
|---|---|
| Key | `"mikebom:demoted-from-main-module"` |
| Value type | `serde_json::Value::String("true")` (boolean-as-string, matching milestone-127's `mikebom:is-workspace-root` precedent) |
| Emission gate | ONLY when the new `--preserve-manifest-main-module` CLI flag is set AND `RootComponentOverride::is_active()` returns true. Per FR-006 + FR-007, omitted in all other cases (including when the flag is set but no override is active — silent no-op with INFO log per Edge Case 1). |
| Wire surface | CDX 1.6 `components[].properties[]` (string-typed value via existing milestone-127 `is_internal_emission_key` filter → property iteration); SPDX 2.3 envelope `Annotation` on the demoted Package; SPDX 3 envelope `Annotation` on the demoted `software_Package` element. Same shape as existing parity-bridging annotations (milestone-127 `mikebom:is-workspace-root`, milestone-133 `mikebom:component-tier`). |
| Cross-format invariance | All three formats carry byte-equivalent boolean-string value `"true"` for the same component (Principle V parity-bridging; covered by new C102 parity-catalog row addition per research §D). |

## New public function

### `apply_main_module_drop_or_demote(...)` at `mikebom-cli/src/generate/root_selector.rs`

| Aspect | Value |
|---|---|
| Visibility | `pub(crate)` (called from `generate/cyclonedx/builder.rs`, `generate/spdx/document.rs`, `generate/spdx/v3_document.rs`) |
| Signature | `pub(crate) fn apply_main_module_drop_or_demote(components: &[ResolvedComponent], root_override: &RootComponentOverride, preserve_main_module: bool) -> DropOrDemoteResult` |
| Return type | `DropOrDemoteResult { effective_components: Vec<ResolvedComponent>, redirected_main_module_purls: Vec<String> }` |
| Behavior — override INACTIVE | Returns `{ effective_components: components.to_vec(), redirected_main_module_purls: vec![] }` — passthrough, no transformation. Preserves byte-identity for non-override scans (SC-003). |
| Behavior — override ACTIVE + preserve OFF | Filters out main-module entries (the milestone-077 clean-replacement path); populates `redirected_main_module_purls` for relationship re-anchoring (the milestone-084 path). Byte-identical to pre-149 (SC-002). |
| Behavior — override ACTIVE + preserve ON | For each main-module entry: KEEP in `effective_components` after applying the demote transformation (remove role tag, add demote annotation); ADD PURL to `redirected_main_module_purls` so relationship re-anchoring still fires per US1 clarification Option A. |
| Side effects | None — pure function over its inputs. |
| Allocations | One `Vec<ResolvedComponent>` (clone of inputs with demote-transformed entries) + one `Vec<String>` (the redirected PURLs). Same allocation profile as the existing per-emitter drop loops. |
| Complexity | O(N) where N = component count. Same cost shape as the existing drop loops. |

### `DropOrDemoteResult` struct

```text
pub(crate) struct DropOrDemoteResult {
    /// The post-filter Vec the emitter should iterate. When override is
    /// inactive, this is a clone of the input. When override is active
    /// + preserve OFF, this is the input minus all main-module entries.
    /// When override is active + preserve ON, this is the input with
    /// main-module entries demoted in place (role tag removed, demote
    /// annotation added) but still present.
    pub effective_components: Vec<ResolvedComponent>,

    /// PURLs of main-module entries that need their outbound relationships
    /// re-anchored on the operator-override root (milestone 084's
    /// relationship-re-anchoring logic). Populated when override is
    /// active regardless of preserve setting — per US1 clarification
    /// Option A, the demoted entry has NO outbound dependsOn edges in
    /// the wire output even when kept in components[]. Empty when
    /// override is inactive.
    pub redirected_main_module_purls: Vec<String>,
}
```

## Pre/post behavior table

| Override + preserve state | Manifest-main-module's wire fate | demoted-from-main-module annotation | dependsOn edges on demoted entry | dependsOn edges on operator-override root |
|---|---|---|---|---|
| No override, no preserve (default) | Stays at `metadata.component` as the project's main-module (`type: "application"` per milestone 053/064–070) | absent (no operator override fired) | n/a | n/a |
| `--root-name` set, preserve OFF (milestone 077 default) | DROPPED entirely — no entry in `components[]` (current clean-replacement behavior) | absent (no demoted entry to annotate) | n/a | YES — re-anchored from the dropped main-module's PURL per milestone 084 |
| `--root-name` set, preserve ON (milestone 149 NEW) | KEPT in `components[]` as a `library`-typed entry with original PURL + name + version + license + hashes | PRESENT — `"mikebom:demoted-from-main-module": "true"` | NONE — outbound edges re-anchored to operator-override root per US1 clarification Option A | YES — same re-anchoring as the drop case |
| `--preserve-manifest-main-module` set, NO override | Stays at `metadata.component` as the project's main-module — flag is a silent no-op with INFO log (Edge Case 1) | absent | n/a | n/a |
| Multi-main-module + override + preserve (Edge Case 4) | Flag is a silent no-op with INFO log — no single main-module to demote; all workspace members continue per milestone-127 multi-main-module behavior | absent | n/a | n/a |

## Validation rules (consolidated from spec FRs)

| Input | Rule | Source |
|---|---|---|
| `--preserve-manifest-main-module` set alone (no override) | INFO-level log + no-op (no annotation emitted, no behavior change) | FR-006, Edge Case 1 |
| `--preserve-manifest-main-module` NOT set | Byte-identical to milestone 077 (no annotation, drop behavior intact) | FR-007, SC-002 |
| No override flags at all | Byte-identical to pre-149 (manifest-main-module IS the root, no annotation, no change) | SC-003 |
| Override active + preserve set + single main-module | Demote the entry: remove role tag, add annotation, keep in `components[]` | FR-001 + FR-002 + FR-003 + FR-004 + FR-005 |
| Override active + preserve set + multi-main-module | INFO-level log + no-op (no demote, no annotation) | FR-013, Edge Case 4 |
| Override active + preserve set + demoted PURL collides with existing dep entry | Dedupe via existing `(ecosystem, name, version, parent_purl)` group-key; surviving entry carries the demote annotation | FR-009, Edge Case 5 |
| Demoted entry's other fields | Unchanged — every field except `extra_annotations` content preserved verbatim | FR-005 |
| Demoted entry's outbound relationships | Re-anchored on operator-override root via existing milestone-084 logic; demoted entry has empty `dependsOn` | US1 clarification Option A, Assumption 5 |
| Annotation value | Always `Value::String("true")` (boolean-as-string per milestone-127 `mikebom:is-workspace-root` precedent) | FR-004 |
| Cross-format byte-equivalence | C102 parity-catalog row asserts `Directionality::SymmetricEqual` across CDX / SPDX 2.3 / SPDX 3 | FR-010, SC-005 |

## Out of model

- **No new types** beyond the `DropOrDemoteResult` struct that consolidates the existing per-emitter return shape.
- **No public API surface changes** outside the new `apply_main_module_drop_or_demote` function (purely additive).
- **No changes to `ResolvedComponent` struct itself**: the demote transformation operates on the existing `extra_annotations` field.
- **No changes to the deduplicator** (`mikebom-cli/src/resolve/deduplicator.rs`): the demote pass runs at emit time, downstream of dedup.
- **No changes to any ecosystem reader**: the main-module role tag is added by milestone-127's `tag_main_modules_with_workspace_root` regardless of reader; the demote pass operates on the normalized post-tagging Vec.
- **No changes to the `RootComponentOverride` struct** (existing): the override semantic is unchanged; only the helper that consumes it adds the new branch.
