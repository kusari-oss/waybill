# Phase 1 Data Model: Durable eBPF Build Resilience

**Feature**: `234-fix-ebpf-linker-regression`
**Date**: 2026-08-12
**Note**: This milestone has no runtime data model (it modifies CI
infrastructure, not the waybill binary). The "entities" below are the
configuration + workflow artifacts and their fields/relationships.

---

## Entity: `BpfLinkerPin`

**File**: `.github/env/bpf-linker.env`
**Format**: shell-style `KEY=VALUE` lines (parseable by `source`,
`docker --build-arg`, and GitHub Actions `env-file` conventions).

**Fields**:

| Field | Type | Required | Description |
|---|---|---|---|
| `BPF_LINKER_VERSION` | semver string | yes | Pinned version passed to `cargo install bpf-linker --version <this>`. Current value: `0.10.4`. |
| `BPF_LINKER_UPSTREAM_ISSUE` | URL string | no | Link to the tracking issue we opened upstream for the pin (per FR-011 upstream-first posture). Populated when US1 opens the issue. |
| `BPF_LINKER_FALLBACK_DEADLINE` | ISO-8601 date | no | Date by which the downstream mitigation executes if upstream hasn't fixed the issue. Set by US1 to `open_date + 30 days` per Phase 0 R2. |

**Validation rules**:

- `BPF_LINKER_VERSION` MUST parse as a valid semver `X.Y.Z` (no `latest`,
  no ranges). Canary use case handles `latest` via the composite action's
  `version` input override, not this file.
- Comments allowed on lines starting with `#`.

**Lifecycle**:

- **Created**: at m234 landing time with the current pin.
- **Updated**: single-line edit whenever the pin bumps (US1 bump PR).
- **Read by**: composite action (default), Dockerfile (build-arg),
  developer scripts.

---

## Entity: `InstallBpfLinkerAction`

**File**: `.github/actions/install-bpf-linker/action.yml`
**Type**: GitHub Actions composite action.

**Inputs**:

| Input | Type | Default | Description |
|---|---|---|---|
| `version` | string | reads `BPF_LINKER_VERSION` from `.github/env/bpf-linker.env` | Target bpf-linker version. Canary passes `latest` for unpinned installs. |
| `toolchain` | string | `nightly` | Rust toolchain to install bpf-linker under. |

**Behavior**:

1. `rustup toolchain install ${toolchain} --profile minimal`
2. `rustup component add rust-src --toolchain ${toolchain}`
3. If `version == latest`: `cargo +${toolchain} install bpf-linker --locked`
4. Else: `cargo +${toolchain} install bpf-linker --locked --version ${version}`
5. Emit `installed-version` output containing the actual installed
   version (via `cargo pkgid` or `bpf-linker --version`).

**Consumers** (existing install sites, all rewired):

- `.github/workflows/ci.yml` → `Install bpf-linker` step in the
  `ebpf-tracing` lane.
- `.github/workflows/release.yml` → `Install bpf-linker` step in
  `Build eBPF object` job.
- `.github/workflows/ebpf-canary.yml` → NEW consumer (passes
  `version: latest`).

`.github/workflows/nightly.yml` is NOT a consumer — it's a dispatcher
that shells out to `release.yml` via `gh workflow run` and inherits
the pin transitively. Verified 2026-08-12: zero bpf-linker references
in nightly.yml.

---

## Entity: `EbpfCanaryWorkflow`

**File**: `.github/workflows/ebpf-canary.yml`

**Triggers**:

- `schedule` — `cron: "0 6 * * *"` (daily 06:00 UTC per R1)
- `workflow_dispatch` — with inputs:
  - `version` (default `latest`) — bpf-linker version to test
  - `dry_run` (default `false`) — skip issue create/update

**Jobs**:

- `canary` on `ubuntu-latest`:
  1. Checkout repo (pinned commit).
  2. Install Rust toolchain (nightly + stable).
  3. Use composite action with `version: ${{ inputs.version || 'latest' }}`.
  4. `cargo run -p xtask -- ebpf` — build eBPF object.
  5. `cargo build --release --features ebpf-tracing` — build userspace.
  6. Determine outcome; if failure → invoke `report-failure` step.

**Failure reporting** (implements FR-003 / FR-004 / FR-005):

- Uses `actions/github-script@v7` to search for existing open issues
  with the exact title `[canary] bpf-linker eBPF build regression`.
- If issue exists: append a comment with new run details (installed
  version, failing command, timestamp, run URL).
- If issue does NOT exist: create new one with body containing all
  five FR-required fields (bpf-linker version, install command that
  failed, first 200 lines of the failing log, run URL, `cc @maintainers`
  mention if configured).
- Labels: `canary`, `ebpf`, `regression`.

**Success behavior**:

- If issue exists AND canary just went green: post a closing comment
  ("canary green as of <date>; closing"), close the issue.
- If issue does NOT exist: no side effects (no spam on repeated
  successes).

---

## Entity: `VerifyEbpfScript`

**File**: `scripts/verify-ebpf.sh`

**Interface**:

```text
Usage: scripts/verify-ebpf.sh [--version <ver>] [--container]
Options:
  --version <ver>   Override the pinned bpf-linker version (default: value from .github/env/bpf-linker.env)
  --container       Force use of Dockerfile.ebpf-test harness (default: auto-detect based on OS)
  --help            Print this help
```

**Behavior** (Linux native, when `--container` not passed):

1. Read `BPF_LINKER_VERSION` from `.github/env/bpf-linker.env` or the
   `--version` override.
2. Install (or reuse) `bpf-linker` via `cargo install bpf-linker --locked --version <ver>`.
3. Run `cargo run -p xtask -- ebpf` — verify kernel-side build.
4. Run `cargo build --release --features ebpf-tracing --bin waybill` —
   verify userspace + linker resolution.
5. Exit 0 on both success; exit non-zero with a diagnostic message
   citing which step failed AND the bpf-linker version.

**Behavior** (macOS/Windows OR `--container` explicit):

1. Verify Docker/Colima is available (fail with clear message if not).
2. `docker build -f Dockerfile.ebpf-test --build-arg BPF_LINKER_VERSION=<ver> .`
3. Exit 0 on successful build; exit non-zero with the docker build
   log on failure.

**State**: none persistent. Any interim files (built binaries,
downloaded crates) live in `$CARGO_TARGET_DIR` (per developer's
cargo config) or docker layer cache.

---

## Relationships

```
.github/env/bpf-linker.env
        │
        ├─── read by ─── .github/actions/install-bpf-linker/action.yml (composite)
        │                                    │
        │                                    ├─── used by ─── ci.yml (ebpf-tracing lane)
        │                                    ├─── used by ─── release.yml (Build eBPF object)
        │                                    └─── used by ─── ebpf-canary.yml (version=latest override)
        │                                    (nightly.yml inherits transitively via release.yml dispatch)
        │
        ├─── read as build-arg by ─── Dockerfile.ebpf-test
        │
        └─── read by ─── scripts/verify-ebpf.sh (default value)
```

**Invariant**: whenever the `BPF_LINKER_VERSION` line in
`.github/env/bpf-linker.env` changes, every consumer above sees the
new value on their next run without further edits. That is the
"single source of truth" the spec's FR-001 requires.
