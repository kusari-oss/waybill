# Contract: the scope filter after spec 1652

`apply_scope_filter(components, relationships, mode, scan_root, declared_member_dirs)`

## Mode `all`

Returns its inputs untouched, before reading any other argument. The output is byte-identical to `main` (FR-007, SC-004).

## Modes `root-only` and `strict`

1. **Roots.** Every main module from `enumerate_workspace_roots`, unchanged.
2. **In scope:**
   - **Root-level:** a root whose `root_dir` is the scan root. This holds whatever the manifest's file name.
   - **Declared** (`root-only` only): a root whose `root_dir` key is in `declared_member_dirs`.
   - **`strict`:** after root-level, milestone 201's workspace-root narrowing (Step 1b), unchanged.
3. **Fall-back (#1045).** No in-scope root means return the inputs untouched, with an empty drop. No C140, C244, reason code or C200 key is emitted.
4. **Projection.**
   - Each in-scope root is projected through dependency edges with `split::project_for_root`, unchanged. The BFS crosses main modules, so a nested main module that the root depends on, directly or through a chain, is reached (FR-002).
   - This holds under both modes.
5. **Follow-up.** The directory follow-up and fixpoint run as today. The in-scope directories are seeded from `root_dir`, not `source_dir`.
   - When declared roots were added (`root-only`), `.` is also seeded: the declaration is a manifest at the scan root (T032).
   - **Unattributable components are kept** (T033, both modes). These are not main modules, carry no `waybill:workspace-member`, and are the endpoint of no relationship.
6. **Filter.** Components and relationships are kept when reached. Order is preserved.
7. **Drop.**
   - `dropped_main_modules` is the sorted PURLs of the roots from step 1 that are not reached.
   - `nested_projects_ignored` is its length.

## Invariants (tested)

- **I1:** under `all`, the output equals the input.
- **I2:** every dropped PURL is a main module in the input and absent from the output.
- **I3:** every main module in the input is either in the output or in `dropped_main_modules`, never both.
- **I4:** under `strict`, `declared_member_dirs` has no effect. Calling with an empty slice gives the same output.
- **I5:** declared directories that match no root leave the result as if none were passed.
- **I6:** `--split` (`split.rs`) is not modified. A `--split` scan of every fixture is byte-identical to `main`.

## The fixture outcomes (SC-001, SC-002)

These are acceptance targets, not measurements.

| Tree | `root-only` keeps | drops (C244) | `strict` |
|---|---|---|---|
| f1 (root `package.json`, nested pair) | root | `a`, `b` (2) | same |
| f2 (root `package.yaml` → `a` → `c`, plus `d`) | root, `a`, `c`, `base` | `d` (1) | same |
| f3 (root `cabal.project` `local-packages/*`, plus unlisted `tools/e`) | `a`, `b`, `base`, `text` | `e` (1) | falls back (no root main module) |
| f4 (f3 plus a root `package.json`) | root, `a`, `b`, `base`, `text` | `e` (1) | root only; drops `a`, `b`, `e` (3) |
| f5 (root `stack.yaml` with `a`, `b`, plus unlisted `tools/e`) | `a`, `b`, `base`, `text` | `e` (1) | falls back |
| f6 (npm root → nested npm by path) | everything | none; no C244 | same |
