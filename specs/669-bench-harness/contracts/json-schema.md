# Contract: JSON schema (baseline.json + run.json)

**Feature**: `669-bench-harness` | **Applies to**: `docs/perf/baseline.json`, `target/bench/run-<sha>.json`

Both files share the same schema — `BenchRun` per `data-model.md`. Contracts here govern schema stability, versioning, and validation.

## Interface

### Input (for consumers reading the file)

- **File path**: `docs/perf/baseline.json` (committed baseline) or `target/bench/run-<git-sha>.json` (per-run capture).
- **Format**: UTF-8 JSON, indent-2 pretty-printed, sorted keys within objects (deterministic for git diffing).
- **Schema**: matches `data-model.md` `BenchRun`.

### Output (from writers — xtask bench + xtask bench --update-baseline)

- Same shape, atomic write via `tempfile::NamedTempFile` + rename to prevent partial-write corruption during concurrent CI runs.

## Behavioral contracts

### C-1: Root-level `schema_version` field mandatory

Every emitted file MUST have `"schema_version": <int>` at the root. v1 is the only value shipped for m669's initial merge. Consumers reading a file MUST refuse to process files where `schema_version != 1` (V1 validation rule). Forward compat: consumers written for v1 fail-close on v2+; won't misinterpret schema.

**Wrong**:
```json
{
  "metadata": {...},
  "results": [...]
}
```

**Right**:
```json
{
  "schema_version": 1,
  "metadata": {...},
  "results": [...]
}
```

### C-2: Field additions are additive-only for 12 months

FR-005 mandates that once v1 ships, no field is renamed or removed for at least 12 months. Adding new optional fields is permitted at any time. Enforcement: xtask-side serde-derive uses `#[serde(default)]` on all v1-added-after-initial-ship optional fields so v1-consumer reading v1-emitted-later files doesn't fail on missing fields. Reviews of `schema.rs` PRs verify no `rename` attribute is added to any existing field.

### C-3: Sorted-key output for deterministic git diffs

Baseline file MUST be written with sorted object keys so `git diff docs/perf/baseline.json` produces minimal noise across baseline updates. Enforced via `serde_json::Serializer` in xtask's baseline-writer path using the `IndexMap`-preserving output order + pre-sorting the results vec by `(fixture_name, mode)` before serialization.

### C-4: Every Result has both SHAs

Per V4 + FR-013, every `BenchResult` in every emitted file MUST have non-empty 40-char hex `waybill_commit_sha` + `fixture_sha`. Emission-time assert; refuses to write if either is empty.

### C-5: Median-vs-samples consistency

Per V3, every `BenchResult.median_wall_clock_ms` MUST equal `raw_samples_ms.sorted()[2]` (0-indexed middle-of-5). Assert at write time. Guards against a sampler bug or serialization mistake.

### C-6: Baseline atomic write

Baseline updates MUST use atomic file replacement (`tempfile::NamedTempFile::persist`) to prevent partial-write corruption if a CI job is killed mid-write. Verified by unit test: `test_baseline_write_is_atomic_under_kill` fires `xtask bench --update-baseline` in a subprocess, `kill -9`s it mid-write, and asserts the baseline file is either the pre-write bytes or the fully-written bytes — never a truncated partial.

## Non-contracts

- **JSON schema NOT expressed in JSONSchema format** — no `.json_schema.json` shipped. The Rust `schema.rs` structs ARE the contract; documented in `data-model.md` prose. Adding a JSONSchema file is a possible v2 follow-up if external consumers need it.
- **Backwards compatibility with schema_version == 0** — not supported. m669 v1 is the initial ship; no v0 exists.

## Test-authoring rules

- **T1**: `xtask/tests/schema_roundtrip.rs` — round-trips a hand-crafted `BenchRun` through serialize + deserialize, asserts field-preservation. Locks every field name to its wire representation. New fields added later automatically get covered as they're added to the test's constructor.
- **T2**: `xtask/tests/baseline_atomic_write.rs` — the C-6 kill-during-write test.
- **T3**: `xtask/tests/schema_version_gate.rs` — writes a file with `schema_version: 2`, tries to read it via the v1-only reader, asserts refusal with a clear error message.
