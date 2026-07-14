# Research: m191 Design-Tier / Source-Tier Reconciliation

**Date**: 2026-07-14
**Purpose**: Resolve the technical unknowns identified in `plan.md` Technical Context. Determined by direct code inspection and cross-reference with prior milestones (m127 root-pick, m149 mainmod demote, m163 npm cross-workspace, m179/m183/m184 lifecycle-scope + optional-dep).

## R1 — Reconciliation insertion point

**Decision**: Insert `reconcile_design_source_tiers(components)` as a new call site in BOTH `scan_fs/mod.rs:807` (after the first `deduplicate` call) AND `cli/scan_cmd.rs:2742` (after the second `deduplicate` call). Wrap in a small helper so the two call sites stay in sync.

**Rationale**: Verified pipeline via direct code inspection:

- `scan_fs/mod.rs:807` — `let mut components = deduplicate(components);` runs on the raw scan-fs output.
- `cli/scan_cmd.rs:2742` — `components = crate::resolve::deduplicator::deduplicate(components);` runs again after pass-2 (resolution / enrichment). This second dedup exists per the comment at line 2733: "fold. Re-running `deduplicate()` here closes the loop."

Both dedup call sites are followed by graph-completeness + emission. The m191 reconciliation MUST run before those downstream steps so:
1. Graph-completeness sees the reconciled component set (avoids counting design+source as two orphans/reachable).
2. Format emitters serialize the reconciled shape into every format uniformly.

Inserting AFTER `deduplicate` (not before) is correct because dedup normalizes identity keys — the reconciler then operates on a stable component list.

**Alternatives considered**:
- (A) Inside `deduplicate`: rejected. The existing dedup key is `(ecosystem, name, version, parent_purl)`; m191's match key is `(ecosystem, name, workspace-scope)` — different semantic, different merge rule. Combining would obscure both rulesets.
- (B) Post-emission normalization inside each format emitter: rejected. Would require 3× duplication (CDX + SPDX 2.3 + SPDX 3), each with format-specific edge-rewriting logic.
- (D — **CHOSEN**) New pass in `resolve/reconciler.rs` called from both dedup sites.

**References**:
- `mikebom-cli/src/scan_fs/mod.rs:807` — first dedup call site.
- `mikebom-cli/src/cli/scan_cmd.rs:2733-2742` — second dedup call site (with in-code justification).
- `mikebom-cli/src/resolve/deduplicator.rs:28` — `deduplicate` signature.
- `mikebom-cli/src/resolve/pipeline.rs:328` — resolution-pipeline dedup (unchanged, orthogonal to scan-fs path).

## R2 — Reconciliation match key + algorithm

**Decision**: Two-pass grouping:

**Pass A** (build source-tier lookup):
```text
source_index: HashMap<(ecosystem, canonical_name, workspace_scope), &SourceComponent>
```

Populated by iterating components where `sbom_tier == Some("source")` (or `"analyzed"` — treated as source-equivalent per existing conventions). `workspace_scope` derivation is per R3 below.

**Pass B** (reconciliation walk):
Iterate every component where `sbom_tier == Some("design")`. For each:
1. Compute match key `(ecosystem, canonical_name, workspace_scope)`.
2. Look up in `source_index`.
3. If exactly one match: transfer design annotations onto the source; mark the design entry for removal; record the ID mapping `design_bom_ref → source_bom_ref` for later graph-edge rewriting.
4. If multiple matches: attach annotations to EVERY matching source component per FR-003.
5. If zero matches: leave the design-tier component in place; the standalone US2 shape (versionless PURL) already applies from the reader.

**Pass C** (graph-edge rewriting):
Walk the `dependencies` / relationship structures in the component-list carrier. For each edge whose target is a removed-design-tier bom-ref, rewrite to the reconciled source-tier bom-ref. Never leave a dangling edge (FR-005).

**Pass D** (removal):
Remove marked design-tier components from the final list.

**Rationale**: Two-pass with a HashMap lookup is O(N) — linear in the component count. The 1998-component customer scan runs in <10ms overhead per T034 back-of-envelope. Passes B + C + D can be interleaved into a single walk with a mutable-list pattern; splitting into logical passes for readability.

**Alternatives considered**:
- (A) Single-pass merge into `deduplicate`: rejected per R1 — different key + different rules.
- (B) Sort-and-scan instead of HashMap: rejected — HashMap is O(N), sort is O(N log N); no ordering requirement.

## R3 — Workspace-scope derivation (per Q2)

**Decision**: `workspace_scope` is a `PathBuf` computed per-component as:

1. Start with the component's `mikebom:source-manifest` annotation value (relative path to declaring manifest, e.g., `packages/foo/package.json`).
2. If that annotation is missing OR the component has NO manifest (transitive lockfile-only), the scope is the scan root — matches "standalone project" case per Q2 answer A.
3. Walk UP the directory tree from the manifest's parent, looking for a workspace-parent marker:
   - npm: parent `package.json` with `"workspaces"` array claiming the child directory.
   - pnpm: parent `pnpm-workspace.yaml` with `packages` list matching the child.
   - yarn: parent `package.json` with `"workspaces"` (same as npm).
   - Cargo: parent `Cargo.toml` with `[workspace] members` array.
   - Python (uv / poetry): parent `pyproject.toml` with `[tool.poetry.group.dev.dependencies]` or `[tool.uv.workspace]`.
   - Composer: parent `composer.json` with `"repositories"` path type.
4. When a workspace parent is found AND it CLAIMS the child directory, the workspace_scope becomes the workspace-root directory (so all peer members share the same scope).
5. When NO workspace parent claims the child, the scope is the component's own manifest-parent directory (so nested independent projects don't cross-reconcile).

**Rationale**: This exactly matches Q2 answer A ("walk up to workspace-parent directories, bounded by workspace-membership"). Reuses infrastructure that m127 (root-pick) + m149 (mainmod demote) + m163 (npm cross-workspace resolution) already implement. Ecosystem-specific claim-checks are simple file reads.

**Implementation detail**: The claim-check MAY be cached per-workspace-root during the reconciliation pass (small HashMap<PathBuf, Vec<PathBuf>>) so repeated lookups for peer members don't repeat the parent-file read. Given typical monorepo shapes (~10-100 workspace members), the cache hit rate is high.

**Alternatives considered**:
- (A) Rely on the existing `parent_purl` field on `ResolvedComponent`: rejected. `parent_purl` is set for JAR-vendored coords (m085 shade-jar work); it's NOT populated for workspace-parent relationships.
- (B) Reuse the m163 `CrossWorkspaceIndex`: rejected — that infrastructure operates on npm-specific `walk.rs` types before ResolvedComponent conversion. m191 runs post-conversion.

## R4 — Annotation transfer mechanics (per Q1 / FR-004)

**Decision**: The source-tier survivor's `extra_annotations` gains, per each reconciled design-tier component:

- A `mikebom:requirement-range` entry (the range string from the design component).
- A `mikebom:source-manifest` entry (the manifest path from the design component).

Because `extra_annotations` is a `BTreeMap<String, serde_json::Value>` (single value per key) in the workspace model, multi-declaration cases (Q1) need a different shape. Two options:

**Option A** (chosen): Extend the shape to `BTreeMap<String, Vec<Value>>` OR use array-valued `Value` for the multi-entry case. Emitters translate to their format-idiomatic representation.

**Option B**: Add a new `Vec<(String, Value)>` field for "multi-source annotations" specifically. Rejected — introduces API surface change to `ResolvedComponent` for a shape distinction only the emitters care about.

**Actual implementation**: Use `serde_json::Value::Array(Vec<Value>)` inside the existing `BTreeMap<String, Value>` when multiple manifests contribute the same annotation key. Single-declaration case remains a scalar `Value::String(...)` — byte-identical to pre-m191 for the common case.

Emitter translation:
- **CDX 1.6**: Each element in the JSON array becomes a separate `properties[]` entry with `name: "mikebom:requirement-range"` and the corresponding value. Paired `mikebom:source-manifest` properties emit in matching order to preserve the pairing per Q1 answer B. The CDX property array is unordered by spec but our emitter is deterministic (sorted by property name + insertion order within a name), so the pairing survives round-trips.
- **SPDX 2.3**: Multiple `Annotation` elements per reconciled component, each with a `Comment` containing `{"mikebom:requirement-range":"^1.0","mikebom:source-manifest":"packages/foo/package.json"}` (JSON-in-comment envelope, per existing m111 pkg-alias-binding convention).
- **SPDX 3**: Multiple `annotation` graph elements per reconciled component, each with `statement` containing the JSON-in-string envelope.

**Rationale**: All three formats already support multi-annotation-per-component. The change is purely how MANY entries the reconciliation attaches (was: 1 per design-tier component; becomes: N per source-tier survivor where N is the reconciled-count).

**Parity extractor impact (FR-017)**: The C20 extractor (`mikebom:requirement-range`) currently returns the first matching property's value. Post-m191 it MUST return ALL matching properties in insertion order — same for CDX + SPDX 2.3 + SPDX 3. Small change to the extractor helpers `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` OR the C20-row extractor bodies specifically. Directionality remains `SymmetricEqual` — no downgrade.

## R5 — Standalone versionless PURL fix (per US2 / FR-009)

**Decision**: Update every per-ecosystem `build_*_purl` helper to emit `pkg:<type>/<name>` (no `@`, no version segment) when the `version` argument is empty. Change the format string from `"pkg:npm/{name}@{version}"` to `"pkg:npm/{name}"` when `version.is_empty()`.

**Ecosystems requiring the fix** (from grep of `requirement_range: Some(...)` populators):
- npm (`scan_fs/package_db/npm/mod.rs::build_npm_purl` @ line 676)
- pip (`scan_fs/package_db/pip/requirements_txt.rs:97` — check the PURL builder in pip module root)
- cargo (`scan_fs/package_db/cargo.rs::build_cargo_purl`)
- maven (`scan_fs/package_db/maven.rs::build_maven_purl`)
- gem (`scan_fs/package_db/gem.rs::build_gem_purl`)
- composer (`scan_fs/package_db/composer.rs:696`)
- dart (`scan_fs/package_db/dart.rs:527`)
- cocoapods (`scan_fs/package_db/cocoapods.rs:697`)
- scala (`scan_fs/package_db/scala.rs:1227`)
- haskell (`scan_fs/package_db/haskell.rs:966`)
- erlang (`scan_fs/package_db/erlang.rs:1637`)

Each fix is ~5 lines: an `if version.is_empty()` branch producing the versionless PURL string, otherwise the existing shape unchanged. **NOT extracted to a shared helper** because each ecosystem's PURL shape carries its own encoding rules (scoped-name handling, namespace segments, etc.). Duplicating the branch per ecosystem is cleaner than a generic helper with per-ecosystem hooks.

**Alternatives considered**:
- (A) Shared helper `build_versionless_purl_shape(pkg_type, name, extras)`: rejected — the extras (scopes, namespaces, qualifiers) vary too much; the shared surface would be brittle.
- (B) Emitter-side normalization (strip trailing `@` at CDX/SPDX time): rejected — leaves the PURL string wrong at the `ResolvedComponent.purl` level; every consumer that reads the model directly (parity extractors, graph-completeness, tests) still sees the malformed form.
- (C — **CHOSEN**) Fix at PURL construction time in each `build_*_purl` helper. Small local changes; single-source-of-truth per ecosystem.

**Byte-identity preservation**: Every existing golden that had a non-empty version passes byte-identically because the new `if version.is_empty()` branch never fires for non-empty inputs. Only design-tier components with no source-tier match AND no concrete version see the new shape — and per T003-style audit, we expect the drift set to be zero (design-tier-with-no-match is rare in the current fixture corpus).

## R6 — Format-specific version-field omission (per FR-010 / FR-011 / FR-012)

**Decision**: Emitter-side changes downstream of the PURL fix:

- **CDX 1.6** (`generate/cyclonedx/builder.rs`): the emitter currently emits `.version` unconditionally as a JSON string. Change to `if !component.version.is_empty() { entry["version"] = json!(component.version); }` — matches the existing pattern used for other optional fields (e.g., `.licenses` at line 937). Preserves byte-identity for non-empty version paths.
- **SPDX 2.3** (`generate/spdx/packages.rs`): the `SpdxPackage.version_info` field is `String` today. When the source component's `.version` is empty, populate it with the string `"NOASSERTION"` (matches the existing NoAssertion patterns for downloadLocation + supplier).
- **SPDX 3** (`generate/spdx/v3_document.rs` OR wherever `software_Package` is built): the `software_packageVersion` property is emitted unconditionally when `.version` is non-empty; when empty, omit the property from the graph element. Matches the FR-012 wire shape.

**Rationale**: All three changes are minimal, format-idiomatic, and mirror existing empty-field-handling patterns elsewhere in each emitter.

## R7 — Graph-edge rewriting for FR-005

**Decision**: The `dependencies` structure on the component-list carrier (workspace model) or the per-format `dependsOn`/`DEPENDS_ON` relationships must have any edge pointing at a removed design-tier bom-ref rewritten to the reconciled source-tier bom-ref.

**Implementation**: After Pass C's ID-mapping is built (`HashMap<design_bom_ref, source_bom_ref>`), walk every component's `relationships` field (if the model has one) OR walk the emitter-time relationship-building code to consult the mapping. If a target ID matches an entry in the map, rewrite the target to the mapped source ID.

**Verification**: Investigation task — locate where relationships are stored in the pre-emission model. Likely candidates:
- `ResolvedComponent.dependencies: Vec<Purl>` (if present).
- A sibling `Vec<Relationship>` struct passed alongside `Vec<ResolvedComponent>` through the pipeline.
- Format-specific relationship construction inside each emitter (if per-emitter, the rewrite happens at each emitter's relationship-build step).

**Alternatives considered**:
- (A) Emit relationships as-is and rely on downstream consumers to normalize dangling edges: rejected — that's exactly the bug FR-005 forbids.
- (B) Emit BOTH the design and source ID with a "hasEquivalent" relationship: rejected — the point is to have ONE component post-reconciliation, not two + a relationship.

## R8 — Backward-compatibility with existing goldens (FR-016, SC-006)

**Decision**: Grep every golden `.cdx.json` and `.spdx.json` for the pattern `pkg:<type>/<name>@"` (PURL with empty version and trailing `@`) AND for pairs where the same `name` appears in two components with different versions (design/source pair signal). Any hit → those specific goldens MUST be regenerated as part of the milestone. Any golden without a hit → byte-identity MUST be preserved (SC-006 gate).

**Rationale**: Deterministic identification of drift set. Prevents surprise byte-identity failures at PR time. Same procedure as m190 T003.

**Regen strategy** (per memory `feedback_release_bump_regen_all_golden_tests`):
1. Identify affected goldens via grep.
2. Run `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test -p mikebom` on the identified test files only.
3. Diff-review the regenerated files: every diff MUST be either (a) two `commander@` components collapsing to one `commander@11.1.0`, (b) a `pkg:npm/foo@` becoming `pkg:npm/foo`, or (c) a `version: ""` becoming an omitted field / `NOASSERTION`. No other diffs permitted.
4. Commit the regen alongside the fix, cite the exact drift class in the commit message.

## R9 — Constitution Principle V audit (native-first)

**Decision**: NO new `mikebom:*` annotation is introduced. Every fix uses standards-native fields:

| Fix | Native Field Used | Format |
|---|---|---|
| US1 reconciliation (annotation transfer) | Multiple `properties[]` entries with same `name` | CycloneDX 1.6 §5.7 property array |
| US1 reconciliation (annotation transfer) | Multiple `Annotation` elements per Package | SPDX 2.3 §8 Annotation |
| US1 reconciliation (annotation transfer) | Multiple `annotation` graph elements per Package | SPDX 3.0.1 § Annotation |
| US1 graph-edge rewriting | `dependencies[].dependsOn` (CDX), `DEPENDS_ON` (SPDX 2.3), `dependsOn` (SPDX 3) — existing edge types, just rewritten targets | All three |
| US2 versionless PURL | Purl-spec canonical shape (`pkg:<type>/<name>`) | purl-spec §5 |
| US2 CDX version omission | Optional `.version` field per CDX 1.6 §5.3 | CDX 1.6 |
| US2 SPDX 2.3 NOASSERTION | Spec-standard sentinel | SPDX 2.3 |
| US2 SPDX 3 property omission | Optional `software_packageVersion` per SPDX 3.0.1 vocab | SPDX 3 |

**Rationale**: Direct audit result satisfying Principle V's "spec authors MUST cite the audit result in the spec's Functional Requirements". Recorded in FR-018. Reviewers can reject any implementation-time drift that introduces a `mikebom:*` alternative.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Reconciliation location | New pass after both existing `deduplicate` sites | Inside `deduplicate`; per-emitter | Distinct key + rules; clear responsibility separation |
| Match key | `(ecosystem, canonical_name, workspace_scope)` | Include version; include hash | Version differs by design; hash not always present |
| Workspace-scope | Walk up to workspace-parent when claimed; else own manifest-dir | Strict same-dir; walk arbitrarily | Matches Q2 answer A; reuses m163 plumbing |
| Multi-decl storage | `serde_json::Value::Array` inside `BTreeMap<String, Value>` | New `Vec<(K, V)>` field on `ResolvedComponent` | Minimal API surface change |
| Multi-decl emission (CDX) | Multiple `properties[]` entries | Single JSON-array-encoded property | Per Q1 answer B |
| PURL fix location | Per-ecosystem `build_*_purl` helpers | Shared helper; emitter-side strip | Local + minimal; each ecosystem has unique shape rules |
| Empty-version emission | CDX omit / SPDX 2.3 NOASSERTION / SPDX 3 omit | All uniform NOASSERTION | Per-format-idiomatic; consistent with each emitter's existing patterns |
| Bom-ref for standalone | Versionless PURL as-is | Synthetic prefix; hashed | Per Q3 answer A |
| Observability | INFO summary + DEBUG per-component | Silent; document-scope annotation | Per Q4 answer B |
| New Cargo deps | None | Add semver crate | Zero-new-deps posture confirmed |
| New `mikebom:*` annotations | None | Add `mikebom:reconciled-from` | Per FR-018 / Principle V |
