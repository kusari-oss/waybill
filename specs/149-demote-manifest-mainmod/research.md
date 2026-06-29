# Research — milestone 149 (preserve manifest-derived main-module as demoted library entry when `--root-name` overrides it)

Phase 0 output. Resolves five implementation-affecting design questions before Phase 1.

## §A — Drop sites are three symmetric locations across the emitters

**Decision**: The drop logic that the new preserve branch hooks into lives at three symmetric sites across the per-format emitters. Consolidating into a single shared helper in `root_selector.rs` is structurally cleaner than threading a per-format conditional through each emitter.

**Verification** (grep at plan-phase time):

| Site | Lines | Pattern |
|---|---|---|
| `mikebom-cli/src/generate/cyclonedx/builder.rs` | 325-347 | `let mut dropped_main_module_purls: Vec<String> = Vec::new(); ... if override_active { ... iterate components, drop main-modules, populate dropped_main_module_purls for relationship re-anchoring ... }` |
| `mikebom-cli/src/generate/spdx/document.rs` | 262-282 | Same shape as CDX; `dropped_main_module_purls` populated for the same relationship-re-anchoring purpose. |
| `mikebom-cli/src/generate/spdx/v3_document.rs` | 57-75 | Same shape. |

All three sites use:
- The same predicate: `c.extra_annotations.get("mikebom:component-role").and_then(|v| v.as_str()) == Some("main-module")`
- The same outer gate: `if override_active { ... } else { components.to_vec() }` (or equivalent)
- The same output Vec: `dropped_main_module_purls: Vec<String>` for downstream relationship re-anchoring

The duplication is intentional — each emitter owns its own filtering pass — but it's a maintenance liability for milestone 149 because the new preserve branch would have to land at all three sites identically.

**Decision rationale**: extract into a single shared helper in `root_selector.rs` (the natural home given the helper consumes `RootComponentOverride` and produces a filtered `Vec<ResolvedComponent>`). The refactor is net-LOC-neutral (~25 LOC × 3 sites = ~75 LOC removed; ~40 LOC helper added; ~10 LOC of call-site replacement = ~50 LOC across the 3 emitters).

**Alternatives considered**:
- **Threading the preserve branch through each emitter inline** — rejected: triples the maintenance surface; any future drop-behavior tweak (e.g., demote-and-also-emit-VARIANT-OF-relationship per a future milestone) would land at three places.
- **A per-emitter helper in each emitter's module** — rejected: same duplication, just renamed. The point of consolidation is to reduce duplication.

## §B — Demote transformation is purely metadata-level

**Decision**: When the preserve branch fires, the demote transformation on the main-module's `ResolvedComponent` instance is:

1. **Remove** `extra_annotations["mikebom:component-role"]` (the value is `"main-module"`). This single removal causes downstream type-derivation to fall through to the default library type (CDX `type: "library"`, SPDX 2.3 + SPDX 3 default `Package` shape).
2. **Add** `extra_annotations["mikebom:demoted-from-main-module"] = Value::String("true")`. The new annotation IS the transparency signal per FR-004.
3. **Keep** every other field of `ResolvedComponent` verbatim — `purl`, `name`, `version`, `parent_purl`, `evidence.*`, `hashes`, `lifecycle_scope`, `licenses`, `concluded_licenses`, `supplier`, `cpes`, etc. The demoted entry IS the same manifest-derived component, just with a different role annotation.

**Verification**: read the CDX builder's type-derivation code at line 658:

```rust
"type": binary_role_to_cdx_type(component.binary_role),
```

Type is derived from `component.binary_role`, not from any role-tag check. Main-modules have `binary_role: None`, so `binary_role_to_cdx_type(None)` returns `"library"` (the default). The main-module's "application" type in the emitted CDX comes from a DIFFERENT path: the `metadata.component` block which receives the operator-override identity (when override active) OR the milestone-053 main-module promotion (when override inactive + single main-module). The `components[]` siblings get `type: "library"` by default.

**Implication**: removing the `mikebom:component-role: main-module` annotation is sufficient to flip the entry's wire-side `type` from "application" → "library" across all three formats, because the type derivation flows through the existing `binary_role`-based path (which has always been the default-library path for non-main-modules).

**Per US1 clarification Option A** (recorded 2026-06-29): even when the entry is KEPT (demoted), its PURL MUST still be added to `dropped_main_module_purls` (rename to `redirected_main_module_purls` for clarity post-149) so the existing relationship re-anchoring logic (milestone 084 at `cyclonedx/builder.rs:442-447` and parallel sites in SPDX 2.3 + SPDX 3) re-routes dep edges from the (now-demoted) main-module's PURL onto the operator-override root's bom-ref. The demoted entry has no outbound `dependsOn` edges in the wire output.

**Alternatives considered**:
- **Mutate `component.type` directly** — rejected: `ResolvedComponent` doesn't have a `type` field; `binary_role` is the closest analog. Mutating `binary_role` would risk affecting other code paths that read it.
- **Add a new `is_demoted: bool` field on `ResolvedComponent`** — rejected: pollutes the struct with milestone-specific state. The annotation-bag approach IS the existing pattern for transparency signals.

## §C — Cross-ecosystem coverage is free via the centralized helper

**Decision**: Because the new helper operates on the post-dedup `Vec<ResolvedComponent>` (where every ecosystem reader's main-module has been normalized to carry the `mikebom:component-role: main-module` annotation per milestones 053 / 064 / 066 / 068 / 069 / 070), cross-ecosystem coverage is automatic. The helper has zero ecosystem-specific code paths.

**Verification**: read milestone-127 `tag_main_modules_with_workspace_root` at `scan_fs/mod.rs:802` — it iterates ALL components regardless of ecosystem and adds the `mikebom:component-role: main-module` annotation for entries identified as main-modules by the per-reader logic. The annotation is the universal handoff between readers and the root-selector pipeline.

**Implication for SC-001**: covered for all six ecosystems (Cargo, npm, pip, gem, Maven, Go) with no per-reader changes. The CI-binding integration test (SC-004) need only cover a representative trio (Cargo + npm + Go per spec); operator-cadence verification covers the remaining three (pip + gem + Maven per Assumption 8).

## §D — Parity-catalog row scoping

**Decision**: add a single new parity-catalog row `C102` (verify next available number at implement time — milestone 148 added C101) for the new `mikebom:demoted-from-main-module` annotation with `Directionality::SymmetricEqual` (boolean-string `"true"` value round-trips byte-identically across CDX `properties[]`, SPDX 2.3 envelope annotation, and SPDX 3 envelope annotation).

**Verification**: read the existing pattern from milestone 148's C101 (`mikebom:peer-edge-targets`) at `mikebom-cli/src/parity/extractors/mod.rs:404+` — the row uses the existing `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros with `component` scope (per-component annotation). Same pattern applies to C102.

**Implication**: the existing CI-binding parity tests (`cross_format_byte_identity`, `holistic_parity`) automatically exercise C102 once the integration-test fixtures' goldens refresh with the new annotation set. No new parity-test infrastructure needed.

## §E — CLI flag plumbing

**Decision**: add `preserve_manifest_main_module: bool` (default `false`) to the existing `ScanArgs` clap struct via `Args`-derive. Thread through `ScanRequest` (the existing request struct) into `ScanArtifacts` (the existing struct passed to each emitter) so the helper has access via the existing emitter call signature without a new parameter at every call site.

**Verification**: read milestone 134's `--check-divergence` flag at `cli/scan_cmd.rs` and milestone 119's `--supplement` flag — both follow the same pattern: clap field → ScanArgs → ScanRequest → ScanArtifacts → emitter reads via `artifacts.<field>`. Established convention; no design call needed.

**Alternatives considered**:
- **An env-var override** (`MIKEBOM_PRESERVE_MAIN_MODULE=1`) — rejected: the flag is operator-facing (not infrastructure-facing); CLI flag is the correct surface.
- **Auto-enable when `--root-name` is set** — rejected: would change milestone-077 default behavior; spec FR-007 explicitly requires backward compat (default off).

## Summary of decisions feeding Phase 1

- **§A**: One shared helper at `mikebom-cli/src/generate/root_selector.rs::apply_main_module_drop_or_demote()` consolidates the duplicated drop logic from three emitters.
- **§B**: Demote transformation is purely metadata-level — remove main-module role tag, add demote annotation, no type field mutation needed (downstream type-derivation naturally produces library).
- **§C**: Cross-ecosystem coverage is automatic via the centralized helper operating on the post-dedup Vec where every ecosystem reader's main-module is normalized.
- **§D**: Single new parity-catalog row C102 with SymmetricEqual directionality (verify next available number at implement time).
- **§E**: CLI flag `--preserve-manifest-main-module: bool` plumbed via the existing ScanArgs → ScanRequest → ScanArtifacts pattern.
- **No new Cargo dependencies.**
