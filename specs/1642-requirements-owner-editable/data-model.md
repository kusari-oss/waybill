# Data Model: requirements ownership and anchor immunity

All state is in-process for one scan. Nothing new reaches the wire except through existing fields (research R8).

## Merged main module (R2)

The entry the pip main-module pass leaves for a PURL that a lockfile reader also emitted.

| Field | Before (`main`) | After |
|---|---|---|
| `source_path` | the lockfile (`tool/uv.lock`) | the manifest's directory, from the synthesized main module |
| `extra_source_paths` | the lockfile entry's extras | the lockfile path first, then the existing extras |
| `evidence.source_file_paths` (derived) | `[lockfile, …]` | `[manifest dir, lockfile, …]` |
| C18 `waybill:source-files` | `["tool/uv.lock"]` | `["tool/devtool", "tool/uv.lock"]` |
| C120 `waybill:workspace-member` | `["tool"]` | `["tool/devtool"]` |

- **Applies when** the existing entry's `sbom_tier` is not `deployed`. An installed entry is unchanged.
- **Editable cross-check:** a uv.lock entry carries its `source.editable` path, relative to the lock, on the parse result. The merge compares it, resolved against the lock's directory, with the manifest's directory. On disagreement, it logs at debug level and the manifest wins. The path is not emitted.

## Owner candidate (R3)

| Field | Meaning |
|---|---|
| directory | relative to the scan root, from `derive_workspace_root(first source path)` |
| rank | `RANK_PYPI_MAIN_MODULE` < other main module < CMake project < submodule (unchanged) |
| non_runtime | the candidate's `lifecycle_scope` is non-runtime |
| purl | the tie-break of last resort |

**Order:** `(rank, non_runtime, purl)`. A tie on `(rank, non_runtime)` decided by the PURL emits a `debug` event on `waybill::owner_index`.

## Anchor seed (R5)

- **Source:** the reader root anchors `scan_cmd` installs, which are the declared root anchors (spec 1537 R4) and the top-level git submodules.
- **Filter:** `is_runtime_seed_candidate`, so the anchor's own scope is runtime, and it is not under a listed CI or docs dot-directory.
- **Use:** added to spec 1616's `added_seeds` for the immunity search only. `seeds` (graph completeness, FR-012) is unchanged.
- **Summary:** `PropagationSummary` gains `immune_by_root_anchors`, counted apart from `immune_by_added_seeds`.

## Census events (R6, log-only commit)

On target `waybill::owner_census`, one JSON object per event:

| event | fields |
|---|---|
| `owner_candidates` | directory, candidates `[{purl, rank, non_runtime, first_path}]`, deciding step, owner today, owner after R2 and R3 |
| `owner_change` | kind (`requirements` / `cmake-fetch` / `cmake-search`), component PURL, file, owner today, owner after |
| `merge_source` | PURL, existing tier, existing source path, manifest directory, editable path (or null), agrees (bool) |
| `dev_only_main_module` | PURL, directory today and after R2, is root candidate, is owner candidate, is added seed |
| `anchor_immunity` | PURL, ecosystem, scope today, anchor kind (`declared` / `submodule`), would be immune |
