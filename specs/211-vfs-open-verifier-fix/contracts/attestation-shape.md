# Contract: attestation JSON shape (FR-003 wire-shape preservation)

**Milestone**: 211
**Date**: 2026-07-20
**Purpose**: Explicit statement that m211 changes NO fields, values, or structure in the emitted attestation JSON — only populates `.predicate.file_access.operations[]` with real events that were previously always empty.

## A-1: JSON schema equality

**Assertion**: The JSON schema of `predicate.file_access` before and after m211 is IDENTICAL.

**What this means precisely**:
- Field names identical.
- Field types identical (arrays stay arrays, strings stay strings, integers stay integers).
- Nested-object structure identical.
- Serde-derive orderings identical.
- Optional-field emission gates identical (`skip_serializing_if` conditions unchanged).

## A-2: Only-populated-vs-empty is the diff

**Assertion**: The ONLY user-observable JSON difference from m211 is that `.predicate.file_access.operations[]` transitions from `[]` (empty) to `[{...}, {...}, ...]` (populated) on Linux 6.5+ kernels within the FR-001 support matrix. Every other field of the attestation is byte-identical.

**Corollaries**:
- `.predicate.network_trace` — byte-identical.
- `.predicate.compiler_pipeline` — byte-identical (comes from milestone 210's fixed code path).
- `.predicate.trace_integrity` — byte-identical EXCEPT `.kprobe_attach_failures[]` may lose entries for `vfs_open` and `do_filp_open` when the fix succeeds on the current kernel (correct: the failures no longer occur).
- `.predicate.metadata` — byte-identical.
- `.subject[]`, `._type`, `.predicateType` — byte-identical.

## A-3: Per-operation JSON shape (individual FileAccess record)

Each entry in `.predicate.file_access.operations[]` follows the SAME JSON shape as the pre-m211 code would have produced IF the pre-m211 code had ever loaded successfully. mikebom-common's `FileEvent` serializer (via `#[derive(Serialize)]` on the type in `mikebom-common/src/events.rs`) is the source of truth:

```json
{
  "event_type": "Open",
  "timestamp_ns": 132577805032194,
  "pid": 1462516,
  "tid": 1462516,
  "comm": "rustc",
  "path": "/home/dev/proj/src/main.rs",
  "path_truncated": false,
  "flags": 0,
  "bytes_transferred": 0,
  "content_hash": "0000...0000",
  "inode": 4718592
}
```

No new fields added by m211. No existing fields removed by m211. Only `path_truncated`'s SEMANTIC interpretation refines per `data-model.md` E1 (previously always `false`; now `true` iff `n == 256`).

## A-4: Downstream consumer contract preserved

**Assertion**: mikebom-cli's own `sbom generate --attestation <path>` consumer (which drives m210's C130 emission via `map_component_to_source_read_set`) requires NO changes.

**Rationale**: The consumer reads `.predicate.file_access.operations[]` as an ARRAY of records with the schema in A-3. The records go from "the array is empty" to "the array has real content" — the consumer's parsing logic doesn't need to change.

**Regression guard**: run `mikebom sbom generate --attestation <post-m211-trace> --path <fixture>` and assert the same code path emits successfully (no new errors from schema drift).

## A-5: Downstream witness attestor consumer

**Assertion**: `witness-v0.1` attestation-format consumers (per m210 T039 the `compiler-invocation/v0.1` inner attestor) — witness itself and downstream go-witness-aware verifiers — receive NO shape change.

**Rationale**: witness wraps the mikebom-v1 predicate in its own attestation-collection envelope. The wrapping is unchanged; only the inner predicate's `file_access.operations` field acquires content. Witness consumers who ignore `file_access` see zero change; consumers who consume it see real data where they saw an empty array before.
