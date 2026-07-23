# Research: Split-mode grouping strategies

**Feature**: 219-split-modes | **Date**: 2026-07-23

## R1 — CLI-flag shape: `clap::ValueEnum` with `default_missing_value`

**Decision**: Change `pub split: bool` → `pub split: Option<SplitMode>` where `SplitMode: ValueEnum { Workspace, Directory }`. Use `#[arg(long, value_enum, num_args = 0..=1, default_missing_value = "workspace", require_equals = true)]` on the field. This gives operators three valid invocations:
- `--split` (bare) → `Some(SplitMode::Workspace)` (via `default_missing_value`).
- `--split=workspace` → `Some(SplitMode::Workspace)` (explicit).
- `--split=directory` → `Some(SplitMode::Directory)`.
- (flag absent) → `None` (no split).

**Rationale**: `default_missing_value` is the clap idiom for "flag present without a value = use this value". The `require_equals = true` prevents the next positional arg from being silently consumed as the mode value (matches waybill's `--offline` convention at `scan_cmd.rs:~192`). Rejected: (a) two separate flags `--split` + `--split-mode=<x>` — pollutes the CLI surface + adds validation complexity; (b) `Vec<String>` with manual parsing — reinvents `ValueEnum`'s error-message goodness for zero win.

**Alternatives considered**:
- `#[arg(long, action = ArgAction::SetTrue)]` with a companion `--split-mode` env var: rejected — env vars are second-class CLI signals for waybill (m173 warm-go-cache uses env only as an alias to the CLI flag, not as the sole vehicle).
- `EnumFrom<String>` on a custom type: `ValueEnum` derive is strictly better — auto-generates `--help` output listing accepted values.

## R2 — `SplitMode` enum shape

**Decision**: 

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lowercase")]  // Workspace → "workspace", Directory → "directory"
pub enum SplitMode {
    /// m215 default. Group key = SubprojectRoot::subproject_id() — one group per main-module.
    Workspace,
    /// m219 addition. Group key = canonicalized SubprojectRoot::source_dir.
    Directory,
}

impl SplitMode {
    /// Return the grouping-key string for `root` under this mode.
    pub fn group_key(&self, root: &SubprojectRoot) -> String {
        match self {
            SplitMode::Workspace => root.subproject_id(),
            SplitMode::Directory => root.source_dir.to_string_lossy().to_string(),
        }
    }
}

impl Default for SplitMode {
    fn default() -> Self {
        SplitMode::Workspace
    }
}
```

**Rationale**: `Copy` because `SplitMode` is a 1-byte enum (2 variants); passing by value everywhere. `#[value(rename_all = "lowercase")]` gives operators `--split=workspace` / `--split=directory` (not `--split=Workspace`). `group_key` returns `String` (not `&str`) because `Directory`'s key is derived from `PathBuf::to_string_lossy()` which needs a new allocation anyway.

**Extensibility contract** (US2 P2 gate): adding a future variant (`Ecosystem`, `Owner`, `Custom`) requires touching only:
1. The enum's variant list (`Ecosystem,`).
2. The `group_key` match arm (`SplitMode::Ecosystem => root.ecosystem.clone(),`).
3. The docs page's mode table (add row).
4. A new test scenario.

Zero changes to CLI-flag definition (clap re-derives), split-manifest schema (already flexible), or `emit_split` orchestration (grouping is data-driven).

## R3 — Documentation surface

**Decision**: Extend `docs/reference/split-modes.md` (NEW page) and link from README's SBOM interpretation section. Cross-reference from `docs/user-guide/cli-reference.md#split` (if it exists) or from a similar CLI-reference doc site — surveyed at Phase 1 write time.

**Rationale**: `docs/reference/*.md` is the established waybill pattern for feature-specific docs (m134 divergent-purl, m216 package-shape, m217 go-toolchain, m218 cross-ecosystem-edges all live under `docs/reference/`). CLI-reference sections are terse; split-mode's extensibility contract + worked examples need a dedicated page. m218 sets the template — 6 sections (flag description; when-to-use; interpretation; decision tree; extensibility contract; worked example).

**Content sections** (~150-200 lines):
1. What the modes mean (workspace vs directory).
2. When to choose which (decision table: monorepo shape → recommended mode).
3. Worked example for each (fixture-based; jq/`ls` snippets).
4. `split-manifest.json` schema evolution (additive-optional `members[]`).
5. Filename convention for multi-member groups (`<dir-slug>.multi.<format-ext>`).
6. Extensibility contract for contributors (the 4-file touch list from R2).

## R4 — `<dir-slug>` derivation for multi-member filenames

**Decision**: `<dir-slug>` = canonicalized `SubprojectRoot::source_dir` with:
1. Path separator (`/` or `\`) → `-`.
2. Leading `-` (from absolute-path leading `/`) → stripped.
3. Character-safety pass matching m215's `subject_slug` at `split.rs:405-431` (backslash, colon, glob, wildcards, quotes, angle brackets, pipe, whitespace stripped).
4. Truncate to 100 bytes.
5. Lowercase.

**Empty source_dir** (scan root itself): use the literal string `"root"` as the slug. Filename: `root.multi.cdx.json`. Rationale: `<empty>.multi.cdx.json` would produce `.multi.cdx.json` (leading dot = hidden file on POSIX). `root` is the least surprising sentinel.

**Rationale**: Reuses m215's proven path-slug logic (same char-safety set + truncation + lowercase). Extends it to strip the leading `-` that path-slugification of absolute paths would produce.

**Collision-safety**: two neighbor groups can't share a `<dir-slug>` under directory mode by construction — `group_key` IS the source_dir; distinct source_dirs → distinct group_keys → distinct slugs. If two source_dirs canonicalize to the same slug after char-substitution (e.g., `services/api` and `services-api` both slugify to `services-api`), the collision-map machinery from `build_collision_map` at `split.rs:491` handles the disambiguation via `-<sha8-hex>` suffix.

## R5 — `SplitEntry.members` additive-optional serde shape

**Decision**: 

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitMember {
    pub purl: String,
    pub source_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitEntry {
    pub subproject_id: String,
    pub root_purl: String,
    pub source_dir: String,
    pub component_count: u64,
    pub shared_deps_count: u64,
    pub files: BTreeMap<String, String>,
    /// NEW m219 additive-optional field. OMITTED (via
    /// `skip_serializing_if`) when the group covers exactly one
    /// main-module — preserves m215 wire-shape byte-identity per
    /// SC-005. PRESENT (sorted lex by `purl`) when the group covers
    /// ≥2 members (only possible under `--split=directory` today).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub members: Option<Vec<SplitMember>>,
}
```

**Rationale**: `#[serde(skip_serializing_if = "Option::is_none")]` is the standard serde idiom for "additive optional field that doesn't appear when None". `default` on the deserialize side handles the m215 JSON payloads that don't have the field. Sorted-lex by `purl` on emit for byte-identity (BTreeMap of members would work too but Vec is easier to preserve order-per-scan-run + is cheaper to serialize).

**When to populate**: `group_roots` collects the source `Vec<SubprojectRoot>` per group. If group.len() == 1, `members = None`. If group.len() >= 2, `members = Some(sorted-vec)`.

**Wire shape (multi-member example)**:

```json
{
  "subproject_id": "services-api.multi",
  "root_purl": "pkg:generic/services-api@0.0.0-unknown",
  "source_dir": "services/api",
  "component_count": 123,
  "shared_deps_count": 5,
  "files": {"cyclonedx-json": "services-api.multi.cdx.json"},
  "members": [
    {"purl": "pkg:cargo/api@0.1.0", "source_dir": "services/api"},
    {"purl": "pkg:npm/api@0.1.0", "source_dir": "services/api"}
  ]
}
```

**Wire shape (single-member — unchanged from m215)**:

```json
{
  "subproject_id": "libsafe.cargo",
  "root_purl": "pkg:cargo/libsafe@0.1.0",
  "source_dir": "crates/libsafe",
  "component_count": 42,
  "shared_deps_count": 3,
  "files": {"cyclonedx-json": "libsafe.cargo.cdx.json"}
}
```

No `members` field. Byte-identical to m215 alpha.67 output.

## R6 — `subproject_id` for multi-member groups

**Decision**: For a multi-member group under `--split=directory`, the manifest's `subproject_id` = `<dir-slug>.multi` (matches the emitted filename's slug+marker portion, minus the format-ext). The `root_purl` in this case uses a synthetic `pkg:generic/<dir-slug>@0.0.0-unknown` — because the group has no single canonical root PURL (it has N members' PURLs, listed in `members[]`).

**Rationale**: `subproject_id` is a stable identifier consumers use to correlate manifest entries with sub-SBOM files (the `files` map values). Making it `<dir-slug>.multi` gives one-to-one correspondence with the filename base. `root_purl` = synthetic-`pkg:generic/` mirrors the m216 pattern for source-tree apps with no upstream identity — consumers who parse `root_purl` still get a purl-spec-conformant string; consumers who need the ACTUAL member PURLs read `members[]`.

**Alternatives considered**:
- Set `subproject_id` to the FIRST member's `subproject_id`: rejected — ambiguous when member ordering shifts across scan runs (BTreeMap-derived ordering is stable, but the semantic of "first member wins" is opaque).
- Concatenate member PURLs into a delimited string: rejected — unreadable at manifest-inspect time.
- Omit `subproject_id` entirely: rejected — breaks m215 consumers who key on it.

## R7 — `GroupedProjection` internal type

**Decision**:

```rust
#[derive(Debug)]
pub(crate) struct GroupedProjection {
    /// The grouping key string (from `SplitMode::group_key`).
    pub group_key: String,
    /// Every SubprojectRoot contributing to this group. Length ≥ 1.
    /// Sorted lex by `purl_string` for byte-identity.
    pub members: Vec<SubprojectRoot>,
    /// Merged components — union of every member's per-projection
    /// components. Deduplicated by PURL (last wins; matches m215's
    /// intra-projection dedup).
    pub components: Vec<ResolvedComponent>,
    /// Merged relationships — union of every member's per-projection
    /// relationships. Deduplicated by (from, to, kind) tuple.
    pub relationships: Vec<Relationship>,
    /// Count of THIS group's components that also appear in ≥1
    /// sibling GroupedProjection. Populated post-hoc.
    pub shared_deps_count: usize,
}
```

**Rationale**: Structurally parallel to `SplitProjection` (m215) so the downstream emit code doesn't care whether it's emitting a single-member or multi-member group. `group_key` doubles as the manifest's `subproject_id` for multi-member groups (with `.multi` appended) and as the filename slug base.

**Building it**: 
1. `group_roots(roots: &[SubprojectRoot], mode: SplitMode) -> Vec<GroupedProjection>` — group by `mode.group_key(root)`, sort members within each group, return.
2. For each group, call `project_for_root` per member, then merge into the group's aggregate. Dedup rules: PURL-uniqueness on components; (from, to, kind)-uniqueness on relationships.

## R8 — `emit_split` refactor shape

**Decision**: The current `emit_split` signature:

```rust
pub(crate) fn emit_split(
    base_artifacts, formats, registry, output_dir, created, waybill_version, scan_root
) -> anyhow::Result<bool>
```

Gains ONE new parameter, `mode: SplitMode`:

```rust
pub(crate) fn emit_split(
    base_artifacts, formats, registry, output_dir, created, waybill_version, scan_root,
    mode: SplitMode,  // NEW
) -> anyhow::Result<bool>
```

Body refactor:
1. `enumerate_workspace_roots(...)` — unchanged.
2. `group_roots(&roots, mode) -> Vec<GroupedProjection>` — NEW.
3. For each `GroupedProjection`, run BFS per member + merge → populate `components`/`relationships`.
4. `compute_shared_deps(&mut projections)` — refactor to work on `&mut [GroupedProjection]` (interface parallel; same aggregate logic).
5. Emit loop iterates over groups; filename picks between `<slug>.<ecosystem>.<ext>` (single-member) and `<dir-slug>.multi.<ext>` (multi-member).
6. Manifest population: `SplitEntry.members = if group.members.len() >= 2 { Some(sorted-vec) } else { None }`.

**FR-010 INFO log**: existing `--split emit: fan-out starting` log gains a `mode` field, and a new `--split emit: complete` log at function-exit emits `mode=<mode> groups=<N> total_main_modules=<M>`.

**Caller update**: `scan_cmd.rs::run_scan` passes `args.split.unwrap_or_default()` (or handles the `None` case as "no split" before entering the branch). The current `if args.split` becomes `if let Some(mode) = args.split`.

## R9 — SC-005 byte-identity strategy

**Decision**: The SC-005 gate — bare `--split` + `--split=workspace` byte-identical to alpha.67 — is enforced mechanically by:
1. **`Workspace` mode's `group_key` returns `subproject_id()`** (same as m215's implicit per-root grouping). Every m215 root becomes a single-member group. Members-list stays None (omitted). Filename picks the single-member branch (m215's `<slug>.<ecosystem>.<ext>`).
2. **No changes to `build_collision_map`, `subject_slug`, `filename_for`'s single-member branch, `SplitEntry.files`, `SplitEntry.subproject_id`, or any other m215 emission path** for the single-member case.
3. **Existing m215 tests (`transitive_parity_gem`-style, split-mode fixture tests) MUST pass unchanged.** No golden regen.

**Verification**: run the entire `waybill-cli/tests/split_*.rs` suite with the m219 branch; assert every test passes. Zero test-file edits allowed as part of this milestone (except adding NEW test files under `split_modes.rs`).

## R10 — Multi-member merge dedup rules

**Decision**: When merging N member projections into ONE `GroupedProjection.components`:
1. Component dedup key: `component.purl.as_str().to_string()`. Last-write-wins on tie (matches m215 `intra-projection` dedup at `emit_split:project_for_root`).
2. Relationship dedup key: `(from: String, to: String, kind: RelationshipType)` tuple. Set-based (BTreeSet); order preserved by the first-observed insertion.
3. Main-module role: within a merged group, ≥1 member may retain the `waybill:component-role: main-module` annotation. Multiple main-modules in a single sub-SBOM is EXPECTED under directory mode (that's the whole point). No demotion happens at merge time. (m215's demotion — see `project_for_root:287-289` — is per-BFS-projection and stays that way; the merge unions post-demotion component sets.)

**Rationale**: Same rules m215 uses within a single projection, extended to the union of N. The main-module preservation is the observable behavioral delta — consumers reading a directory-mode sub-SBOM will see N `waybill:component-role: main-module` annotations, one per member. This is the semantically correct signal that the SBOM represents a merged group.
