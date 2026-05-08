# Data Model — milestone 084 CDX 1.6 main-module super-root collapse

The milestone introduces ZERO new production Rust types. The "data model" here is the conceptual identifier-flow change inside `cyclonedx/builder.rs` and the closure-invariant set used by the new regression test.

## Existing types touched (no signature changes)

### `String` — `target_ref` (in `cyclonedx/builder.rs`)

Current: `let target_ref = format!("{}@{}", effective_target_name, effective_target_version);` — always the legacy short-form.

Post-fix: same `String` type; computed conditionally per research §1:

```text
target_ref =
    main-module PURL                                if  main_module_present && !override_active
    "{effective_target_name}@{effective_target_version}"   otherwise
```

The runtime type is unchanged. Only the value-derivation logic differs.

### `&str` — `main_module_purl` (new local binding)

A `&str` borrowed from `effective_components[i].purl.as_str()` of the component whose `extra_annotations["mikebom:component-role"] == "main-module"`. Lifetime tied to `effective_components`; consumed immediately to build `target_ref` (`String` allocated; the `&str` does not outlive the local block).

No new production newtypes (Constitution Principle IV — `target_ref` remains a `String` at the same boundary it crosses today; the milestone explicitly defers a `BomRef` newtype refactor per spec Out of Scope §3).

## Existing types referenced (no changes)

- `ResolvedComponent` (existing) — has `.purl: Purl` and `.extra_annotations: serde_json::Map<String, serde_json::Value>`. Read by both the existing override-filter (`builder.rs:272-293`) and the new main-module-detection at the same site.
- `Relationship { from: String, to: String, relationship_type: RelationshipType }` (existing, defined in `mikebom-cli/src/scan_fs/mod.rs`) — populates `dep_map` in `dependencies.rs`. Unchanged by this milestone.
- `RootOverride` (existing, milestone 077) — `is_active() -> bool`, `name: Option<String>`, `version: Option<String>`. Read by the new conditional via `!override_active`.

## New conceptual entity (test-only)

### `ClosureSet` (conceptual; not a Rust type)

```text
ClosureSet S = components[].bom-ref ∪ {metadata.component.bom-ref}
```

The set of all bom-ref values *declared* by the document. Computed in the new regression test at `mikebom-cli/tests/cdx_ref_closure_invariant.rs` from the parsed CDX 1.6 JSON (`serde_json::Value`) and represented concretely as a `HashSet<String>`. No persisted shape; computed per test invocation.

## Validation rules

- **VR-084-001**: `target_ref` (the value passed to `build_compositions` and `build_dependencies` in `cyclonedx/builder.rs:342-348`) MUST equal `metadata.component.bom-ref` (computed by `build_metadata` at `cyclonedx/metadata.rs:391-409`) for every emission where `main_module.is_some() && !override_active`. Encoded operationally by the new closure-invariant regression test.
- **VR-084-002**: For every emitted CDX 1.6 document, the closure invariant holds: `dependencies[].ref ∪ dependencies[].dependsOn[] ∪ compositions[].assemblies[] ∪ compositions[].dependencies[] ⊆ S` where `S = components[].bom-ref ∪ {metadata.component.bom-ref}`. Tested across all post-053 ecosystem fixtures per FR-011 / research §4.
- **VR-084-003**: When `main_module.is_some()`, the `dependencies[]` array MUST contain exactly ONE entry whose `ref` equals the main-module PURL — not two (the orphan and the real one collapse into one). Tested by counting matching entries in the regression test or by golden-diff inspection.
- **VR-084-004**: When `main_module.is_some()`, `compositions[]` MUST contain exactly ONE entry whose `assemblies[]` contains the main-module PURL with `aggregate: incomplete_first_party_only`, AND exactly ONE entry whose `dependencies[]` contains the main-module PURL with `aggregate: complete`. (The inventory `complete` composition #1 already includes the main-module PURL pre-fix; that doesn't change.)
- **VR-084-005**: When `main_module.is_none()` AND no override is active (no-manifest fallback), the legacy super-root behavior is byte-identical pre/post — the alpha.23 golden for any such fixture regenerates with zero diff (FR-004 / SC-003).
- **VR-084-006**: When override is active (milestone 077), the override identity `format!("{name}@{version}")` flows into both `metadata.component.bom-ref` AND `target_ref` (already true pre-fix); additionally, per research §2 Option A, any `relationships[]` edges keyed off the now-dropped main-module PURL are filtered or rewritten so that `dependencies[].ref` does not contain the main-module PURL as an orphan. Closure invariant from VR-084-002 holds in the override path.
- **VR-084-007**: SPDX 2.3 + SPDX 3 emission MUST be byte-identical pre/post merge. No changes to `mikebom-cli/src/generate/spdx*/`. Verified by running `cargo +stable test -p mikebom --test spdx_regression` and `cargo +stable test -p mikebom --test spdx3_regression` pre/post merge.

## Backward compatibility

- **Operator-perceived**: post-fix CDX 1.6 documents have one fewer `dependencies[]` entry and two retargeted `compositions[]` entries. Any consumer that walked-down-from-metadata.component is unaffected. Any consumer that used the orphan ref as a load-bearing identifier was already broken; the fix removes the source of confusion.
- **Internal**: no public Rust API changes. `cyclonedx-cli` validators and downstream tools see fewer dangling-ref warnings; their decision logic is unchanged.
- **Goldens**: post-053 ecosystem CDX goldens regenerate per research §3. No-manifest fallback goldens stay byte-identical. Override goldens regenerate with the §2-Option-A scope (relationship filter for the override path) — diffs MUST stay within VR-084-006's invariant.
- **No new Cargo dependencies**: ratified by `cargo tree --invert <new-crate>` returning empty (no new crate is introduced).
- **No CI workflow changes**: the new regression test runs under the existing `lint-and-test` lane via `cargo test --workspace`. Linux + macOS CI lanes both exercise it (CDX emission is platform-agnostic).
- **No goldens regenerated outside CDX**: SPDX 2.3, SPDX 3, and any non-CDX emission paths stay byte-identical (FR-007 / VR-084-007).
