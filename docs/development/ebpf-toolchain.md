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

### First: read the title (m896)

The canary opens one of **two** issues, and which one it is tells you who can
act. Check the title before anything else:

| Title | Meaning | Who fixes it |
|---|---|---|
| `[canary] bpf-linker eBPF build regression` | The pinned version built in that same run; the newer one did not. | Upstream — `aya-rs/bpf-linker` |
| `[canary] the eBPF canary cannot run` | The canary's own environment failed. The watched version went **untested**. | Us, in this repo |

The distinction is decided by a **control build against the pinned version**,
not by which step failed or by matching error text. Both of those mis-classify
the failure this mechanism was built after: it occurred at the build step —
which position alone would call upstream's — and was caused by a missing
toolchain component in the canary's own job. See
`specs/896-fix-ebpf-canary/` for the full account; the short version is that
36 consecutive runs reported a canary fault as an upstream regression.

A canary-fault report never tells you to file upstream, and says explicitly
that nothing was learned about the watched version in either direction. Do not
read it as the component being healthy.

### If the title says the component regressed

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
#    - The 30-day window elapses → execute downstream mitigation (below).
```

### If the title says the canary cannot run

```bash
# 1. Read the failing step and error excerpt in the issue body.
#    Both are included; you should not need to re-run anything to form
#    a hypothesis.

# 2. Compare this job against ci.yml's eBPF lane, which builds
#    successfully. The canary asserts that parity itself — the
#    "Assert toolchain parity with ci.yml" step asks rustup whether the
#    components ci.yml declares are actually installed here.

# 3. Verify a fix by dispatching against the PINNED version, which is
#    known to build:
gh workflow run ebpf-canary.yml -f version=<pin> -f dry_run=true
```

## The escalation clock

The 30-day window runs from **the streak's first failure** — concretely, the
`created_at` of the open report for that kind of failure. It starts on its own.

This is deliberate and it is a correction. The clock used to be gated on
upstream being *unresponsive*, which presupposes that somebody filed upstream.
On #685 nobody did, so by that reading the clock never started: 35 days passed
with escalation technically not yet due while the canary reported the same
non-information every night. A window that can be stalled by inaction is not a
window.

A change of cause opens a different issue and therefore starts a new clock — a
canary-fault streak never inherits an upstream-regression streak's age.

## Downstream mitigation (fallback path, per spec.md FR-011)

If the 30-day window expires without an upstream fix, execute one of:

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
