# Quickstart — milestone 1072 (#1144)

How to work on this feature, and what to run to see the defect and its fix.

## See the defect first

Before changing anything, reproduce #1144 from committed data — no kernel
needed:

```sh
# Every write-set entry claims it survived; every hash is 64 zeros.
grep -n 'or_insert((Some(hash), true))' waybill-cli/src/trace/compiler_pipeline.rs
grep -n 'Placeholder — final hash' waybill-cli/src/trace/compiler_pipeline.rs
```

And the measured ground truth the fix has to produce instead:

```sh
./specs/1072-compiler-set-close-time/measurements/probe.sh
```

Probe 3 prints `none (expected: #1144's case)` — rustc's `deps/rmeta*`
temporaries are gone by trace end, while the attestation says they
survived.

## Most of this is testable on macOS

The close-time pass lives in `waybill-cli/src/trace/close_time.rs` and
compiles **unconditionally** (research R6). Only its call site in `scan.rs`
is behind `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`.

So the normal loop is the normal loop:

```sh
./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"
grep -E 'all pre-PR checks passed|test result: ok' /tmp/prepr.log
```

Assert on the **positive** signal — the `>>> all pre-PR checks passed.`
line and the per-target `test result: ok. N passed; 0 failed` lines. The
absence of the word FAILED is not evidence: a build that never compiled also
contains no failures.

Unit tests for the pass use `tempfile::tempdir()` against the real
filesystem, so every edge case in `spec.md` is reachable without
privileges or eBPF:

| edge case | how the test builds it |
|-----------|------------------------|
| deleted intermediate | create, record the path, `remove_file` |
| unreadable | `std::fs::set_permissions` to `0o000` (non-root only) |
| not a regular file | `create_dir` at a recorded write path |
| over the per-file cap | pass a tiny `max_bytes`, assert `paths_skipped` |
| over the remaining budget | `Budget { total_bytes: 0, .. }` |
| shared path across invocations | two synthetic invocations, one path |

Remember the test-module convention this crate needs, since clippy's
`--all-targets` enforces `unwrap_used` inside `#[cfg(test)]` too:

```rust
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
```

## What needs a real kernel

SC-001 – SC-006 assert on an actual trace, so they need a real kernel. macOS
`cargo check` is blind to the gated call site — see the standing note in
memory about the eBPF container test gap.

The *committed* form of these assertions is the `m1072` section of
`scripts/ebpf-integration-test.sh`, which runs under `--privileged` via
`Dockerfile.ebpf-test` (tasks T023/T027/T028/T033/T042/T043). Constitution VII
is why they live there rather than in a cfg-gated Rust test: the feature-on
`cargo test` lane runs unprivileged. The commands below are the manual
equivalent for working on them interactively.

```sh
# in the Linux VM
cargo run -p xtask -- ebpf
cargo +stable test --workspace --no-fail-fast --features ebpf-tracing

# `trace capture`, not `trace run`: run's SBOM step fails on a hermetic
# cargo build ("resolution produced zero components from attestation"),
# recorded at scripts/ebpf-integration-test.sh:44-50. capture is also the
# `ScanArgs` path this feature modifies.
sudo ./target/release/waybill trace capture \
  --attestation-format waybill-v1 \
  --output /tmp/att.json \
  -- cargo build --release \
       --manifest-path waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge/Cargo.toml
```

Then check the three things #1144 is about:

```sh
# 1. no all-zero hashes anywhere (SC-002)
grep -c '"value": "0\{64\}"' /tmp/att.json      # expect 0

# 2. at least one entry reports absent -- the positive control that the
#    pass actually ran, rather than defaulting to true (SC-001)
jq '[.predicate.compiler_pipeline.invocations[]
     | .write_set[] | select(.existed_at_close == false)] | length' /tmp/att.json

# 3. a hash waybill emitted matches an independent one (SC-003)
jq -r '.predicate.compiler_pipeline.invocations[].write_set[]
       | select(.sha256_at_close) | "\(.sha256_at_close.value)  \(.path)"' \
  /tmp/att.json | head -10 | while read -r h p; do
    printf '%s  %s\n' "$(sha256sum "$p" | cut -d" " -f1)" "$h"
  done
```

A zero from check 2 is a **failure**, not a clean result: it is
indistinguishable from the pass never running. That is the whole reason
SC-001 demands at least one `false`.

## Determinism check (SC-005)

Byte-identity is load-bearing for signing, so check it directly rather than
trusting the ordering code:

```sh
for i in 1 2; do
  sudo ./target/release/waybill trace capture --attestation-format waybill-v1 \
    --output /tmp/att-$i.json -- \
    cargo build --release --manifest-path …/two_binaries_diverge/Cargo.toml
  jq -S . /tmp/att-$i.json | grep -v '"trace_start"\|"trace_end"\|"pid"' > /tmp/norm-$i.json
done
diff /tmp/norm-1.json /tmp/norm-2.json && echo "byte-identical"
```

Clean the fixture's `target/` between runs or the second build is a no-op
and the comparison is vacuous — two empty traces also match.

## Budget truncation (SC-006)

No large fixture required; `0` means "hash nothing":

```sh
sudo ./target/release/waybill trace capture --attestation-format waybill-v1 \
  --compiler-hash-budget-bytes 0 --output /tmp/att-trunc.json -- cargo build --release …
echo "EXIT=$?"      # must be 0

jq '.predicate.trace_integrity.compiler_hash_coverage' /tmp/att-trunc.json
# truncated: true, paths_hashed: 0, budget_source: "operator",
# and hashed + absent + skipped == total
```

## Order of work

1. **Measure first.** The default budget constant must not be guessed
   (`contracts/cli-budget.md` C-1072-9). Until the measurement lands,
   `Budget` is a required parameter with no default. SC-008's 5%
   wall-clock ceiling is not that number — it is fixed in advance and the
   measurement either meets it or reports a finding.
2. Wire shapes in `waybill-common` (E1–E4) — this breaks compilation at
   every consumer, which is the point: it enumerates the work.
3. Aggregator maps + rename accumulation (E5).
4. The pass itself, with its tempdir unit tests (E6).
5. Call site, flag, and the C130 payload builder (`compiler_pipeline_annotation.rs:129`).
6. Attestor bump — **two** constants (`compiler_pipeline.rs:216`,
   `witness.rs:57`) and their two tests.

## Files you will touch

```
waybill-common/src/attestation/compiler_pipeline.rs   E1, E2, attestor const
waybill-common/src/attestation/integrity.rs           E3, E4
waybill-common/src/attestation/witness.rs             attestor const
waybill-cli/src/trace/close_time.rs                   NEW — the pass
waybill-cli/src/trace/compiler_pipeline.rs            E5, stdin sentinel
waybill-cli/src/cli/scan.rs                           call site, `let mut trace`, flag
waybill-cli/src/generate/compiler_pipeline_annotation.rs  C130 payload
waybill-cli/src/cli/run.rs                            ScanArgs literal (:361)
scripts/ebpf-integration-test.sh                      the m1072 kernel assertions
```

No golden regeneration: nothing under `waybill-cli/tests/fixtures/` records
these fields (research R7).
