# Phase 1 Data Model: File-tier surfacing for source-heavy trees

**Feature**: 671-file-tier-cpython
**Date**: 2026-09-01

## Overview

Purely in-memory extensions to existing m133 file-tier plumbing. No persistence, no new database, no wire-format changes beyond one new annotation. Matches every ecosystem-reader milestone posture since m002.

## Entities

### `SourceShape` (new enum)

Closed enum with one variant per source-code extension in the FR-002 allowlist.

**Location**: `waybill-cli/src/scan_fs/file_tier/source_shape.rs` (new file)

**Variants** (21):
- `Py`, `Pyi` — Python source + stub
- `C`, `Cc`, `Cpp`, `Cxx` — C / C++ implementation
- `H`, `Hh`, `Hpp` — C / C++ headers
- `Rs` — Rust
- `Go` — Go
- `Java`, `Kt` — JVM
- `Js`, `Ts` — JavaScript / TypeScript
- `Rb` — Ruby
- `Php` — PHP
- `Cs` — C#
- `Swift`, `M`, `Mm` — Apple platforms

**Derives**: `Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`.

**Methods**:
- `SourceShape::from_extension(ext: &str) -> Option<Self>` — case-insensitive lookup
- `SourceShape::as_str(&self) -> &'static str` — returns lowercase extension without the dot (e.g., `"py"`, `"cpp"`)

**Invariants**:
- Every variant round-trips through `from_extension(v.as_str())` back to itself
- The complete `as_str` set matches the 21 extensions in the FR-002 spec
- Ordering is stable (derived from declaration order via `Ord`) so `SourceShapeSet` iteration yields deterministic annotation values

### `SourceShapeSet` (new type alias)

```rust
pub(crate) type SourceShapeSet = std::collections::BTreeSet<SourceShape>;
```

Sorted, deduplicated set. Empty set is INVALID (parser rejects empty `--file-inventory-source-shapes=` value via FR-009 loud-fail).

### `SourceShapeParseError` (new error enum)

**Location**: `waybill-cli/src/scan_fs/file_tier/source_shape.rs`

**Variants**:
- `UnknownExtension { actual: String }` — operator named an extension NOT in the FR-002 allowlist. Error message lists all 21 accepted extensions.
- `Empty` — operator passed `--file-inventory-source-shapes=` (empty value)
- `DuplicateExtension { actual: String }` — operator passed the same extension twice. Non-fatal candidate; probably just dedup silently. Design decision: **dedup silently** to match how `clap` handles repeated `--exclude` args. Retained as an error variant for future strictness.

**Derives**: `Debug, thiserror::Error`

### `FileInventoryMode` (existing enum, extended)

**Location**: `waybill-cli/src/scan_fs/file_tier/mod.rs:292` (existing)

**Existing variants** (unchanged):
- `Off`
- `Orphan` (default)
- `Full`

**New variant** (m671):
- `SourceTree { restriction: Option<SourceShapeSet> }` — activates the new mode. `restriction = None` → all 21 shapes eligible. `restriction = Some(set)` → only shapes in the set are eligible; remaining FR-002 extensions get `shape_skipped`.

**Parser update**:
- `FileInventoryMode::parse(raw: &str)` extended to recognize `"source-tree"` → `SourceTree { restriction: None }`. The restriction subset comes from a SEPARATE flag (`--file-inventory-source-shapes`), so the mode's own parse function only handles the enum-value string. Wiring lives in the CLI layer at `scan_cmd.rs` — combines the two flag values into `SourceTree { restriction: Some(parsed_set) }` before calling downstream code.

### `ContentShape` (existing enum, extended)

**Location**: `waybill-cli/src/scan_fs/file_tier/content_shape.rs:32` (existing)

**New variant**:
- `SourceFile` — file classified under the `SourceTree` mode (matches FR-002 extension + not excluded). Distinct from existing variants (`ElfBinary`, `PeBinary`, etc.) so downstream emission logic can differentiate source-tier vs binary-tier vs archive-tier file-tier components.

**Note**: An accompanying `waybill:file-shape` per-component annotation naming the source shape (`"py"`, `"c"`, etc.) is deferred to a follow-up milestone per R7 research decision. v1 does NOT emit per-component shape metadata; downstream discriminator is via the file's path/extension only.

## New annotation (parity catalog)

### C156 — `waybill:file-inventory-source-shapes-active`

**Scope**: document (per m665 C153 pattern)

**Directionality**: `SymmetricEqual`

**Emitted when**: `--file-inventory=source-tree` is active. Absent on the default (`orphan`) path — preserves FR-007 byte-identity.

**Value shape**: JSON-stringified object

```json
{
  "mode": "source-tree",
  "restriction": ["c", "h", "py"]
}
```

or when unrestricted:

```json
{
  "mode": "source-tree",
  "restriction": null
}
```

**Constraints**:
- `mode` field is a closed enum with a single value (`"source-tree"` for v1). Future milestones may add sibling values.
- `restriction` field is `Array<String>` (sorted, lex) OR `null`. Strings match `SourceShape::as_str` output — never contains the `.` prefix.

### Parity extractors

Three new macro invocations (matching m670 T016 pattern):

- `cdx_anno!(c156_cdx, "waybill:file-inventory-source-shapes-active", document);`
- `spdx23_anno!(c156_spdx23, "waybill:file-inventory-source-shapes-active", document);`
- `spdx3_anno!(c156_spdx3, "waybill:file-inventory-source-shapes-active", document);`

Plus one `ParityExtractor` entry in `parity/extractors/mod.rs::EXTRACTORS` with `Directionality::SymmetricEqual`, `order_sensitive: false`.

## State transitions

Not applicable — pure in-memory computation per scan. No persistence, no lifecycle.

## Validation rules

Enforced at parse / classify time:

| Rule | FR reference | Enforcement |
|------|--------------|-------------|
| Operator restriction subset ⊆ FR-002 allowlist | FR-009 | `SourceShape::from_extension(&str) -> Option<Self>` returns `None` for unknown; `parse_restriction` maps to `SourceShapeParseError::UnknownExtension` |
| Empty restriction list → parse fail | FR-009 | `parse_restriction("")` returns `SourceShapeParseError::Empty` |
| Restriction flag without mode → parse fail | FR-001 | `clap`'s cross-arg validation OR post-parse check in `scan_cmd.rs` |
| Default-mode byte-identity | FR-007 | `content_shape::classify` mode-gated bypass; when mode is `Orphan`/`Off`/`Full`, code path is identical to v0.5.0 |
| Derivative artifacts stay excluded | FR-006 | `EXCLUDED_EXTENSIONS` at `content_shape.rs:92` retains `.pyc`, `.o`, `.obj`, `.pyd` — NOT part of FR-002 |
| Path-based dedupe against package-DB claims | FR-004 | m133 FR-011 hybrid dedupe applies verbatim; new mode does NOT bypass the dedupe pass |

## Emission contract

**When `FileInventoryMode::SourceTree { restriction }` is active + a file passes `content_shape::classify`**:

1. Compute SHA-256 (unless `--no-deep-hash`)
2. Compute scan-root-relative path
3. Check m133 FR-011 dedupe (hash-set + path-set) against package-DB-claimed evidence
4. If not deduped, emit `ResolvedComponent` with:
   - `type = "file"` (CDX) / `Package` shape (SPDX)
   - `hashes = [{alg: SHA-256, value: <hex>}]` (or empty per `--no-deep-hash`)
   - `evidence.occurrences = [{location: <rel-path>, ...}]`
   - **No PURL** (file-tier components carry no ecosystem identity)
   - **No annotations from this milestone** — v1 deliberately omits per-component `waybill:file-shape` (deferred per R7)

**On the emitted document (regardless of file-tier count)**:

1. Add `metadata.properties[]` entry (CDX) / doc-scope annotation (SPDX 2.3 / SPDX 3): `waybill:file-inventory-source-shapes-active` per C156 value shape above.

**On the log**:

1. Extend the existing `file_tier walker complete` INFO log line to include the mode name + restriction subset (or "unrestricted"):

   ```
   file_tier walker complete
     file_tier_components=<N>
     mode=SourceTree
     source_tree_restriction=[c,h,py]  # or "none"
     shape_skipped=<M>
     ...
   ```
