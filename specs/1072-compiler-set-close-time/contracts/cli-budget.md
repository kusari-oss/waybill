# Contract: close-time hashing budget (CLI surface)

**Applies to**: `waybill trace run` (`ScanArgs`, `waybill-cli/src/cli/scan.rs:6`).

## C-1072-8. The flag

```
--compiler-hash-budget-bytes <BYTES>    [env: WAYBILL_COMPILER_HASH_BUDGET_BYTES]
```

```rust
/// Cumulative bytes the close-time hashing pass may read across all
/// compiler read/write sets. Unset uses the built-in default.
#[arg(long, env = "WAYBILL_COMPILER_HASH_BUDGET_BYTES", value_name = "BYTES")]
pub compiler_hash_budget_bytes: Option<u64>,
```

`Option<u64>` with no `default_value_t`, so the two states are distinct in
the type rather than inferred from a sentinel:

| value | budget | `budget_source` |
|-------|--------|-----------------|
| absent | the default constant | `"default"` |
| `Some(n)` (flag or env) | `n` | `"operator"` |

Env and flag are the same state deliberately — both are an operator
choosing a number, and the attestation discloses that a choice was made,
not which mechanism carried it.

`0` is a legal value meaning "hash nothing": every existing path is counted
in `paths_skipped`, `truncated` is `true`, and the observation tier is
emitted in full. It is the cheapest way to reproduce the old field coverage
without the old field lies, and it makes US4 testable with no large
fixture.

## C-1072-9. The default constant

**Unset at planning time, by design.** SC-008 forbids an absolute cost
figure entering the spec or plan before measurement, per the project rule on
measuring external behaviour before designing around it.

Until the measurement task records a figure under `measurements/`, the
budget is a **required parameter** of `apply_close_time_facts`. The default
constant lands in the CLI layer only once measured, and its definition site
MUST cite the measurement file.

The measurement establishes the pass's cost against total trace wall-clock
on a real kernel trace, which is also the baseline SC-008's ratio is quoted
against. Nothing downstream may quote a byte figure that is not traceable
to that file.

## C-1072-10. Exhaustion behaviour

On exhaustion the command MUST:

1. set `truncated: true` in the coverage record;
2. emit exactly one `WARN` naming the counts, e.g.
   `close-time hashing budget exhausted: hashed=380 skipped=8 absent=24 total=412 budget_bytes=…`;
3. write the attestation;
4. **exit zero.**

Principle III (Fail Closed) is not engaged. Its triggers are the eBPF trace
failing to attach, losing events, or observing zero dependency activity —
none of which a userspace post-pass budget can cause. No observation is
lost when hashing truncates: every path, read kind and rename chain is still
emitted. Principle X governs instead, and its remedy for "cannot guarantee"
is structured disclosure, which C-1072-5 provides.

Discarding a complete observation tier because an advisory tier was partial
would lose real data to protect a weaker claim.

## C-1072-11. Non-interference

With `--features ebpf-tracing` off, or on a non-Linux host, behaviour is
byte-identical to pre-m1072: no flag effect, no coverage record, no new
output. The pass's module compiles on every host (research R6) but its call
site stays behind the existing
`#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`.

A trace that captured zero compiler invocations runs no pass and emits no
coverage record, so its `trace_integrity` serializes byte-identically to a
pre-m1072 attestation (SC-007).
