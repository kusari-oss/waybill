# Contract: `--file-inventory=<mode>` — extended with `source-tree`

**File touched**: `waybill-cli/src/cli/scan_cmd.rs` + `waybill-cli/src/scan_fs/file_tier/mod.rs`
**FRs covered**: FR-001, FR-002 (indirect), FR-007

## Existing values (unchanged from v0.5.0)

- `off` — do not emit file-tier components; walker is entirely disabled
- `orphan` (default) — surface unattributed content per the m133 `EXCLUDED_EXTENSIONS` allowlist
- `full` — same as `orphan` but bypasses the m133 FR-011 hybrid dedupe (emits duplicates for files also covered by package-DB readers)

## New value (m671)

- `source-tree` — surface source-code file extensions (FR-002 allowlist) as file-tier components. Existing `EXCLUDED_EXTENSIONS` list stays authoritative for docs/configs/etc. — only the FR-002 subset of source-code extensions is unblocked.

### Interaction with `--file-inventory-source-shapes`

- `source-tree` alone: all 21 FR-002 extensions are eligible.
- `source-tree` + `--file-inventory-source-shapes=<list>`: only extensions in `<list>` are eligible; extensions outside the list are `shape_skipped` (same code path as the default mode).

### Interaction with `--no-deep-hash`

Same as existing file-tier behavior: `--no-deep-hash` omits the SHA-256 hash from every file-tier component's `hashes[]`. The path evidence stays populated.

### Interaction with `--exclude`

`--exclude` globs are applied uniformly. A file matching `--exclude` is NOT emitted as a file-tier component under any mode.

## Emission

Under `source-tree` mode:

- Emits `metadata.properties[]` C156 annotation with JSON-stringified value per `../data-model.md`.
- Emits one file-tier component per file that passes:
  1. `SourceShape::from_extension(ext)` returns `Some`
  2. The restriction (if present) contains that shape
  3. m113 `--exclude` doesn't match the path
  4. m133 FR-011 dedupe doesn't suppress the path

## Error posture

- Unknown mode value → `clap` fails with usage error listing accepted values (`off`, `orphan`, `full`, `source-tree`)
- Combining `source-tree` with an incompatible flag (none identified for v1) → future extensibility

## Test coverage (mapped to spec SCs)

- Default mode byte-identity → 6 golden test suites (SC-004)
- `source-tree` mode with cpython-shape fixture → ≥ 100 file-tier components (SC-001)
- `source-tree` + shape restriction → subset behavior (SC-006)
- C156 annotation present under `source-tree`, absent under default (SC-007)
