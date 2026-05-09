# Research — milestone 091 go.sum-fallback step 5

Phase 0 investigation. Three decision points; all resolved without further clarification.

## §1 — Per-component provenance: native field per format

Constitution Principle V mandates auditing each output format for an existing native construct before introducing a `mikebom:*` property. Findings:

### CDX 1.6
- **Native construct**: `Component.evidence.identity[].methods[].technique` (string enum). Permitted values include `manifest-analysis`, `binary-analysis`, `source-code-analysis`, `filename`, `ast-fingerprint`, `hash-comparison`, `instrumentation`, `attestation`, `other`. There's no `lockfile-fallback` value in the spec, so the closest semantic match is `manifest-analysis` with a lower `confidence` (≤0.5 per spec §6.4 guidance).
- **Existing mikebom usage**: every emitted component already carries `evidence.identity[]` with `technique = manifest-analysis` and `confidence = 0.85` (verified at `mikebom-cli/src/generate/cyclonedx_v1_6.rs`).
- **Discriminator approach**: for step-5 components, set `confidence = 0.50` instead of `0.85`. The per-format-native discriminator is the *value* of an existing field, not a new field. **Constitution V cleanly satisfied — zero new mikebom:* properties needed for CDX.**

### SPDX 2.3
- **Native construct**: `Package.annotations[]` array of `{annotationDate, annotator, annotationType, comment}` objects. `annotationType` is enum `REVIEW | OTHER`; the prose semantics live in `comment`.
- **Existing mikebom usage**: package-level annotations are the canonical SPDX 2.3 transparency carrier. milestone 084 added `mikebom:resolver-step` annotations (verified at `mikebom-cli/src/generate/spdx_2_3.rs:emit_package_annotations`).
- **Discriminator approach**: emit an annotation with `annotationType = OTHER` and `comment = "mikebom:resolver-step=go-sum-fallback"` on each step-5 component's package. Reuses milestone-084's existing pattern; new value `go-sum-fallback` adds to the value space without new fields.

### SPDX 3
- **Native construct**: `software_Package.evidence` is a typed `Element` reference, but mikebom's existing emission uses the per-element `Annotation` type with `subject` cross-referencing the package and `statement` carrying the prose.
- **Existing mikebom usage**: milestone 084 emits SPDX 3 annotations with `statement = "mikebom:resolver-step=<step>"` (verified at `mikebom-cli/src/generate/spdx_3_0_1.rs:emit_resolver_step_annotation`).
- **Discriminator approach**: same as SPDX 2.3 — extend the annotation's value space. New value `go-sum-fallback`.

**Decision**: reuse the existing milestone-084 `mikebom:resolver-step` carrier across all 3 formats. Add `go-sum-fallback` to the value enum. Zero new fields, zero new `mikebom:*` properties. Constitution V cleanly satisfied because the precedent (the `mikebom:resolver-step` field) already exists and was previously audited.

**Rationale**: introducing a new field would force a fresh Constitution V audit, a docs/reference/sbom-format-mapping.md update, and a parity-bridging justification — none of which add operator value over the existing convention. The existing field's per-format-native shape (CDX `Component.evidence.identity[].methods[].technique` with custom-property fallback for the value, SPDX 2.3 `package.annotations[].comment`, SPDX 3 `Annotation.statement`) is already the right idiom; we extend the enum value space, not the schema.

**Alternatives considered**:
- **CDX `Component.evidence.identity[].confidence` differential alone** (without the `mikebom:resolver-step` property): operator-readable but distinguishes step-5 from step-1/2/3 only via a numeric threshold. Brittle for downstream consumers. Rejected.
- **Per-edge `Relationship` annotation in SPDX 3**: SPDX 3 supports per-relationship annotations, but CDX 1.6 + SPDX 2.3 don't — using SPDX 3's higher-fidelity native field would force the other two formats into a `mikebom:*` workaround. Spec clarification (session 2026-05-09) pinned per-component, ruling this out.

## §2 — Where does step 5 live? Resolver-internal vs post-resolver

**Decision**: in `GraphResolver::resolve()` at `graph_resolver.rs:322`, between current step 3 and step 4. Step 5 augments the root module's edge set by inserting a new entry into `ModuleGraphMap` keyed by the root `ModuleId` (path = workspace's main-module path, version = the resolved root version). The new entry's `requires` list is the deduped go.sum module set; its `source = ResolutionStep::GoSumFallback`.

**Rationale**:
- The existing ladder layout (steps 1, 2, 3 for transitives + step 4 for empty fallthrough) already lives in `GraphResolver::resolve()`. Inserting step 5 in the same function keeps the ladder readable + maintainable.
- The root-module entry isn't currently in `ModuleGraphMap` (step 1 explicitly skips `parent.version().is_empty()` entries). Step 5 adds it explicitly, with version derived from the workspace's root go.mod (or "" for the special case where the workspace root module has no version declared, which is fine — the lookup closure at `legacy.rs:1146-1154` handles empty-version IDs).
- The post-resolver augmentation in `legacy.rs` would require duplicating the root-module-id construction logic AND would split the ladder taxonomy across two files. Single-location dispatch is cleaner.

**Alternatives considered**:
- **Augment in `legacy.rs::read()` after the resolver returns**: feasible but splits the ladder. Rejected for the maintainability reason above.
- **Run step 5 BEFORE step 4 (current order) only when steps 1–3 produced few transitives**: introduces a heuristic threshold that's hard to test. The simpler "always run step 5 + then step 4 catches whatever step 5 missed" composition is more robust. The two are now complementary, not exclusive.

## §3 — Edge attribution semantics in mixed scans

**Decision**: a component's `mikebom:resolver-step` value is the source of the edge that brought the component into the SBOM. In offline-cache-empty mode, every transitive's source is `GoSumFallback`. In cache-populated mode, the same transitive's source is `GoModCache` (or whatever step picked it up). The annotation reflects the discovery mechanism, not a quality score.

**Rationale**:
- mikebom's resolver runs ONE step per scan per (root, transitive) pair — there's no hybrid output where a single component is "discovered via step 2 AND step 5". Steps 1–4 are sequential per the existing implementation; step 5 only fires for entries steps 1–3 didn't claim.
- A component reachable via the project's go.mod direct-`require` block AND via go.sum is tagged with the higher-fidelity step's source (steps 1, 2, or 3 if any populated; step 5 only for the leftovers).
- The `LadderSummary` counter `gosum_fallback_count` (new) joins existing `graph_count` / `cache_count` / `proxy_count` / `missing_count` so operators reading the `tracing::info!` summary can see the per-step distribution at a glance.

**Edge case — `replace` directives**: the existing `apply_replaces` post-pass at `graph_resolver.rs:563` rewrites edge targets; step-5 edges go through this same pass so the replacement target lands in `requires` correctly. Provenance stays attached to the source module's `ResolutionStep`, which is unaffected by replacement.

**Edge case — `+incompatible` versions**: `parse_go_sum` already handles these (the existing milestone-055 contract). Step 5 inherits the behavior; the emitted PURL preserves the suffix verbatim.

**Edge case — multi-module workspaces**: `WorkspaceContext` is per-project-root. Each project root invokes `GraphResolver::resolve()` independently; step 5 fires per-root with that root's go.sum closure. Multi-module workspaces (`go.work` files) get one resolve-pass per member module, each with its own step-5 augmentation if applicable.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (parse go.sum, dedup, emit edges) | §2 → step 5 in `GraphResolver::resolve` reuses `parse_go_sum` + dedup at the (module, version) key level. |
| FR-002 (per-component provenance, native field) | §1 → reuse existing milestone-084 `mikebom:resolver-step` carrier; add `go-sum-fallback` value. Zero new fields. |
| FR-003 (steps 1-3 unchanged) | §2 → step 5 inserts BETWEEN step 3 and step 4 in the same function; existing step bodies untouched. |
| FR-004 (zero-dep / no-go.sum fallthrough) | §3 → step 5's source enumeration (`ctx.go_sum_modules`) is empty when go.sum is missing or empty; step 4 catches whatever's left. No regression. |
| FR-005 (no cache-populated regression) | §3 → step 5 only claims modules NOT yet in the map after steps 1–3; cache-populated path produces the same map as today. |
| FR-006 (transitive_parity_go baseline bump) | Standard milestone-083 quickstart Recipe 3 pattern. |
| FR-007 (per-format scope) | §1 → CDX/SPDX 2.3/SPDX 3 each carry the discriminator in their native idiom via the existing milestone-084 emission code path. Goldens MAY regenerate for `golang/simple-module` IF its modules now flow through step 5; expected delta is the per-component annotation value only. |
| FR-008 (no new Cargo deps) | §1+§2 → reuses `parse_go_sum`, `WorkspaceContext.go_sum_modules`, `ResolutionStep`. Zero new deps. |
| Constitution V audit | §1 → audited; existing carrier reused; no new `mikebom:*` fields. |
| Constitution X transparency | §1+§3 → per-component annotation makes step-5 discovery explicit. |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
