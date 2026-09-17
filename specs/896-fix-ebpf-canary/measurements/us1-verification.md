# US1 verification (T017-T020)

## T017 — SC-001: a run against the pinned version passes

| | |
|---|---|
| Run | [`35257496241`](https://github.com/kusari-oss/waybill/actions/runs/35257496241) |
| Dispatched | `--ref 896-fix-ebpf-canary -f version=0.11.0 -f dry_run=true` |
| Conclusion | **success** |
| Significance | The canary's **first green run in 36 attempts** (see `run-history.md`) |

Every new step ran rather than being skipped:

```
success  Resolve pinned version
success  Install eBPF build deps
success  Install nightly Rust with rust-src
success  Install stable Rust toolchain
success  Make stable the default toolchain
success  Cache cargo + build artifacts
success  Remove rustup-init (prevents `cargo +<channel>` dispatch race)
success  Assert toolchain parity with ci.yml
success  Install bpf-linker (via m234 composite)
skipped  Assert latest-mode actually differs from pin   <- correct: explicit version, not `latest`
success  Remove stale eBPF artifact
success  Build eBPF kernel object
success  Verify eBPF artifact was produced
success  Build userspace binary with ebpf-tracing feature
```

**Unintended side effect**: this run closed #685. See README.md — `report-success`
did not honour `dry_run`. Issue reopened; workflow fixed in the same branch.

## T018 — SC-002: the parity check has teeth (verified locally)

The step's `run:` script was extracted verbatim from the YAML and executed
against mutated copies of `ci.yml`:

| Mutation | Result |
|---|---|
| none (real `ci.yml`) | `ok: rust-src installed`, exit 0 |
| nightly block requires a component this host lacks | `::error::...requires nightly component 'waybill-nonexistent-component'`, **exit 1** |
| nightly block declares two `components:` lines | `::error::Expected exactly 1 components declaration...found 2`, **exit 1** |

The second mutation matters: the first version of this check scanned to
end-of-file rather than to end-of-step and counted **3** declarations in the
real `ci.yml` (which has six `components:` lines). The count guard caught the
check's own bug before it ever ran in CI.

## T019 — SC-008: the artifact check has teeth

The build step was temporarily mutated to delete its own output on success
(commit `5fafc862`, reverted in `b1d085f2`) and dispatched.

| | |
|---|---|
| Run | [`35258175801`](https://github.com/kusari-oss/waybill/actions/runs/35258175801) |
| Conclusion | **failure** — exactly the FR-002a case |

```
success  Build eBPF kernel object              <- exited zero
failure  Verify eBPF artifact was produced     <- caught it anyway
skipped  Build userspace binary with ebpf-tracing feature
```

Runtime output:

```
##[error]Build reported success but produced no artifact at
waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf. Reporting red — a
green here would authorise a version bump on the strength of nothing having
been built.
```

Before m896 this run would have concluded **green** and, via the
`report-success` gap below, closed the open report.

### A grep mistake worth not repeating

Checking this, `grep '::error::Build reported success' | tail -1` appeared to
show `$obj` unexpanded. It had not failed to expand: `gh run view --log-failed`
includes the runner's **echo of the script source**, which contains the literal
`::error::` string, while the runtime line renders as `##[error]` and carries
the expanded path. The source line was the only thing the pattern could match.
Grep for `##[error]` when you want output, not `::error::`.

## T020 — record

This file. Also see README.md for the `report-success` / `dry_run` defect that
run 35257496241 exposed.
