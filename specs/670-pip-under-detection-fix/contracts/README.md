# Reader contracts

Each contract file describes the input surface, output surface, and error behavior of one of the new readers introduced by milestone 670.

All readers share the trait shape:

```rust
pub(crate) fn read(
    path: &Path,
    walker_context: &SharedWalkerContext,
) -> Result<Vec<PackageDbEntry>, anyhow::Error>;
```

Consistent with the m664 shared-walker registry convention. Individual readers are wired into `waybill-cli/src/scan_fs/package_db/pip/mod.rs::dispatch`.

## Files

| Contract | Format | Covers |
|----------|--------|--------|
| [pyproject_toml.md](./pyproject_toml.md) | TOML | PEP 621, PEP 735, Poetry-legacy |
| [requirements_txt.md](./requirements_txt.md) | Line-based (PEP 508) | Recursive discovery, scope-heuristic |
| [setup_py_static.md](./setup_py_static.md) | Static regex over Python source | `install_requires` literal-list extraction |
| [lockfiles.md](./lockfiles.md) | TOML + JSON | uv.lock, poetry.lock, pdm.lock, Pipfile.lock |

## Emission contract (shared)

All readers emit `PackageDbEntry` records with:

- `purl: Purl` — always well-formed `pkg:pypi/<name>@<version>` (or `@unresolved`)
- `source_file_paths: Vec<PathBuf>` — at least one path pointing to the file the reader parsed
- `lifecycle_scope: LifecycleScope` — `Main` or `Optional { scope_name }`
- `extra_annotations: Vec<Annotation>` — per-reader specifics documented in each contract

## Error posture

Per FR-016: all readers return `Ok(vec![])` on parse failure of the file they were invoked on, after emitting a `tracing::warn!` with the file path and error. They never propagate `Err` upward (that would fail the entire scan; instead, we degrade gracefully). Return `Err` only for internal invariant violations (should not happen in practice; test coverage catches them).

## Reconciliation contract

Multiple readers may produce the same `pkg:pypi/<name>@<version>` PURL. The m191 reconciler at `waybill-cli/src/resolve/reconciler.rs` collapses them into one component with:
- Union of `source_file_paths`
- Union of `extra_annotations` (deduplicated by key)
- Lockfile-sourced version wins over manifest-declared constraint (FR-012)
- `LifecycleScope::Main` beats `LifecycleScope::Optional { .. }` on collision
