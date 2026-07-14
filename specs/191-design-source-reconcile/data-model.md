# Data Model: m191 Design-Tier / Source-Tier Reconciliation

**Date**: 2026-07-14
**Scope**: In-process types touched by the m191 reconciliation pass. Located in `mikebom-cli/src/resolve/reconciler.rs` (new module) + small deltas in `mikebom_common::resolution::ResolvedComponent` handling. No cross-crate wire-format types (no new `mikebom-common` public exports required for the milestone; the reconciliation types are internal to `mikebom-cli`).

## Entity: ResolvedComponent (existing, UNCHANGED at the Rust struct level)

The reconciliation pass operates on the existing `mikebom_common::resolution::ResolvedComponent` type. NO new fields added. The pass:

- READS: `sbom_tier: Option<String>`, `purl: Purl`, `name: String`, `version: String`, `extra_annotations: BTreeMap<String, serde_json::Value>` (specifically the keys `mikebom:source-manifest` + `mikebom:requirement-range`), `dependencies` / relationship info.
- WRITES: `extra_annotations` on the source-tier survivor (transferring keys from the removed design-tier component).

**Rationale for no struct change**: the milestone is a pure in-memory transformation. The multi-declaration case (Q1) is handled by upgrading the `extra_annotations` value type from a scalar `Value::String` to a `Value::Array(Vec<Value>)` — legal for the existing `Value` field, no schema change required.

## Function: `reconcile_design_source_tiers` (NEW)

**Signature**:

```rust
/// Milestone 191 — reconcile design-tier components into source-tier
/// siblings when a match exists in the same workspace scope. Transfer
/// design-tier metadata onto the survivor; rewrite dep-graph edges;
/// emit INFO summary + DEBUG per-component logs per FR-020.
///
/// Runs after `deduplicate()` in both scan_fs/mod.rs and cli/scan_cmd.rs
/// call sites. See research.md §R1.
pub fn reconcile_design_source_tiers(
    components: Vec<ResolvedComponent>,
) -> Vec<ResolvedComponent>
```

**Location**: `mikebom-cli/src/resolve/reconciler.rs` (new file, ~200 LOC).

**Behavior**:
- Splits input into `(source_tier, design_tier)` partitions.
- Builds `HashMap<(ecosystem, canonical_name, workspace_scope), Vec<usize>>` indexing source-tier component positions (indexing by Vec position rather than &ref to avoid borrow-checker complications during the merge phase).
- Walks design-tier components; for each with a match, transfers its metadata to every matching source-tier component + records `design_bom_ref → source_bom_ref` in a rewrite map.
- Walks all components' dep-graph edges (if present in the component list carrier) — rewrites any edge target present in the rewrite map.
- Emits INFO log summary: `reconciled N design-tier components into source-tier siblings; K standalone design-tier components emitted`.
- Emits DEBUG log per reconciled pair: source component ID + design component ID + transferred annotation keys.
- Returns a new `Vec<ResolvedComponent>` with the reconciled state.

**Invariants**:
- Byte-identity: if the input has ZERO design-tier components, output equals input (early return).
- Byte-identity: if a design-tier component has NO source-tier match, it appears verbatim in the output.
- No-drop: every input component is either preserved as-is, promoted into the source-tier survivor's annotations, or is the source-tier survivor itself. Total metadata is conserved.

## Function: `workspace_scope_for` (NEW helper)

**Signature**:

```rust
/// Compute the workspace-scope path for a component per Q2 (Session
/// 2026-07-14) — walk up from the component's manifest to the workspace
/// parent that claims the child directory. Standalone (non-workspace)
/// projects return the component's own manifest-parent directory.
fn workspace_scope_for(
    component: &ResolvedComponent,
    workspace_index: &WorkspaceIndex,
) -> PathBuf
```

**Behavior**:
- Reads the component's `mikebom:source-manifest` annotation.
- Walks parent directories looking for a workspace-parent marker (npm/pnpm/yarn/cargo/pip/composer variants per R3).
- Consults `workspace_index` — a cached lookup structure built once per reconciliation call from the workspace-parent's members list.
- Returns the workspace-root path when claimed; the manifest-parent directory otherwise.

**Cache**: `WorkspaceIndex` — a `HashMap<PathBuf, HashSet<PathBuf>>` mapping workspace-root → set of member paths. Built lazily on first miss per workspace root; reused for peer members.

## Function: `build_<ecosystem>_purl` (existing, EXTENDED per US2 / R5)

**Existing signature** (npm example — mirrored across all 11 ecosystems):

```rust
fn build_npm_purl(name: &str, version: &str) -> Option<Purl>
```

**Existing behavior**: unconditionally formats `"pkg:npm/{name}@{version}"`. When `version` is empty, produces `pkg:npm/foo@` (the #558 bug).

**Extended behavior (m191)**:

```rust
fn build_npm_purl(name: &str, version: &str) -> Option<Purl> {
    let purl_str = if let Some(rest) = name.strip_prefix('@') {
        let (scope, bare_name) = rest.split_once('/')?;
        if version.is_empty() {
            format!("pkg:npm/%40{}/{}",
                encode_purl_segment(scope),
                encode_purl_segment(bare_name))
        } else {
            format!("pkg:npm/%40{}/{}@{}",
                encode_purl_segment(scope),
                encode_purl_segment(bare_name),
                encode_purl_segment(version))
        }
    } else if version.is_empty() {
        format!("pkg:npm/{}", encode_purl_segment(name))
    } else {
        format!("pkg:npm/{}@{}", encode_purl_segment(name), encode_purl_segment(version))
    };
    Purl::new(&purl_str).ok()
}
```

**Same pattern applies to**: `build_cargo_purl`, `build_pypi_purl` (pip), `build_maven_purl`, `build_gem_purl`, `build_composer_purl`, `build_dart_purl`, `build_cocoapods_purl`, `build_scala_purl`, `build_haskell_purl`, `build_erlang_purl`. Each retains its ecosystem-specific segment-encoding rules.

**Byte-identity preservation**: The empty-version branch never fires for non-empty inputs, so every existing golden with `version != ""` passes byte-identically. Only the standalone design-tier case (unresolved declaration) exercises the new branch.

## Entity: WorkspaceIndex (NEW, internal)

**Purpose**: Cache workspace-membership claims across the reconciliation pass to avoid re-reading `Cargo.toml` / `package.json` / `pyproject.toml` per peer member.

**Shape**:

```rust
struct WorkspaceIndex {
    /// workspace-root path → set of member directories the workspace claims.
    /// Populated lazily on first lookup per root.
    claims: HashMap<PathBuf, HashSet<PathBuf>>,
}
```

**Lifetime**: One instance per `reconcile_design_source_tiers` call; discarded on return. No cross-scan persistence.

## Entity: Reconciliation event log (INFO + DEBUG per FR-020)

**INFO shape** (emitted once per scan):

```
reconciled N design-tier components into source-tier siblings; K standalone design-tier components emitted
```

Where `N = count of merged design-tier components` and `K = count of design-tier components with no source-tier match (standalone)`.

**DEBUG shape** (emitted per reconciled pair):

```
reconcile_design_source_tiers: matched design={design_purl} → source={source_purl} (workspace_scope={path}); transferred annotations: [mikebom:requirement-range=..., mikebom:source-manifest=...]
```

**Suppression**: standard `RUST_LOG` filter applies; DEBUG events are silent at INFO level by default.

## Emission-shape contracts (see `contracts/`)

The reconciliation pass changes what a component list looks like BEFORE it reaches the format emitters. Format emitters are responsible for translating that reconciled shape into their format-idiomatic wire form per US2 (FR-010 / FR-011 / FR-012) and FR-004 (multi-declaration property entries). See `contracts/emission-shape.md` for the byte-level shape per format.
