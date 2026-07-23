# Data Model: Split-mode grouping strategies

**Feature**: 219-split-modes | **Date**: 2026-07-23

## E1 — `SplitMode` enum

New public type in `waybill-cli/src/generate/split.rs`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum SplitMode {
    /// m215 default. Group key = SubprojectRoot::subproject_id() —
    /// one group per main-module. Byte-identity contract with
    /// alpha.67 `--split` (SC-005) preserved.
    Workspace,
    /// m219 addition. Group key = canonicalized
    /// SubprojectRoot::source_dir. All main-modules whose source
    /// dirs match collapse into ONE group → ONE sub-SBOM.
    Directory,
}
```

**Validation rules**:
- `Copy` implementation valid because enum is 1 byte (2 variants).
- clap-parsed values are `"workspace"` / `"directory"` (lowercase per `rename_all`).
- Invalid parse (e.g., `--split=nonexistent`) → clap emits stderr error listing accepted values.

**Field ordering**: Workspace first (default variant); Directory second (extensibility variant).

## E2 — `SplitMode::group_key` method

```rust
impl SplitMode {
    /// Return the grouping-key string for `root` under this mode.
    pub fn group_key(&self, root: &SubprojectRoot) -> String {
        match self {
            SplitMode::Workspace => root.subproject_id(),
            SplitMode::Directory => root.source_dir.to_string_lossy().to_string(),
        }
    }
}
```

**Validation rules**:
- Return value is always non-empty (empty source_dir → literal `"root"` sentinel per R4).
- Deterministic per call: same input → same output.
- No side effects.

**Extensibility contract**: adding a future variant requires ONLY: (1) enum variant addition, (2) match-arm addition. Zero other code changes.

## E3 — `SplitMode::Default`

```rust
impl Default for SplitMode {
    fn default() -> Self {
        SplitMode::Workspace
    }
}
```

Consumed by `args.split.unwrap_or_default()` when the clap `default_missing_value = "workspace"` doesn't fire (defense-in-depth).

## E4 — `SplitMember` struct

New public type in `waybill-cli/src/generate/split_manifest.rs`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitMember {
    /// PURL string of one contributing main-module in a multi-member
    /// group. Purl-spec conformant.
    pub purl: String,
    /// Source directory of that main-module (relative to scan root).
    /// For multi-member groups under --split=directory, every member's
    /// source_dir is IDENTICAL to the group's source_dir — but the
    /// field is preserved per-member for consistency and future
    /// non-directory grouping modes (e.g., --split=ecosystem might
    /// group members from different dirs).
    pub source_dir: String,
}
```

**Validation rules**:
- `purl` MUST parse via `waybill_common::types::purl::Purl::new` (implicit — waybill upstream produces only valid PURLs; deserialize side trusts input).
- `source_dir` MAY be empty string when the main-module IS the scan root.
- Cheap to construct + serialize.

## E5 — `SplitEntry.members` additive-optional field

Extends the m215 `SplitEntry` struct at `waybill-cli/src/generate/split_manifest.rs:32`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitEntry {
    pub subproject_id: String,
    pub root_purl: String,
    pub source_dir: String,
    pub component_count: u64,
    pub shared_deps_count: u64,
    pub files: BTreeMap<String, String>,
    /// m219 additive-optional field. OMITTED when the group covers
    /// exactly one main-module (SC-005 byte-identity for m215 wire
    /// shape). PRESENT (sorted lex by `purl`) when the group covers
    /// ≥2 members (only possible under --split=directory today).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub members: Option<Vec<SplitMember>>,
}
```

**Validation rules**:
- `members` MUST be `None` iff the group has exactly 1 main-module contributing (SC-005 gate). Enforced by the `emit_split` refactor at population time.
- When `Some(vec)`, `vec.len() >= 2` (a 1-element vec would violate the SC-005 wire-shape contract; emit-time check forbids it).
- When `Some(vec)`, entries are sorted lex by `purl` for byte-identity across scan runs.
- Deserialize: absence of field → `None` (via `#[serde(default)]`). m215 payloads round-trip unchanged.

## E6 — `GroupedProjection` internal type

New private type in `waybill-cli/src/generate/split.rs`. Not exported — CLI layer and split-manifest layer both work with `Vec<GroupedProjection>` as the pipeline currency.

```rust
#[derive(Debug)]
pub(crate) struct GroupedProjection {
    /// Grouping key string from `SplitMode::group_key`. For
    /// --split=workspace, this equals `members[0].subproject_id()`.
    /// For --split=directory, this equals the canonicalized
    /// source_dir string.
    pub group_key: String,
    /// Every SubprojectRoot contributing to this group. Length ≥ 1.
    /// Sorted lex by `purl_string` for byte-identity.
    pub members: Vec<SubprojectRoot>,
    /// Merged components — union of every member's per-BFS-projection
    /// components. Deduplicated by PURL (last-write-wins on tie;
    /// matches m215 intra-projection dedup).
    pub components: Vec<ResolvedComponent>,
    /// Merged relationships — union of every member's per-BFS-projection
    /// relationships. Deduplicated by (from, to, kind) tuple.
    pub relationships: Vec<Relationship>,
    /// Count of THIS group's components that also appear in ≥1
    /// sibling GroupedProjection. Populated post-hoc by the shared-
    /// deps computation (same shape as m215's SplitProjection).
    pub shared_deps_count: usize,
}
```

**Validation rules**:
- `members.len() >= 1` (by construction — group_roots only emits groups with ≥1 member).
- `group_key` non-empty (per E2 constraint).
- Component dedup key: `component.purl.as_str().to_string()`. Last-write-wins.
- Relationship dedup key: `(from: String, to: String, kind: RelationshipType)`. Set-based; first-observed insertion order preserved.

## E7 — Multi-member group's `subproject_id` derivation

Not a Rust type — a computation rule.

For a `GroupedProjection` where `members.len() == 1`:
- `subproject_id` = `members[0].subproject_id()` (matches m215 verbatim).
- `root_purl` = `members[0].purl_string.clone()`.

For a `GroupedProjection` where `members.len() >= 2`:
- `subproject_id` = `<dir-slug>.multi` where `<dir-slug>` is derived per R4 (canonicalized source_dir, path-separator → `-`, char-safety pass, truncate, lowercase; empty source_dir → literal `"root"`).
- `root_purl` = `pkg:generic/<dir-slug>@0.0.0-unknown` — synthetic per R6; consumers who need the actual member PURLs read `members[]`.

## E8 — CLI flag field

Rewrite of `waybill-cli/src/cli/scan_cmd.rs:448`:

```rust
/// Milestone 215 — split monorepo SBOM into per-workspace-member
/// sub-SBOMs.
/// Milestone 219 — accepts an optional value:
///   `--split` (bare) OR `--split=workspace` → per-main-module
///     grouping (m215 default; byte-identity preserved).
///   `--split=directory` → group all main-modules whose canonicalized
///     source dirs match into ONE sub-SBOM per dir. Useful for
///     polyglot repos where Cargo + package.json coexist.
///
/// Requires `--output-dir <dir>`; incompatible with `--output`.
/// On single-package projects with no workspace boundaries, falls back
/// to one SBOM with a WARN log (FR-009). See
/// `docs/reference/split-modes.md` for the mode table + worked
/// examples.
#[arg(
    long,
    value_enum,
    num_args = 0..=1,
    default_missing_value = "workspace",
    require_equals = true,
    conflicts_with = "output"
)]
pub split: Option<crate::generate::split::SplitMode>,
```

**Validation rules**:
- `require_equals = true`: `--split directory` (space-separated) is REJECTED; `--split=directory` (equals-separated) is ACCEPTED. Prevents next-positional-arg-consumption footgun.
- `num_args = 0..=1`: allows both bare `--split` AND `--split=<value>`.
- `default_missing_value = "workspace"`: when bare, resolves to `Some(Workspace)`.
- `conflicts_with = "output"`: m215 constraint preserved.

**Default value** in the `Default` impl for `ScanArgs`: `split: None`.

## State Transitions

None — all data is scan-lifetime in-process. No state machines, no persistence.

## Data Volume Assumptions

- **Largest realistic monorepo**: ~50 main-modules (m215's practical upper bound; Kubernetes-shaped repos with N controllers).
- **Grouping table**: `BTreeMap<String, Vec<SubprojectRoot>>` of ~≤ 50 keys × avg ~1 member each. Trivial memory (<50 KB).
- **Multi-member groups per scan**: typical polyglot repo has 1-5 dirs with multi-ecosystem coexistence. Group count ≤ member count always.
- **Multi-member `members[]` vec length**: typical is 2 (npm + go, or cargo + npm); rare is 3+ (npm + cargo + go in one dir). Vec allocation is cheap.
