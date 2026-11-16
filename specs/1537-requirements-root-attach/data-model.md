# Data model: declared requirements attachment

In process only. Nothing persists beyond the emitted document.

## OwnerIndex (new, `scan_fs/declared_attach.rs`)

Maps each directory to the project that owns the files in it (research R3).

| Field | Type | Meaning |
|---|---|---|
| `by_dir` | `BTreeMap<PathBuf, String>` | A directory, canonicalised, mapped to its owner PURL. One entry per directory, after precedence. |

**Built from:**
- **Main modules:** components whose `waybill:component-role` is `main-module`. The directory is the parent of `source_file_paths[0]`.
- **Submodules:** the git-submodule reader's map from directory to PURL, carried in `DbScanResult.diagnostics`. This is not a wire field.

**Precedence within one directory (V1):** a `pkg:pypi` main module, then any other main module with the lowest PURL first, then a submodule.

**The walk stops below the scan root.** The scan root's own project is never returned as an owner, so a requirements file at the scan root is a declared root anchor (FR-002), even beside a main module.

## DeclaredAttachment (new, the pass's output)

| Field | Type | Meaning |
|---|---|---|
| `edges` | `Vec<Relationship>` | `owner → component`, of type `DependsOn`. Typed by `apply_lifecycle_scope_to_edges` (FR-010). That pass runs in `scan_fs` and again in `scan_cmd` after scope propagation, so the edge is typed wherever the pass sits (mutation M10). |
| `root_anchors` | `BTreeSet<String>` | PURLs declared at the scan root, or with no owner below it (FR-002, FR-003). |

**Rules:**
- **V2:** one candidate per declaring file. A package declared in N files can produce up to N edges and at most one root anchor.
- **V3:** no self-edge, and no edge that already exists.
- **V4:** only `pkg:pypi` components whose declaring file's basename matches `requirements*.txt`.

## root_edges (new, `generate/root_edges.rs`)

**Input:**
- `components: &[ResolvedComponent]`;
- `relationships: &[Relationship]`;
- `root_ref: &str`;
- `anchors: &[String]`, which is supplement ∪ reader ∪ declared-root.

**Output:** `BTreeSet<String>`, the root's direct targets.

```
gate_open = no relationship from root_ref whose type is a dependency type
            and whose target is not a lockfile-resolve component
targets   = { r.to | r.from == root_ref, r is a dependency edge }
          ∪ { a ∈ anchors | a is a component }
          ∪ (gate_open ? { c | c.parent_purl is None, nothing targets c, c ≠ root_ref } : ∅)
```

**Rules:**
- **V5:** anchors are added after the gate is decided, and never change it.
- **V6:** the same function serves CycloneDX, SPDX 2.3, SPDX 3 and the completeness classifier. Each format renders the edges itself; SPDX uses `synthesized_root_edge` for scope-typed spellings.

## ScanResult additions (`scan_fs/mod.rs`)

| Field | Type | Meaning |
|---|---|---|
| `declared_root_anchors` | `Vec<String>` | From the pass. `scan_cmd` adds them to the reader anchors it installs. |

## pip merge change (`pip/mod.rs`)

`merge_without_override`: when an addition's PURL is already present, the addition's `source_path` is appended to the kept entry's `extra_source_paths`, unless it is already there. The entry itself is still not replaced (research R5).
