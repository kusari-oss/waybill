# eBPF Toolchain: bpf-linker Pin, Canary, and Un-Pin Flow

**Owner**: m234 (Durable eBPF Build Resilience)
**Audience**: waybill release lead, maintainers, contributors touching
the eBPF build path.

## Install method: pre-built binary (v0.11+)

Starting with `bpf-linker` v0.11.0 (upstream 2026-08-12), the
`cargo install bpf-linker` install path requires system LLVM 22.
ubuntu-24.04 (the default `ubuntu-latest` GHA runner) does not ship
LLVM 22, so `cargo install` fails with `unable to find library -lLLVM`.

**Fix**: install the pre-built musl binary from
[aya-rs/bpf-linker releases](https://github.com/aya-rs/bpf-linker/releases).
The pre-built binary statically links LLVM 22 in — no system
dependency required. This is the composite action's DEFAULT
(`install-method: binary`).

The `cargo install` path is still available via
`install-method: cargo` if you have LLVM 22 installed locally and
want to build from source (e.g., testing a patched fork).

Upstream docs-feedback issue tracking the switch:
[aya-rs/bpf-linker#399](https://github.com/aya-rs/bpf-linker/issues/399).

## What lives where

| Path | Purpose |
|---|---|
| `.github/env/bpf-linker.env` | **Single source of truth** for `BPF_LINKER_VERSION`. Bumping this file is the whole un-pin flow. |
| `.github/actions/install-bpf-linker/action.yml` | Composite action that every CI install site invokes. Reads the env file by default; the canary passes `version: latest` to override. |
| `.github/workflows/ci.yml` | Consumer — `ebpf-tracing` lane. |
| `.github/workflows/release.yml` | Consumer — `Build eBPF object` job. Preserves the m234 `skip_ebpf` workflow_dispatch escape hatch. |
| `.github/workflows/ebpf-canary.yml` | Daily 06:00 UTC canary. Installs `latest` bpf-linker; auto-opens a deduped GitHub issue on failure. |
| `.github/workflows/pin-consistency.yml` | Guardrail — fails CI if a literal `--version <N.N.N>` re-inlines the pin outside the env file, or if `Dockerfile.ebpf-test`'s `ARG` default drifts from the env file. |
| `Dockerfile.ebpf-test` | Consumer — reads the pin via `ARG BPF_LINKER_VERSION`. CI passes `--build-arg BPF_LINKER_VERSION=$(...)` sourcing the env file. |
| `scripts/verify-ebpf.sh` | Contributor's un-pin readiness command. Runs the same build path release.yml uses; exits 0 iff the candidate version works. |
| `.github/workflows/nightly.yml` | NOT a consumer — it dispatches `release.yml` and inherits the pin transitively. Guarded by `pin-consistency.yml`. |

## How to bump the pin (upstream fix landed)

Say upstream ships a fixed `bpf-linker@0.12.1`:

```bash
# 1. Verify locally.
scripts/verify-ebpf.sh --version 0.12.1
# Expect: "verify-ebpf: PASS — bpf-linker v0.12.1 works end-to-end."

# 2. Edit the single source of truth.
sed -i.bak 's/^BPF_LINKER_VERSION=.*/BPF_LINKER_VERSION=0.12.1/' \
    .github/env/bpf-linker.env
rm .github/env/bpf-linker.env.bak  # macOS/BSD sed leaves a backup

# 3. Update the Dockerfile ARG default to match (pin-consistency guard requires this).
sed -i.bak 's/^ARG BPF_LINKER_VERSION=.*/ARG BPF_LINKER_VERSION=0.12.1/' \
    Dockerfile.ebpf-test
rm Dockerfile.ebpf-test.bak

# NOTE: no other files to edit. Both the composite action and
# `scripts/verify-ebpf.sh` read from `.github/env/bpf-linker.env`
# and download the pre-built binary directly.

# 4. Open the bump PR.
git checkout -b bump/bpf-linker-0.12.1
git add .github/env/bpf-linker.env Dockerfile.ebpf-test
git commit -m "chore(deps): bump bpf-linker to 0.12.1"
git push -u origin bump/bpf-linker-0.12.1
gh pr create --title "chore(deps): bump bpf-linker to 0.12.1"

# 5. Wait for CI. The ebpf-tracing lane MUST pass; pin-consistency MUST pass.
# 6. Merge. Next nightly + release will use 0.12.1.
```

## How to triage a canary failure

You see a GitHub issue titled `[canary] bpf-linker eBPF build regression`
with labels `canary`, `ebpf`, `regression`.

```bash
# Extract the version from the issue body.
# 1. Reproduce locally.
scripts/verify-ebpf.sh --version <version-from-issue>
# Expect: matching FAIL output.

# 2. File upstream.
# Open https://github.com/aya-rs/bpf-linker/issues/new
# Paste the FAIL log + a link to the canary run URL from the issue.

# 3. Track the upstream response.
# Add a comment on the waybill canary issue linking the upstream one.
# The canary will keep posting comments on subsequent runs; do NOT
# manually edit the issue body (dedupe uses title match).

# 4. Wait for either:
#    - Upstream fix released → bump the pin (see above).
#    - 30-day fallback window elapses without a fix → execute downstream
#      mitigation (see below).
```

## Downstream mitigation (fallback path, per spec.md FR-011)

If the 30-day fallback window expires without an upstream fix, execute
one of:

- **(a) Explicit LLVM install** — Add `sudo apt-get install -y llvm-<ver>`
  + `LD_LIBRARY_PATH` export to the composite action. (Not a
  Principle-I violation — LLVM is already a transitive runtime dep of
  bpf-linker's Rust bindings; this makes it explicit.)
- **(b) Pre-built binary** — Switch the composite action from
  `cargo install bpf-linker` to downloading + verifying a pre-built
  binary from bpf-linker's GH releases. Byte-copy; no toolchain
  invocation.
- **(c) Fork bpf-linker** — Fork the upstream Rust source, apply the
  fix, install from the fork. Pure Rust; still Principle-I compliant.

The executing PR MUST cite which option was taken and reaffirm
Principle-I compliance.

## The `skip_ebpf` escape hatch

If bpf-linker breaks in a way not yet mitigated and a release MUST ship
soon:

```bash
gh workflow run release.yml -f tag=v<X.Y.Z> -f skip_ebpf=true --ref main
```

The `skip_ebpf=true` input skips the `Build eBPF object` job entirely
and produces userspace-only tarballs. `waybill trace` won't be
available in the released binary, but `waybill sbom scan` etc. will
be. This is intended as an emergency escape hatch — not a routine
option. Always prefer fixing the underlying pin.

## Debugging tip: force the container path

Even on Linux, you can force `scripts/verify-ebpf.sh` to use the
container harness to reproduce CI's exact build environment:

```bash
scripts/verify-ebpf.sh --container --version 0.12.1
```

This matches what CI does when running the `Dockerfile.ebpf-test` layer
for integration tests.

## Cadence + scheduling

- **Canary**: daily 06:00 UTC (colocated with nightly cron so ops
  signals cluster in one wake window).
- **pin-consistency guard**: runs on every push to main + every PR
  that touches `.github/**`, `Dockerfile*`, or `scripts/**`.
- **Fallback window**: 30 calendar days from upstream-issue-open,
  tracked via a comment on the canary issue.

## References

- Spec: [`specs/234-fix-ebpf-linker-regression/spec.md`](../../specs/234-fix-ebpf-linker-regression/spec.md)
- Plan: [`specs/234-fix-ebpf-linker-regression/plan.md`](../../specs/234-fix-ebpf-linker-regression/plan.md)
- Contracts: [`specs/234-fix-ebpf-linker-regression/contracts/`](../../specs/234-fix-ebpf-linker-regression/contracts/)
- Upstream: <https://github.com/aya-rs/bpf-linker>
