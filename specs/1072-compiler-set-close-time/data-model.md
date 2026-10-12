# Phase 1 data model — milestone 1072 (#1144)

Entities are given as the Rust shapes that carry them, because these types
*are* the wire format: `waybill-common/src/attestation/` serializes straight
to the predicate. Field order matters — JSON field order follows struct
order, and FR-014 / SC-007 require pre-m1072 prefixes to stay
byte-identical.

Legend: **O** = observation tier (what the kernel saw), **C** = close-time
tier (what userspace measured after the command exited). The tiers are kept
structurally distinct per the SBOMit pattern in `spec.md`.

## E1. `ReadSetEntry` (modified)

`waybill-common/src/attestation/compiler_pipeline.rs:81`

| field | tier | type | change |
|-------|------|------|--------|
| `path` | O | `PathBuf` | unchanged |
| `kind` | O | `ReadKind` | unchanged |
| `sha256` | — | `ContentHash` | **removed** (FR-001) |
| `existed_at_close` | C | `bool` | **new** (FR-002) |
| `sha256_at_close` | C | `Option<ContentHash>` | **new** (FR-002) |

```rust
pub struct ReadSetEntry {
    pub path: PathBuf,
    pub kind: ReadKind,
    // ── close-time tier: the file's state when the trace ended,
    //    NOT the bytes the compiler read. See spec.md.
    pub existed_at_close: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256_at_close: Option<ContentHash>,
}
```

Removing `sha256` is a **breaking** field removal, sound under FR-018's
reasoning: the field only ever emitted 64 zeros. It is also what retires
the `<stdin>` sentinel (FR-007) without a special case — `ReadKind::StdinInput`
keeps its `bytes_read`, and the entry simply reports
`existed_at_close: false` with no hash.

Validation: `sha256_at_close.is_some()` implies `existed_at_close == true`.
A hash of a file that did not exist is unrepresentable in practice and is
asserted in tests.

A `StdinInput` entry is not a filesystem path and is never stat'd — see the
exclusion in E6 step 1 and C-1072-2a. Stat'ing `"<stdin>"` would resolve it
against the working directory, so a build containing a file of that name
would report the stdin entry as having survived.

## E2. `WriteSetEntry` (modified)

`waybill-common/src/attestation/compiler_pipeline.rs:96`

| field | tier | type | change |
|-------|------|------|--------|
| `path` | O | `PathBuf` | unchanged — the **final** observed path (FR-009) |
| `renamed_from` | O | `Vec<PathBuf>` | **new** (FR-008) |
| `sha256` | — | `Option<ContentHash>` | **removed**, superseded by `sha256_at_close` |
| `survived_trace_window` | — | `bool` | **renamed** → `existed_at_close` (FR-005) |
| `existed_at_close` | C | `bool` | replaces the above, now actually derived (FR-003) |
| `sha256_at_close` | C | `Option<ContentHash>` | **new** (FR-002) |

```rust
pub struct WriteSetEntry {
    pub path: PathBuf,
    /// Observed intermediate names, in observed order. Renames only --
    /// a hard link is not a rename (FR-009a).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renamed_from: Vec<PathBuf>,
    // ── close-time tier ──
    pub existed_at_close: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256_at_close: Option<ContentHash>,
}
```

Note `sha256` is dropped rather than renamed. It was already
`Option<ContentHash>` with a doc comment claiming it is `None` for deleted
intermediates — a claim the aggregator never honoured
(`compiler_pipeline.rs:240`, `.or_insert((Some(hash), true))`). Keeping the
name would preserve a field whose documented meaning and actual meaning
have never agreed.

Invariants:

- no entry's `path` appears in another entry's `renamed_from` (SC-004);
- `renamed_from` is append-only in observation order, so a two-step rename
  reads oldest-first;
- two entries naming one path agree on both close-time fields (FR-011) —
  guaranteed by construction, since the pass stats and hashes a path once.

## E3. `CompilerHashCoverage` (new)

Lives in `waybill-common/src/attestation/integrity.rs`, beside
`TraceIntegrity`.

```rust
pub struct CompilerHashCoverage {
    pub paths_total:   u64,
    pub paths_hashed:  u64,
    pub paths_absent:  u64,
    pub paths_skipped: u64,
    pub truncated:     bool,
    pub budget_source: BudgetSource,
}

#[derive(…, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSource { Default, Operator }
```

**All four counts are over distinct paths, not set entries** (FR-013). A
`.rmeta` read by three invocations contributes 1, not 3 — measured as the
normal case in `measurements/findings.md` Q3.

| count | meaning |
|-------|---------|
| `paths_total` | distinct paths across every read and write set |
| `paths_hashed` | existed, was a regular file, fit the budget, hash succeeded |
| `paths_absent` | did not exist at close |
| `paths_skipped` | existed but not hashed: unreadable, not a regular file, over the per-file cap (R2), or over the remaining budget |

Invariant, enforced by a unit test over every edge case in `spec.md`:

```
paths_hashed + paths_absent + paths_skipped == paths_total
```

`BudgetSource` is a two-variant enum rather than a bool so the wire shape
reads as `"budget_source": "operator"` instead of
`"budget_was_overridden": true` — Principle IV (no raw primitives for
domain values across boundaries).

## E4. `TraceIntegrity` (extended)

`waybill-common/src/attestation/integrity.rs:7`

One additive field, **last** in the struct, following the m1070 precedent
two lines above it:

```rust
    /// Milestone 1072 (#1144): what the close-time hashing pass covered.
    /// Absent when the trace captured zero compiler invocations, so
    /// pre-m1072 attestations serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_hash_coverage: Option<CompilerHashCoverage>,
```

`TraceIntegrity` derives `Default`, so `Option` + `serde(default)` keeps
both round-tripping and `..Default::default()` construction working at the
four existing construction sites (`aggregator.rs:502`,
`scan_cmd.rs:4142`, `pipeline_legacy_reference.rs:509`, and the tests).

## E5. `PartialInvocation` in-flight maps (modified, internal)

`waybill-cli/src/trace/compiler_pipeline.rs:114-116`. Not a wire type, but
the change is what makes the tier separation type-enforced.

| | before | after |
|---|--------|-------|
| `read_set` | `HashMap<PathBuf, (ContentHash, ReadKind)>` | `HashMap<PathBuf, ReadKind>` |
| `write_set` | `HashMap<PathBuf, (Option<ContentHash>, bool)>` | `HashMap<PathBuf, PendingWrite>` |

```rust
/// Observation-tier state of an in-flight write. Carries no hash:
/// the kernel has none to give (research R1).
struct PendingWrite {
    renamed_from: Vec<PathBuf>,
}
```

After this change the aggregator cannot thread a kernel hash into either
set, because neither map has a field to put one in. The `ContentHash`
import and the `content_hash_from_bytes` call at
`compiler_pipeline.rs:229` drop out of these two code paths entirely.

`ObservedKind::Rename { from }` becomes an accumulation:

```rust
ObservedKind::Rename { from } => {
    let mut pending = from.as_ref()
        .and_then(|old| invocation.write_set.remove(old))
        .unwrap_or_default();
    if let Some(old) = from {
        pending.renamed_from.push(old.clone());   // FR-008, observed order
    }
    invocation.write_set.insert(path, pending);
}
```

A rename whose source was never observed as a write yields an entry with
`renamed_from: [old]` and no prior history — the `unwrap_or_default()` arm,
which is `spec.md`'s "renamed a file it did not create" edge case. A rename
target colliding with an existing entry keeps the destination's position
and appends, which is the "accumulates both histories" edge case.

## E6. `CloseTimePass` result (new, internal)

`waybill-cli/src/trace/close_time.rs`, compiled unconditionally (research
R6).

```rust
pub struct Budget {
    pub total_bytes: u64,
    pub source: BudgetSource,
}

/// Applies close-time facts to every entry in `data`, in place, and
/// returns what it covered. Never adds, removes or reorders an entry --
/// the observation tier is read-only here (FR-010, Principle II).
pub fn apply_close_time_facts(
    data: &mut CompilerPipelineData,
    budget: Budget,
) -> CompilerHashCoverage
```

Algorithm, fixed by FR-011/FR-011a so that two runs agree:

1. Collect the distinct path set: every `write_set[].path`, then every
   `read_set[].path`, each sorted lexicographically. A path in both is
   visited once, in its write-set position. Entries whose
   `kind` is `StdinInput` are **excluded**: `"<stdin>"` is a marker, not a
   location, so it is never stat'd, gets `existed_at_close: false`
   directly, and contributes to none of the four counts
   (`contracts/read-write-set-shape.md` C-1072-2a).
2. For each path in that order: `symlink_metadata` →
   - absent → `existed_at_close = false`, `paths_absent += 1`;
   - present, not a regular file → `existed_at_close = true`,
     `paths_skipped += 1`;
   - present, regular, `len() > remaining_budget` → `existed_at_close = true`,
     `paths_skipped += 1`, `truncated = true`, **continue** (FR-012);
   - present, regular, fits → hash; on `Ok` set `sha256_at_close`,
     `paths_hashed += 1`, subtract from remaining; on `Err` (unreadable, or
     over the 256 MB per-file cap of R2) `paths_skipped += 1` and a WARN.
3. Fan the per-path result back out to **every** entry naming that path, in
   both sets, across all invocations (FR-011).

Determinism: the path set is sorted before iteration and the budget is
charged in that order, so the truncation point is a function of the build's
content alone — not of wall-clock, thread scheduling or `HashMap` iteration
order. This is what SC-005 and feature 006 US2 depend on.

## Removed concepts

- **The zero-hash sentinel.** `ContentHash` built from `"0".repeat(64)` at
  `compiler_pipeline.rs:528`-ish (`record_stdin_input`) and the
  `zero_sha256()` test helper at `:225`. FR-006 forbids the value; after E1
  and E5 there is no field left to hold it.
- **`survived_trace_window` as a concept**, not merely as a name. The old
  field was a hardcoded `true`; the new one is a `symlink_metadata` result.
