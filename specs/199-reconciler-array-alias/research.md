# Research: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Date**: 2026-07-15
**Purpose**: Resolve 4 mechanical unknowns before task decomposition. Much inherited from m197 research §R4-R5 (which covered these stories in the parent bundle); m199 verifies the code-path claims against current tree state.

## R1 — Reconciler transfer-logic location

**Investigation** (`grep -n` of `mikebom-cli/src/resolve/reconciler.rs`):

- Line 37: `pub fn reconcile_design_source_tiers(...)` — entry point.
- Lines 54-56: match-key defined as `(ecosystem, canonical_name, source_manifest_dir)`.
- Lines 85-105: the transfer logic — currently branches on `.contains_key("mikebom:requirement-range")` and `.contains_key("mikebom:source-manifest")`, transferring only if the source doesn't already have the annotation (first-wins semantics).

**Decision**: Rewrite lines 85-105 in-place to:
1. Always initialize the survivor's `mikebom:requirement-ranges` as an empty `serde_json::Value::Array(vec![])` on first design-tier match encountered.
2. On each match, push the design-tier's declared range onto the array (mirror for `mikebom:source-manifests`).
3. Post-transfer sort per FR-003 — sort `mikebom:source-manifests` lex, reorder `mikebom:requirement-ranges` 1:1 to match.
4. Remove any residual code path emitting the singular `mikebom:requirement-range` / `mikebom:source-manifest` scalar keys — these field names must not appear on m199-post survivors.

**Alternatives considered** (all rejected in m197 clarify):
- Emit both singular AND array for backwards compat: rejected per m197 Q1 always-array decision.
- Post-pass rewrite instead of in-place: rejected — atomic edit at existing site is simpler + reviewer-friendly.

**References**:
- `mikebom-cli/src/resolve/reconciler.rs:85-105` — the code block to rewrite.
- m197 spec Q1 clarification + data-model E2/E3.

## R2 — npm alias extraction from package.json

**Investigation**:
- `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs:37` — the `AliasResolution` struct already carries `local_name` (alias) + `aliased_name` (resolved) + `aliased_version` as distinct fields. m159 (pnpm alias) already produces these values.
- Line 42-45 confirms: `pub(crate) local_name: String` + `pub(crate) aliased_name: String` + `pub(crate) aliased_version: String`. All three exist today.
- Existing detector `detect_pnpm_alias` (m159) works on pnpm-lock.yaml declarations. No equivalent detector for `package.json` inline declarations yet.

**Decision**: Add a new function `parse_package_json_alias(dep_name: &str, dep_ver_raw: &str) -> Option<AliasResolution>` alongside `detect_pnpm_alias`. Logic: if `dep_ver_raw` starts with `"npm:"`, strip the prefix and parse `@<scope>/<name>@<version>` OR `<name>@<version>` via `str::rsplit_once('@')`. Return `AliasResolution { local_name: dep_name.into(), aliased_name: <parsed>, aliased_version: <parsed>, ..., ecosystem: AliasEcosystem::Npm }`.

**Grammar variants to handle**:
- `"my-alias": "npm:actual@1.0.0"` — unscoped, versioned
- `"my-alias": "npm:@scope/actual@1.0.0"` — scoped, versioned (rsplit_once on the LAST `@` since scope also uses `@`)
- `"my-alias": "npm:actual@^1"` — unscoped, range (mikebom's design-tier just stores the raw range; no resolution)

**Alternatives considered**:
- Extend `detect_pnpm_alias` to handle package.json's alias form too: rejected — pnpm's YAML lookup context is different from package.json's map iteration; a parallel function with clear name is more discoverable.

**References**:
- `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs:37-75` — `AliasResolution` + existing `detect_pnpm_alias`.
- m197 spec data-model Entity 4 — `AliasResolution` extension expectations.

## R3 — npm reader stamping design-tier component with `mikebom:declared-as`

**Investigation** (grep for design-tier dep emission in npm reader):
- `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` — the m066 mainmod loop reads package.json deps and constructs `PackageDbEntry` entries for each.
- Need to find where the package.json dep-name / dep-value pair gets iterated and turned into a `PackageDbEntry` — that's the stamp site.
- Grep pattern to use during implementation: `grep -n "dependencies\|dev_dependencies\|package.json" mikebom-cli/src/scan_fs/package_db/npm/mod.rs`.

**Decision**: At the design-tier dep-emission site inside `npm/mod.rs` (per m066 loop or an adjacent site), invoke `parse_package_json_alias(dep_name, dep_value)`. When it returns `Some(alias)`:
1. Use `alias.aliased_name` (resolved identity) as the PURL construction input — the design-tier component gets PURL `pkg:npm/<resolved_name>` (versionless per m191/m197 US3) or `pkg:npm/<resolved_name>@<range>` if the range parses.
2. Stamp `extra_annotations["mikebom:declared-as"] = json!([alias.local_name])` on the emitted `PackageDbEntry`.

When it returns `None` (regular dep), no `mikebom:declared-as` annotation stamped.

**Alternatives considered**:
- Emit two components (one keyed on alias, one on resolved) and let reconciler dedupe: rejected — the pre-m199 buggy behavior. Emitting exactly one component keyed on resolved identity is cleaner.

## R4 — Golden + code-site regen scope (REVISED 2026-07-16 after implement-phase T002 audit)

**Original investigation flaw**: The initial grep targeted `mikebom-cli/tests/fixtures/golden/` — a directory that does NOT exist in this project. The real golden directory is `mikebom-cli/tests/fixtures/public_corpus/` (per m195/m196). The bad path returned 0 hits and led to the false conclusion that no goldens exercised the singular-scalar path.

**Corrected investigation** (T002 audit at implement time):

```bash
grep -rlE '"mikebom:requirement-range"|"mikebom:source-manifest"' mikebom-cli/tests/
grep -rnE 'requirement_range|"mikebom:source-manifest"' mikebom-cli/src/ mikebom-common/src/
```

Actual scope:

| Layer | Count | Sites |
|---|---|---|
| Public-corpus goldens | **9 files, 234 singular hits** | python-flask + maven-guice + npm-express × {cdx.json, spdx-2.3.json, spdx-3.json} |
| Emitter stamp sites | **3** | `cyclonedx/builder.rs:1079`, `spdx/annotations.rs:318`, `spdx/v3_annotations.rs:333` (all read `component.requirement_range` and emit singular scalar) |
| Reconciler sites | **6** | `resolve/reconciler.rs` lines 85-105 (transfer), 226 (source-manifest read), 269 (source-manifest write), 359/367 (edge rewrite), 293 (spec-init) |
| Schema field | **1 rename + ~25 init sites** | `mikebom-common/src/resolution.rs:82` `requirement_range: Option<String>` + default-init at ~23 other sites |
| Reader stamps | **2 direct writers** | `haskell.rs:1202` (writes `mikebom:requirement-range` to extra_annotations), `npm/mod.rs:679` (writes `mikebom:source-manifest`) |
| Parity extractor rows | **1 row × 4 files** | C20 in `parity/extractors/{mod,cdx,spdx2,spdx3}.rs` |
| Test-site assertions | **10 sites in 8 files** | `scan_python.rs:273`, `scan_maven.rs:717`, `scan_npm.rs:247`, `dart_design_tier.rs:132/138`, `cocoapods_tier_fallbacks.rs:118`, `composer_tier_fallbacks.rs:148/153`, `elixir_tier_fallbacks.rs:121`, `haskell_edge_cases.rs:178` |

**Decision (REVISED)**: Full schema rotation per user selection (Option 1 at implement-phase scope-drift disposition):

1. Rename `ResolvedComponent.requirement_range: Option<String>` → `requirement_ranges: Vec<String>` at `mikebom-common/src/resolution.rs:82`.
2. Migrate every default-init + reader-write site (`requirement_range: None` → `requirement_ranges: Vec::new()`; `requirement_range: Some(x)` → `requirement_ranges: vec![x]`).
3. Rotate the 3 emitter stamp sites to read the plural field and emit `mikebom:requirement-ranges` as JSON array (always-array shape per FR-001).
4. Rotate the 2 direct writers (haskell + npm) to use plural annotation names.
5. Rewrite the reconciler transfer path (per R1) to accumulate arrays instead of first-wins scalars.
6. Update the C20 parity row to reflect the pluralized annotation names.
7. Regenerate all 9 public-corpus goldens.
8. Update all 10 test-site assertions to the plural annotation names.
9. Land US2 alias work (per R2/R3) on top of the rotated substrate.

**LOC revision**: plan.md's original 500-LOC estimate is superseded — actual scope is ~1200-1500 LOC (dominated by 234 golden bytes + ~25 init-site updates, most mechanical).

**Implication for SC-005**: satisfied by the diff-review being scoped to (a) shape rotation across 9 goldens, (b) new `mikebom:declared-as` on the 2 US2 fixture goldens. Every diff falls in these two classes.

**Alternatives considered + rejected at implement-phase disposition**:
- Narrow FR-001 to reconciler-survivors only (Option 2): rejected because SC-002's "anywhere" was load-bearing.
- Split into m199a (alias) + m199b (schema rotation) (Option 3): rejected because bundling reuses the schema-migration cache invalidation once instead of twice.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Reconciler rewrite site | In-place at lines 85-105 | Post-pass rewrite | Atomic edit, reviewer-friendly |
| Alias-in-package.json parser | New `parse_package_json_alias` alongside `detect_pnpm_alias` | Extend detect_pnpm_alias | Clearer naming; separate contexts |
| Design-tier PURL under alias | Keyed on resolved name (`aliased_name`) | Keyed on alias name | Reconciler match-by-identity works naturally |
| `mikebom:declared-as` accumulation | Sorted lex + deduped per m197 data-model E1 | Preserve duplicates like E2/E3 | m197 clarified in analyze phase; alias-count is not provenance |
| Golden regen scope | 0 pre-existing goldens (empirical) | Preemptive fixture addition | Zero-drift on existing goldens; new fixtures only |
| New Cargo deps | Zero | (n/a) | Nothing needed |
