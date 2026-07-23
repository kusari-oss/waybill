# Contract: `SplitMode` enum + `group_key()` extensibility

**Feature**: 219-split-modes | **Related**: FR-007, FR-003, SC-009

## Surface

The internal grouping abstraction is a single enum with a method. No trait objects, no dynamic dispatch, no lifetime gymnastics.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum SplitMode {
    Workspace,  // m215 default
    Directory,  // m219 addition
    // Future variants (v2+) plug into the same interface.
}

impl SplitMode {
    /// Return the grouping-key string for `root` under this mode.
    pub fn group_key(&self, root: &SubprojectRoot) -> String {
        match self {
            SplitMode::Workspace => root.subproject_id(),
            SplitMode::Directory => {
                let s = root.source_dir.to_string_lossy().to_string();
                if s.is_empty() { "root".to_string() } else { s }
            }
        }
    }
}

impl Default for SplitMode {
    fn default() -> Self {
        SplitMode::Workspace
    }
}
```

## Semantic contract

- `group_key(&root)` MUST be **deterministic**: same `(mode, root)` → same key across scan runs and hosts.
- `group_key(&root)` MUST return a **non-empty** string. Empty inputs (e.g., empty `source_dir` under `Directory`) resolve to a documented sentinel (`"root"` for empty-source-dir per R4).
- Distinct main-modules under the SAME grouping key merge into ONE `GroupedProjection`. Distinct keys → distinct groups.
- **No side effects**: the method is pure. No I/O, no locks, no mutation.

## Extensibility contract (US2 P2 gate)

Adding a future grouping strategy MUST require touching only:

1. **The enum variant list** (add `Ecosystem,` or similar).
2. **The `group_key` match arm** (add the match branch computing the key from `root`).
3. **The docs page's mode table** at `docs/reference/split-modes.md`.
4. **A new test scenario** in `waybill-cli/tests/split_modes.rs`.

Zero required changes to:

- CLI-flag definition (clap re-derives `ValueEnum` automatically).
- Split-manifest schema (the additive-optional `members[]` field accommodates any grouping — a member is a main-module regardless of what grouping put it there).
- `emit_split` orchestration (grouping is data-driven; the orchestrator iterates `Vec<GroupedProjection>` blind to the mode).
- Filename computation (single-member groups use m215's `<slug>.<ecosystem>` shape; multi-member groups use the m219 `<dir-slug>.multi` shape; both branches key off `members.len()`, not the mode).

**Verification (SC-009 mechanical test)**: hand-add a `#[cfg(test)] TestOnlyEcosystem` variant in the test module + the corresponding `group_key` match arm + a test scenario. Assert the build succeeds and the new scenario passes. If any of the four "zero changes" surfaces above needed modification, the extensibility contract is broken and the test fails at the compile-error stage.

## Determinism contract

For any two invocations of `group_key(&mode, &root)` with identical inputs (including root's `source_dir` post-canonicalization):
- Output IS identical bytes.
- No dependency on wall-clock time, PID, host, cwd, or environment variables.

This is load-bearing for SC-005 byte-identity: `Workspace::group_key` MUST produce the same string m215's implicit per-root grouping produced (namely `subproject_id()`), so single-member groups get filename-identical output.

## Consumer usage

```rust
// In emit_split's refactored body:
let groups: Vec<GroupedProjection> = group_roots(&roots, mode);

for group in &groups {
    let filename = if group.members.len() == 1 {
        // m215 single-member shape verbatim
        filename_for(&group.members[0], fmt, &collision_map)
    } else {
        // m219 multi-member shape (see multi-member-filename.md)
        format!("{}.multi.{}", dir_slug(&group.group_key), format_ext(fmt))
    };
    // ... emit sub-SBOM with (group.components, group.relationships)
}
```

## Anti-patterns (rejected)

- **`Box<dyn Grouper>` trait-object dispatch**: rejected — dynamic dispatch, allocation per variant, additional Cargo surface for the trait, harder for grep to find all impls. Enum-with-method is compile-time zero-cost.
- **`fn(&SubprojectRoot) -> String` function-pointer table**: rejected — loses variant identity in `Debug` output + error messages; makes clap `ValueEnum` derive impossible.
- **String-keyed strategy registry**: rejected — no compile-time enforcement that a strategy exists; runtime lookup + error handling for zero win.
