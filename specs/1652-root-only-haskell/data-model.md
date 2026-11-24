# Data model: spec 1652

Everything is in process for one scan. Nothing persists beyond the emitted document.

## `root_dir(root, scan_root) -> PathBuf` (new, `project_discovery/mod.rs`)

**What it returns:** the main module's directory, relative to the scan root.
- `root.source_dir` when `scan_root.join(source_dir)` is not a file.
- Otherwise its parent: `package.yaml` gives `""`, and `local-packages/a/a.cabal` gives `local-packages/a`.

**Who uses it:**
- `is_root_level`, in place of `source_dir`;
- `filter.rs`, where the in-scope directory set is seeded through `dir_key(&root_dir(..))`;
- matching a root against a declared member directory.

## `HaskellParseSummary.root_declared_member_dirs: Vec<String>` (new field)

**What it holds:** the package directories declared by a scan-root `cabal.project` or `stack.yaml` `packages:`.
- Each is scan-root-relative and `/`-separated, with `.` for the root. The list is sorted and unique.
- Empty when there is no root declaration, or when its entries match no discovered package (R4).
- **Never rendered.** It is not part of C200 or any annotation.

| Source | Entry forms | Resolution |
|---|---|---|
| `cabal.project` `packages:` / `optional-packages:` | `dir`, `./dir`, `dir/*` (glob), `x.cabal`, `dir/*.cabal`, `.`; whitespace- or comma-separated | Match against the directories of discovered `package.yaml` and `*.cabal` files, with `literal_separator(true)`. A `.cabal` entry resolves to its parent directory. |
| `stack.yaml` `packages:` | a YAML list of directories; absent means `[.]` | Normalise `./` and trailing `/`, and keep the entries that name a discovered package directory. |

## `ProjectDiscoveryReport` (changed)

| Field | Before | After |
|---|---|---|
| `mode` | unchanged | unchanged |
| `root_main_modules` | the count of in-scope roots | unchanged in meaning; it now includes declared roots under `root-only` |
| `workspace_members_followed` | unchanged | unchanged |
| `nested_projects_ignored` | `all_roots − in_scope_roots` (wrong, R3) | `dropped_main_modules.len()` |
| `dropped_main_modules` | (none) | **new**: `Vec<String>`, the sorted PURLs of the main modules from `enumerate_workspace_roots` that are absent from the reachable set |

The `Copy` derive is removed.

## `ProjectDiscoveryDrop` (new, carried to emission)

```rust
pub struct ProjectDiscoveryDrop {
    pub main_modules: Vec<String>, // sorted PURLs, non-empty
    pub ecosystems: Vec<String>,   // sorted unique PURL types of main_modules
}
```

- It is built from the report only when `dropped_main_modules` is non-empty, and only on the filtered branch, where C140 is `Some`.
- It lives in `ScanArtifacts.project_discovery_dropped: Option<ProjectDiscoveryDrop>`. `None` under `all`, on the #1045 fall-back, and when nothing was dropped.
- It feeds C244 (count, `main_modules`) and `ReasonCode::ProjectDiscoveryScope` (count, `ecosystems`).

## `ReasonCode::ProjectDiscoveryScope { count: usize, ecosystems: Vec<String> }` (new variant)

**Renders:** `project-discovery-scope: <count> project(s) dropped from <ecosystems joined by ", ">`.

**Pushed:**
- on the empty-set path, giving a `partial` result instead of `trivially_complete()`. If `manifest-unread` also fires there, the result stays `unknown`, carrying both codes;
- and on the classified path.

**In CycloneDX `compositions`:** `degraded_ecosystems` adds `ecosystems`.

## C200 value (changed only when a drop hits an hpack manifest)

**New key:** `manifests-dropped-by-scope`, an array of strings.
- Its entries are the `manifests-read` entries whose `pkg:hackage` main module is in the drop, sorted.
- Present only when non-empty. `manifests-read` is unchanged.

## Lifecycle within `scan_cmd`

1. The readers run. The Haskell reader fills `root_declared_member_dirs`.
2. The milestone-176 tagging runs.
3. `apply_scope_filter(components, relationships, mode, root, &declared_member_dirs)`:
   - `all`: return at once (unchanged).
   - Otherwise the in-scope roots are the root-level ones (by `root_dir`), plus, under `root-only`, those whose `root_dir` is declared.
   - If there are none, return the inputs untouched (#1045).
   - Otherwise project, follow up, filter, then compute the drop set.
4. On the filtered branch, `ProjectDiscoveryDrop` is built when the drop is non-empty, and `amend_scope_dropped` is applied to the summary's `hpack_read`.
5. Emission: C140 (unchanged), C244, the reason code, and the C200 key.
