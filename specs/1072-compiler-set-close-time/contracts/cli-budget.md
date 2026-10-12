# Contract: close-time hashing budget (CLI surface)

**Applies to**: `waybill trace capture` (`ScanArgs`,
`waybill-cli/src/cli/scan.rs:6`) — the path the pass runs on — and
`waybill trace run`, which builds that same `ScanArgs` at
`waybill-cli/src/cli/run.rs:361` from its own `RunArgs` and must forward the
flag, or FR-016's override is unreachable on half the CLI surface.

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

**Unset at planning time, by design.** The project rule on measuring
external behaviour before designing around it forbids an absolute **byte**
figure entering the spec or plan before measurement.

This is not SC-008's ratio. SC-008 commits a 5%-of-wall-clock ceiling up
front, precisely so the measurement can miss it; this clause governs the
byte constant, which has no defensible value until measured. Two numbers,
two rules.

Until the measurement task records a figure under `measurements/`, the
budget is a **required parameter** of `apply_close_time_facts`. The default
constant lands in the CLI layer only once measured, and its definition site
MUST cite the measurement file.

The measurement establishes the pass's cost against total trace wall-clock
on a real kernel trace — the baseline SC-008's 5% is quoted against, not the
ceiling itself. Nothing downstream may quote a byte figure that is not
traceable to that file.

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
byte-identical to pre-m1072: no coverage record, no coverage WARN, no change
to any emitted document. The pass's module compiles on every host (research
R6) but its call site stays behind the existing
`#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`.

One difference is visible and intended. `--compiler-hash-budget-bytes` is a
plain `ScanArgs` field rather than a cfg-gated one, so it appears in `--help`
on every host and parses without error; where the call site is compiled out
it has no effect. Gating the flag itself would make the help text differ by
build, which is worse for an operator than a flag documented as Linux-only.
T049 asserts the no-effect half, so this clause is checked rather than
asserted.

A trace that captured zero compiler invocations runs no pass and emits no
coverage record, so its `trace_integrity` serializes byte-identically to a
pre-m1072 attestation (SC-007).
