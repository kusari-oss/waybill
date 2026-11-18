# Data model

All of it is in process for one scan.

## WorkspaceRootAnchor (R1)

The PURL of a component that meets both conditions:
- it has `waybill:component-role` = `workspace-root`;
- no relationship has it as its target.

It is appended to `ScanResult.declared_root_anchors`. `root_edges` drops it when it is the chosen root.

## Subject rung (R2)

**Input:** components and the operator override.

**Output:** `MainModule(idx)`, with C69 heuristic `workspace-root` (0.95), for the single workspace root whose first `waybill:source-files` entry contains no `/`. Otherwise, fall through to the existing ladder.

**Never applies** under `--root-name`.

## LinkDeclaration (R3, R4)

| field | meaning |
|---|---|
| `file` | path of the file holding the link: an included file is named itself |
| `directory` | the resolved, lexically normalised link directory |
| `name` | the plain library name |
| `conditions` | the open blocks at the link: the include sites' first, then the file's own; normalised as C232 |

Produced by `cmake/links.rs` for each walked file and its includes. Held in the reader's coverage, like `FetchCoverage`.

## Correlation (R4, R5)

`(LinkDeclaration, record)` pairs:
- **record:** an installed-library component (`waybill:source-mechanism` `cmake-package-version` or `pkg-config`), with a prefix from `prefix_of`;
- **match:** `directory` is inside the prefix, and `name` equals the record name (case-insensitively), or the directory holds `lib<name>.a`, `lib<name>.so`, `lib<name>.so.*` or a `.dvc` pointer to one of them. When several matching records' prefixes contain the directory, only the longest prefix's record is linked.

**Per matched record:**
- **edge:** `owner_of(file)` → record, `DependsOn`, provenance `cmake-link-declaration`. With no owner, the record is a root anchor. There is no self-edge and no duplicate.
- **scope:**
  - if every matched declaration has conditions, `Optional`, with C122 = `cmake-conditional-link` and C235 = the sorted union of conditions;
  - otherwise unchanged, with no C235;
  - an existing Test or Optional scope from another mechanism is kept, by spec 1538's merge.

## Wire rows

| row | key | shape |
|---|---|---|
| C122 (existing, new value) | `waybill:optional-derivation` | `cmake-conditional-link` |
| C69 (existing, new value) | `waybill:root-selection-heuristic` | `workspace-root`, confidence 0.95 |
| C235 (new) | `waybill:cmake-link-conditions` | per component, a sorted JSON array of strings; present iff the C122 value is `cmake-conditional-link` |
