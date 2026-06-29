# Contract — `apply_main_module_drop_or_demote` pass

Phase 1 output. Defines the pure-function contract for the new shared root-selector helper.

## Function signature

```rust
/// Milestone 149: consolidate the duplicated main-module-drop logic
/// from three emitter sites (cyclonedx/builder.rs:325-347,
/// spdx/document.rs:262-282, spdx/v3_document.rs:57-75) into a single
/// shared helper, AND add the new preserve-as-library-demote branch
/// gated on `preserve_main_module`.
///
/// The helper handles three behaviors:
///
/// 1. **Override INACTIVE** — passthrough; returns a clone of `components`
///    with empty `redirected_main_module_purls`. Byte-identical to pre-149
///    for scans without override flags (SC-003 regression guard).
///
/// 2. **Override ACTIVE + preserve OFF** — milestone-077 clean-replacement.
///    Filters out main-module entries from the returned Vec; populates
///    `redirected_main_module_purls` with their PURLs for downstream
///    relationship re-anchoring (milestone-084 logic at
///    `cyclonedx/builder.rs:442-447` and parallel SPDX sites). Byte-
///    identical to pre-149 default behavior (SC-002 regression guard).
///
/// 3. **Override ACTIVE + preserve ON** — milestone 149 NEW. For each
///    main-module entry: KEEP in the returned Vec after applying the
///    demote transformation (remove `mikebom:component-role: main-module`
///    annotation, add `mikebom:demoted-from-main-module: "true"`
///    annotation); ADD PURL to `redirected_main_module_purls` so
///    relationship re-anchoring still fires per US1 clarification
///    Option A (recorded 2026-06-29).
///
/// Pure function over its inputs. No side effects, no allocations beyond
/// the returned Vec + the redirected PURLs Vec.
pub(crate) fn apply_main_module_drop_or_demote(
    components: &[ResolvedComponent],
    root_override: &RootComponentOverride,
    preserve_main_module: bool,
) -> DropOrDemoteResult;

pub(crate) struct DropOrDemoteResult {
    pub effective_components: Vec<ResolvedComponent>,
    pub redirected_main_module_purls: Vec<String>,
}
```

## Contract requirements

| Requirement | Source spec | Test |
|---|---|---|
| Override inactive: passthrough | SC-003 + FR-007 | `apply_drop_or_demote_no_override_is_passthrough_md149` unit test |
| Override active + preserve OFF: drop main-modules | FR-007 + SC-002 (regression guard) | `apply_drop_or_demote_override_no_preserve_drops_main_module_md149` unit test |
| Override active + preserve OFF: populate redirected PURLs | (preserves milestone-084 re-anchoring) | Same test asserts `redirected_main_module_purls.contains(&main_module_purl)` |
| Override active + preserve ON: keep main-module with demote annotation | FR-001 + FR-004 | `apply_drop_or_demote_override_with_preserve_demotes_main_module_md149` unit test |
| Override active + preserve ON: STILL populate redirected PURLs (US1 Option A) | US1 clarification 2026-06-29 + Assumption 5 | Same test asserts `redirected_main_module_purls.contains(&main_module_purl)` |
| Demote transformation removes `mikebom:component-role: main-module` | FR-003 (type flips to library) + research §B | Unit test asserts `demoted.extra_annotations.get("mikebom:component-role").is_none()` |
| Demote transformation adds `mikebom:demoted-from-main-module: "true"` | FR-004 | Unit test asserts `demoted.extra_annotations.get("mikebom:demoted-from-main-module") == Some(&Value::String("true"))` |
| Demote transformation preserves every other field | FR-005 | Unit test asserts every named field (purl, name, version, parent_purl, evidence.*, hashes, lifecycle_scope, licenses, etc.) unchanged after the transform |
| Multi-main-module + override + preserve: no-op | FR-013, Edge Case 4 | `apply_drop_or_demote_multi_main_module_with_preserve_is_noop_md149` unit test |
| Cross-ecosystem coverage (Cargo/npm/Go) | FR-012 + SC-001 | `mikebom-cli/tests/demote_manifest_mainmod_md149.rs` integration test |

## Algorithm sketch (illustrative; not normative)

```rust
pub(crate) fn apply_main_module_drop_or_demote(
    components: &[ResolvedComponent],
    root_override: &RootComponentOverride,
    preserve_main_module: bool,
) -> DropOrDemoteResult {
    let override_active = root_override.is_active();

    // Path 1: override INACTIVE → passthrough.
    if !override_active {
        return DropOrDemoteResult {
            effective_components: components.to_vec(),
            redirected_main_module_purls: Vec::new(),
        };
    }

    // Multi-main-module guard (Edge Case 4): when N>1 main-modules are
    // tagged, NONE were promoted to metadata.component pre-149 (milestone
    // 127's placeholder-path behavior); the preserve flag is a no-op
    // because there's no single main-module to demote. Emit INFO log and
    // fall through to the drop-all path so the override clean-replacement
    // semantic stays unchanged.
    let main_module_count = components
        .iter()
        .filter(|c| is_main_module(c))
        .count();
    let effective_preserve = preserve_main_module && main_module_count == 1;
    if preserve_main_module && main_module_count > 1 {
        tracing::info!(
            count = main_module_count,
            "--preserve-manifest-main-module skipped: multi-main-module scan ({} modules detected)",
            main_module_count
        );
    }

    // Single-pass walk: collect redirected PURLs + build effective Vec.
    let mut effective = Vec::with_capacity(components.len());
    let mut redirected = Vec::new();
    for c in components {
        if is_main_module(c) {
            redirected.push(c.purl.as_str().to_string());
            if effective_preserve {
                // Path 3: demote in place — keep entry, transform annotations.
                let mut demoted = c.clone();
                demoted.extra_annotations.remove("mikebom:component-role");
                demoted.extra_annotations.insert(
                    "mikebom:demoted-from-main-module".to_string(),
                    serde_json::Value::String("true".to_string()),
                );
                effective.push(demoted);
            }
            // Path 2: drop (don't push). The PURL still went into `redirected`
            // above so relationship re-anchoring fires.
        } else {
            effective.push(c.clone());
        }
    }

    DropOrDemoteResult {
        effective_components: effective,
        redirected_main_module_purls: redirected,
    }
}

#[inline]
fn is_main_module(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get("mikebom:component-role")
        .and_then(|v| v.as_str())
        == Some("main-module")
}
```

## Negative-space contract (what MUST NOT happen)

- The pass MUST NOT add new components beyond the existing input set (no synthesized entries).
- The pass MUST NOT mutate any field of `ResolvedComponent` other than `extra_annotations["mikebom:component-role"]` (removed) and `extra_annotations["mikebom:demoted-from-main-module"]` (added) when the demote branch fires.
- The pass MUST NOT touch the input `components` slice — it's `&[ResolvedComponent]`, immutable. The output Vec is a clone.
- The pass MUST NOT call the demote branch when override is INACTIVE — preserve_main_module is a NO-OP without an active override (FR-006, Edge Case 1).
- The pass MUST NOT skip the redirected-PURLs population on the demote branch — relationship re-anchoring depends on it per US1 clarification Option A.
- The pass MUST NOT fire the demote branch in multi-main-module scans — Edge Case 4 / FR-013 requires silent no-op with INFO log.

## Call-site contract

The function MUST be called from three emitter sites, replacing the existing duplicated drop loops:

### CDX: `mikebom-cli/src/generate/cyclonedx/builder.rs:325-347`

```rust
// Pre-149: ~25 LOC of manual filter loop + dropped_main_module_purls accumulation
// Post-149:
let result = crate::generate::root_selector::apply_main_module_drop_or_demote(
    components,
    &artifacts.root_override,
    artifacts.preserve_manifest_main_module,
);
let filtered_components_owned: Option<Vec<ResolvedComponent>> = Some(result.effective_components);
let dropped_main_module_purls = result.redirected_main_module_purls;
```

### SPDX 2.3: `mikebom-cli/src/generate/spdx/document.rs:262-282`

Parallel replacement using the same helper call.

### SPDX 3: `mikebom-cli/src/generate/spdx/v3_document.rs:57-75`

Parallel replacement using the same helper call.

## Test surface

| Test | Location | Asserts |
|---|---|---|
| `apply_drop_or_demote_no_override_is_passthrough_md149` | `root_selector.rs#mod tests` | Path 1 (passthrough); FR-007 + SC-003 regression guard |
| `apply_drop_or_demote_override_no_preserve_drops_main_module_md149` | `root_selector.rs#mod tests` | Path 2 (drop); FR-007 + SC-002 regression guard |
| `apply_drop_or_demote_override_with_preserve_demotes_main_module_md149` | `root_selector.rs#mod tests` | Path 3 (demote); FR-001 + FR-004 + US1 Option A |
| `apply_drop_or_demote_demote_preserves_other_fields_md149` | `root_selector.rs#mod tests` | FR-005 — every named field except annotations preserved |
| `apply_drop_or_demote_multi_main_module_with_preserve_is_noop_md149` | `root_selector.rs#mod tests` | Edge Case 4 + FR-013 |
| `same_purl_collision_on_demote_dedups_via_existing_pipeline_md149` | `root_selector.rs#mod tests` | Edge Case 5 + FR-009 |
| `demote_cargo_main_module_emits_byte_identical_annotation_across_formats_md149` | `mikebom-cli/tests/demote_manifest_mainmod_md149.rs` | SC-004 cross-format invariance — Cargo fixture |
| `demote_npm_main_module_emits_byte_identical_annotation_across_formats_md149` | same file | SC-004 — npm fixture |
| `demote_go_main_module_emits_byte_identical_annotation_across_formats_md149` | same file | SC-004 — Go fixture |
| Existing C102 parity-catalog tests | `cross_format_byte_identity.rs`, `holistic_parity.rs` | SC-005 — Directionality::SymmetricEqual invariance once goldens refresh |
