# Quickstart: working on this feature

Feature: `specs/1270-stable-probe-tracepoints/` (spec-kit sequence 1270,
which is **not** an issue number) · Issue **#1252**

Everything this feature touches is invisible to a macOS `cargo check`.
The kernel-side code is `#[cfg(target_arch = "bpf")]` by virtue of its
own crate and target; the loader's `inner` module and the trace path in
`cli/scan.rs` are behind `cfg(all(target_os = "linux", feature =
"ebpf-tracing"))`. A clean local run says nothing about any of it.

## 1. Type-check the code that macOS cannot see

```sh
docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .
```

**Prove the check is not vacuous before trusting it.** Introduce a
deliberate type error inside the cfg-gated block, rebuild, and confirm
the build fails at that line. A container build that reports success
while compiling none of the changed code is the failure mode this step
exists to catch, and it has happened on this branch's predecessor.

If a scratch worktree is involved: Colima mounts only `$HOME`, so a
worktree under `/private/tmp` is invisible to the container and fails
with `could not find Cargo.toml in /w`. Put it under `$HOME`.

## 2. Run the integration harness

```sh
docker run --rm --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    waybill-ebpf-test
```

**The debugfs mount is required**, and the Dockerfile's own header is
wrong about it: with only `--privileged`, every tracepoint attach fails
with `tracefs not found` and the trace aborts at `sys_enter_chdir`. CI
does not hit this because it goes through
`scripts/ebpf-container-run.sh`, which passes the mount. A bare
`docker run --privileged` does not.

It traces the two-binary fixture and asserts on the attestation. Since
#1310 it also asserts the honesty invariant: a non-empty
`kprobe_attach_failures[]` with `completeness: complete` is a failure,
and so is the converse. Since #1252 it also asserts a non-zero discarded-open
count, the event-volume ceiling, and an IPv6-only connection to `::1`.

## 3. Take the tracepoint inventory

```sh
docker run --rm -i --privileged alpine:3.20 sh -s \
    < specs/1270-stable-probe-tracepoints/measurements/tracepoint-inventory.sh
```

The `-i` is load-bearing: without it the shell reads EOF, runs nothing
and exits 0. If the output does not begin with a `kernel:` line, it did
not run. Record new runs in `measurements/findings.md` with the kernel
release and architecture — the x86_64 run is still owed, and the
per-arch attach list depends on it.

## 4. The baseline, and what it can and cannot be

`measurements/baseline-before.md` holds it, taken from a worktree at the
commit before this feature so nothing in it is contaminated by work in
progress. Build that image from a worktree **under `$HOME`** — Colima
mounts only `$HOME`, so a worktree under `/private/tmp` is invisible to
the container and fails with `could not find Cargo.toml in /w`.

Note what the baseline is *not*. SC-001a asks for a "successful-open
baseline" taken before the change, and that cannot be done: the
pre-change build observes requests and has no idea which succeeded. That
is the defect, so it cannot also be the instrument. The executable form,
which is stronger, is the identity `after ∪ discarded == before` with
`after ∩ discarded == ∅` — see the baseline file's own note.

The 902 figure quoted in #1252 was measured under request semantics. A
lower count after the change is the correct outcome, so comparing
against it would fail a working implementation.

## 5. The positive control for the discard counter

A zero discard count is ambiguous: either the build attempted no failed
opens, or the exit tracepoint is not firing. Resolve it by forcing the
condition — have the traced build open a path that does not exist — and
confirm `open_failures_discarded` is non-zero. Only then does a zero on
the real fixture mean anything.

Compilers attempt failed opens constantly (header and library search
paths), so the fixture build is expected to produce a non-zero count
without being asked. If it reports zero, suspect the capture before
concluding the build is unusually tidy.

## 6. Verify SC-001b entry by entry

```sh
# 1. Trace with the discard record on.
WAYBILL_TRACE_OPEN_DIAG=1 waybill trace capture ...

# 2. Extract the discarded paths. STRIP ANSI FIRST -- the log is
#    colourised, and a grep for `path=` against the raw bytes matches
#    whole lines instead. That reported "599 distinct discarded paths"
#    when 599 was the event count and 203 the distinct-path count.
sed 's/\x1b\[[0-9;]*m//g' trace.log \
  | sed -n 's/.*discarded open.*path=\([^ ]*\) errno=.*/\1/p' \
  | sort -u > discarded.txt

# 3. Diff the sets with the per-run random names masked.
python3 measurements/diff-sets.py
```

Every removed entry must appear in the discard record. A diff whose
*size* matches the discard count is not verification — the same count
is consistent with a join that dropped successful opens too.

**Mask the random names or the diff is unreadable.** rustc and cargo
name temp directories randomly per run (`rmetaTmtspV`, `.tmpWRoguS`,
`targeto3Q2sv`), so comparing two runs always shows them as removed and
re-added. Unmasked, the first comparison reported 52 unexplained
removals and 18 additions — a serious-looking regression that was
nothing. The signature to look for before believing one: an unexplained
removal paired with a matching addition of the same shape.

## 6a. Reach the degraded paths on purpose

Both exist to handle a host this one is not, so neither runs unless you
ask. A fallback that has never executed is a fallback nobody knows
works.

```sh
# FR-004b: pretend this host cannot report open results. Expect an
# attestation that names sys_exit_openat and reads partial.
WAYBILL_TRACE_FORCE_REQUEST_ONLY=1 waybill trace capture ...
```

## 7. Pre-PR

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr \
    -- ./scripts/pre-pr.sh
```

No `+toolchain` argument — the pin in `rust-toolchain.toml` is what
makes the gate match CI (#1189). For the feature-on path:
`WAYBILL_PREPR_EBPF=1 ./scripts/pre-pr.sh`.

Note that `mod trace;` is declared in `main.rs`, not `lib.rs`, so the
trace tests live in the **binary** target: `cargo test -p waybill --lib`
finds none of them. Use `--bins`.
