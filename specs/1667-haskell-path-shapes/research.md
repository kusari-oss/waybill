# Research: Haskell path values have the right shape

All measurements are at `main` 6084b343, over the committed goldens.

## R1. Which C200 keys hold paths, and how many

- **Decision**: relativise every path-bearing key:
  - `manifests-seen`, `manifests-read` and `generated-cabal-suppressed` (strings);
  - `manifests-unread[].file` and `manifests-partial[].file`;
  - `includes-resolved[]` and `includes-unresolved[]`, both `manifest` and `file`;
  - `manifests-dropped-by-scope` (strings, added by spec 1652).
- **Measured**:
  - 9 targets carry C200, in 27 files;
  - all 7 `hpack-*` targets carry `includes-resolved`;
  - the two `nix-closure-*` targets carry no include keys;
  - per target, C200 holds 2 to 104 paths (hs-opentelemetry has 104).
- **Rationale**: #1463 named three keys, but the include entries (spec 1649) hold the same absolute paths. Fixing three keys would still leave host paths in 7 targets.

## R2. Which root the paths are made relative to

- **Decision**: the canonical scan root, `std::fs::canonicalize(rootfs)`, falling back to `rootfs` if that fails. `HpackReadOutcome::to_document_value` takes the root as a parameter. `finalize(rootfs, …)` is the only caller and already has `rootfs`.
- **Rationale**: the readers already join C200's paths to the canonical root; spec 1652's `amend_scope_dropped` doc comment and its symlink test (`project_discovery_haskell.rs::us3_symlinked_root_c200_matches`) rely on this. Relativising against the root the paths were built from makes the prefix strip exact. This is verified at implement time, before relying on it: the symlink test must pass unchanged.
- **Alternative rejected**: relativising at emission time in each of the three format emitters. That would be three sites, and the value is built once.

## R3. A path outside the root (clarification, 2026-11-27)

- **Decision**: a lexical relative path, built only from the standard library:
  - drop the components the path and the root share;
  - add one `..` for each remaining root component;
  - append what remains of the path.

  Both paths are canonical (R2), so no `..` or symlink is left in them to make the result wrong.
- **Measured**: 0 corpus paths fall outside their root.
- **Found at implement time**: no scan can produce one. The paths in C200 are:
  - manifests, which the walker finds under the root;
  - the `file` and `manifest` of include entries, which are files already read.

  Spec 1649's include resolver (`hpack/prep.rs`, `UnresolvedReason::OutsideRoot`) refuses an include that resolves outside the root, so that file is never read. It appears only as `include`, the path as written, which is already relative.
- **Consequence**: the `..` form is defensive. A unit test proves it. The integration fixture instead asserts that an include from above the root shows up under `includes-unresolved` with reason `outside-root`, and that its `manifest` and `file` are relative.

## R4. `manifests-dropped-by-scope` after the change

- **Decision**: `scan_cmd.rs` passes `report.dropped_evidence_paths` (already relative to the scan root) without joining it to `canonical_root`. `amend_scope_dropped` then compares relative strings with relative strings.
- **Rationale**: both sides are now relative to the same root, so the symlink case becomes trivial rather than handled. The join, and its comment about symlinks, are removed.
- **Check**: `us3_symlinked_root_c200_matches` still finds 2 entries (FR-004).

## R5. One directory rule

- **Decision**: add `split::subproject_dir(rel: &Path, scan_root: &Path) -> PathBuf`. It returns the parent of `rel` when `rel`'s file name is in `is_manifest_basename`, or when `scan_root.join(rel)` is a file. Otherwise it returns `rel`. Both callers use it:
  - `source_dir_for`;
  - `project_discovery::root_dir`, which becomes a thin wrapper over `subproject_dir(&root.source_dir, scan_root)`.
- **Rationale**:
  - Keeping the list in the rule (list **or** file test) means a non-Haskell evidence path that does not exist on disk still has its parent taken, exactly as today (FR-006).
  - The file test covers `package.yaml`, `*.cabal` and any manifest nobody listed (spec FR-005).
  - Once `source_dir` is a directory, `root_dir`'s own test is a no-op. It stays as a wrapper so that any `SubprojectRoot` built elsewhere still resolves.
- **Alternative rejected**: adding `package.yaml` and `*.cabal` to `is_manifest_basename`. A `*.cabal` name can't be listed without a pattern, and the list is how #1691 arose in the first place.

## R6. What else could move

- **Split goldens**: `split_*` fixtures are Cargo, npm, Ruby and polyglot. None is Haskell, so none should change. T-measure confirms this.
- **`--project-discovery` on Haskell**: it already used `root_dir`, so its decisions don't change. C244's main-module list is built from `source_dir`; if it prints directories, it changes from `package.yaml` to the directory. To be measured: grep C244 in the goldens (none expected, since the corpus doesn't run `--project-discovery`).
- **Integration tests**: they match C200 entries by suffix (`ends_with`), so a relative path still matches. `haskell.rs:4326`'s unit test pins absolute `/r/...` strings and is updated.
