# Data model: native binaries inside a Python package

In-process only. Nothing persists beyond the emitted document.

## RecordOwners (new, `package_db/pip/dist_info.rs`)

These are the `RECORD` claims, held apart from the shared claim set (research R1).

| Field | Type | Meaning |
|---|---|---|
| `by_path` | `BTreeMap<PathBuf, PathBuf>` | A claimed path, in both raw and canonical form, mapped to the owning `.dist-info` directory. |
| `by_inode` (Unix) | `BTreeMap<(u64, u64), PathBuf>` | The same, keyed by `(dev, ino)`. |
| `files_by_owner` | `BTreeMap<PathBuf, BTreeSet<String>>` | For each `.dist-info`, the file names its `RECORD` lists. Used for internal-link matching (R4). |

**Rules:**
- **V1 (FR-010):** directories are read in sorted order, and the first insert wins.
- **Lookup** uses the three layers of `is_path_claimed`: raw path, canonical path, then inode.

## OwnedBinary (new, returned by `binary::read`)

There is one per binary that the shared claim set does not claim, `RecordOwners` resolves, and binary analysis recognises as ELF, Mach-O or PE.

| Field | Type | Meaning |
|---|---|---|
| `path` | `PathBuf` | The binary's path, as the walker saw it. |
| `owner` | `PathBuf` | The owning `.dist-info`. |
| `file_purl` | `Purl` | Its file-level component's PURL. Under `<x>.libs/`, the auditwheel suffix is stripped from the name (R6). |
| `links` | `BTreeSet<String>` | The PURLs of linkage components its external imports resolved to. |
| `internal_links` | `BTreeSet<String>` | The file names of other binaries the same owner owns that it links. |
| `identified` | `bool` | Whether at least one content-derived identification fired (clarification 4). |

## The attach pass (`scan_fs/binary/owned.rs`, R4)

**Input:**
- `&mut [PackageDbEntry]`;
- `&[OwnedBinary]`;
- the owner PURL by `.dist-info`, resolved from pip entries whose `source_path` is `<dist-info>/METADATA` (R2).

**Effect:**

| Entry | Change |
|---|---|
| owner `pkg:pypi` | `depends += file_purl` for each owned binary |
| binary file-level | `parent_purl = owner PURL`; `depends += links ∪ {file_purl of internal_links} ∪ {identified children}` |
| content-derived entry from `path` (version string, fingerprint, v2 match, ELF note, bundled tree, `go_binary` module) | `parent_purl = file_purl`, unless one is already set |
| cargo-auditable crate | already `parent_purl = file_purl`; `depends` edge added from the binary |

**Output:** `OwnedBinaryCoverage { analysed, identified }`, which becomes C230.

**Rules:**
- **V2:** an owned binary whose owner PURL is unresolved is analysed but neither nested nor counted (R2).
- **V3 (FR-003):** an owned binary appears exactly once. Its file-level component carries the path as its occurrence and its hash, so the file tier's `DedupeIndex` already covers it and no separate `file` component is emitted.

## Emitter shape (R5)

**Containment tree:** owner → binary → identified. The CycloneDX nested `bom-ref` is `<purl>#<parent bom-ref>`, recursively.

**Rules:**
- **V4:** the same PURL nested under two parents gets two distinct CycloneDX `bom-ref`s, SPDX 2.3 `SPDXID`s and SPDX 3 IRIs. The SPDX identifier of a nested component derives from its PURL plus its parent chain.
- **V5:** every CycloneDX `dependencies[].ref` and `dependsOn` value is a `bom-ref` present in the document. An edge to a PURL with several copies resolves to the copy under the edge's source, else to the top-level copy.

## C230 `waybill:python-owned-binaries`

The value is `{"analysed":N,"identified":M}`, with sorted keys. It is emitted when `N > 0`, and `M ≤ N`.
