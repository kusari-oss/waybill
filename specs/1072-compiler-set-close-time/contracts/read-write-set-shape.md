# Contract: compiler read/write set wire shape

**Status**: supersedes `specs/210-compiler-pipeline-trace/contracts/attestor-predicate.md`
C-1 and C-2 for `ReadSetEntry` and `WriteSetEntry` only. Every other clause
of the m210 contract stands. The m210 file is a historical record of what
was decided then and is not edited.

**Applies to**: `predicateType: https://waybill.dev/attestation/build-trace/v1`
→ `predicate.compiler_pipeline.invocations[].{read_set,write_set}`, and the
witness attestor payload
`https://waybill.dev/attestation/compiler-invocation/v0.2`.

## C-1072-1. Two tiers, structurally distinct

Every entry in either set carries an **observation tier** — facts the eBPF
trace witnessed — and a **close-time tier** — facts userspace measured after
the traced command exited. A consumer MUST be able to tell them apart from
the field name alone, without reference to this document.

| tier | read set | write set |
|------|----------|-----------|
| observation | `path`, `kind` | `path`, `renamed_from` |
| close-time | `existed_at_close`, `sha256_at_close` | `existed_at_close`, `sha256_at_close` |

A field in the close-time tier MUST be named with the `_at_close` suffix.
No field in either set may carry an unqualified `sha256` key.

## C-1072-2. Read-set entry

```json
{
  "path": "/build/src/lib.rs",
  "kind": "file",
  "existed_at_close": true,
  "sha256_at_close": {
    "algorithm": "sha256",
    "value": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
  }
}
```

- `sha256_at_close` is **the content of `path` when the trace ended.** It is
  NOT the content the compiler read. A verifier binding a component to a
  source hash MUST NOT use this field for that purpose.
- `sha256_at_close` is absent whenever the path was not hashed. Absent means
  "not measured", never "zero".
- `existed_at_close` is a `symlink_metadata` result, never a constant: true
  iff the call succeeded. It asserts that the name resolved, NOT that readable
  content was there — a dangling symlink is `true` with no hash, counted in
  `paths_skipped`. A consumer wanting "content existed" must read
  `sha256_at_close`'s presence instead.

`kind` retains its m210 shape: `"file"`, or
`{"stdin_input": {"bytes_read": N}}`.

### C-1072-2a. `<stdin>` is not a filesystem path

An entry with `kind.stdin_input` has the synthetic path `"<stdin>"`. The
close-time pass MUST NOT stat it:

- `existed_at_close` is set to `false` directly, with no filesystem access;
- `sha256_at_close` is absent;
- the entry contributes to **none** of the four coverage counts, because
  there is no path to have covered.

Rationale: `"<stdin>"` is a marker, not a location. Stat'ing it would
resolve against the working directory, so a build that happened to contain
a file named `<stdin>` would be reported as the stdin entry having survived
— a false statement of exactly the kind this milestone removes. The m210
sentinel hash (64 zeros) is removed with no replacement.

## C-1072-3. Write-set entry

```json
{
  "path": "/build/target/release/deps/liblibsafe-603d5d27712eef2e.rlib",
  "renamed_from": ["/build/target/release/deps/.tmpAbC123.temp-archive/tmp.a"],
  "existed_at_close": true,
  "sha256_at_close": { "algorithm": "sha256", "value": "…" }
}
```

- `path` is the **final** observed path. Exactly one entry exists per
  artifact per invocation (FR-009).
- `renamed_from` lists observed intermediate names in observed order,
  oldest first. Omitted from serialization when empty.
- `renamed_from` records **renames only**. A hard link is not a rename:
  both names survive and share an inode. No link syscall is currently
  traced (research R4), so this clause constrains any future link probe —
  it MUST NOT synthesise a `Rename` event.
- An artifact rustc wrote and deleted reports `existed_at_close: false`
  with `sha256_at_close` absent. This is #1144's defect: before this
  milestone every entry reported `survived_trace_window: true`.

`survived_trace_window` is **removed**. `existed_at_close` carries the same
fact under the same name as the read set.

## C-1072-4. Identical hashes are legitimate

Two entries MAY carry the same `sha256_at_close`:

- two distinct paths sharing an inode (cargo hard-links `deps/<bin>-<hash>`
  to its user-facing name on Linux; a copy on macOS — measured in
  `measurements/findings.md` Q2);
- a path named by several invocations' entries, which by FR-011 is hashed
  once and fanned out.

A consumer MUST NOT treat duplicate hashes as an error and MUST NOT
deduplicate entries on hash equality.

## C-1072-5. Coverage disclosure

`predicate.trace_integrity.compiler_hash_coverage`, absent when the trace
captured zero compiler invocations:

```json
{
  "paths_total": 412,
  "paths_hashed": 380,
  "paths_absent": 24,
  "paths_skipped": 8,
  "truncated": false,
  "budget_source": "default"
}
```

- Counts are over **distinct paths**, not entries. A path named by N
  invocations counts once (`measurements/findings.md` Q3).
- `paths_hashed + paths_absent + paths_skipped == paths_total` MUST hold.
- `budget_source` is `"default"` or `"operator"`.
- `truncated: true` means the cumulative-bytes budget was exhausted; the
  attestation is still written and the command still exits zero.

## C-1072-6. Determinism

Two traces of the same reproducible build under the same budget MUST
produce byte-identical pre-signature canonical payloads, including this
coverage record. This is a hard constraint from feature 006 US2, not a
quality goal: the signature is over these bytes.

It follows from the pass being ordered by sorted path and budgeted in
cumulative bytes. A wall-clock budget would violate it and is forbidden.

## C-1072-7. The pass is read-only over the observation tier

The close-time pass MUST NOT add, remove or reorder entries, and MUST NOT
alter `path`, `kind` or `renamed_from`. Only the two `_at_close` fields are
written.

This is Principle II's observed/enriched boundary applied inside the
predicate: a path that no eBPF event witnessed MUST NOT reach a read or
write set, however visible it is on disk at close time.

## Backward compatibility

| change | kind | justification |
|--------|------|---------------|
| `read_set[].sha256` removed | breaking | only ever emitted 64 zeros |
| `write_set[].sha256` removed | breaking | only ever emitted `Some(64 zeros)` |
| `write_set[].survived_trace_window` → `existed_at_close` | breaking | only ever emitted `true` |
| `renamed_from` added | additive | omitted when empty |
| `existed_at_close`, `sha256_at_close` added | additive | new tier |
| `trace_integrity.compiler_hash_coverage` added | additive | last field, omitted when absent |

`predicateType` stays `build-trace/v1` (FR-018). The witness attestor goes
`compiler-invocation/v0.1` → `v0.2` (FR-017): it is explicitly pre-stable,
so the bump is free and signals the shape change to anything matching on it.

No shipped consumer can observe the breaking changes: `compiler_pipeline`
populates only on Linux behind `--features ebpf-tracing`, off by default
everywhere, and the removed fields were constants.
