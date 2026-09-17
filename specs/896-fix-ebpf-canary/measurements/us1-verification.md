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

Pending — requires a dispatch that builds successfully and then removes the
object before verification.
