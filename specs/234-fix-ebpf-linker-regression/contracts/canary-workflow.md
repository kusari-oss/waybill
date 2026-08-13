# Contract: eBPF Canary Workflow

**File**: `.github/workflows/ebpf-canary.yml`
**Type**: GitHub Actions workflow (scheduled + workflow_dispatch)

---

## Triggers

```yaml
on:
  schedule:
    - cron: '0 6 * * *'    # Daily at 06:00 UTC (colocated with nightly.yml)
  workflow_dispatch:
    inputs:
      version:
        description: 'bpf-linker version to test (default: latest)'
        required: false
        default: 'latest'
      dry_run:
        description: 'If true, skip issue create/update on failure'
        required: false
        type: boolean
        default: false
```

## Job: `canary`

- Runs on `ubuntu-latest`.
- Timeout: 15 minutes (generous; typical run ~5 min).
- Steps:
  1. `actions/checkout@<SHA-pinned>` (deep clone not required).
  2. `dtolnay/rust-toolchain@stable` — stable for user-space.
  3. Composite action `./.github/actions/install-bpf-linker` with
     `version: ${{ inputs.version || 'latest' }}` and
     `toolchain: nightly`.
  4. `cargo run -p xtask -- ebpf` — kernel-side eBPF object build.
  5. `cargo build --release --features ebpf-tracing --bin waybill` —
     user-space build with feature on.
  6. `report-failure` step (`if: failure() && !inputs.dry_run`) —
     invokes the issue create/update helper.
  7. `report-success` step (`if: success()`) — closes any open
     canary-tracked issue by title match.

## Contract: Issue creation / update (`report-failure`)

**Search key**: exact title match on
`[canary] bpf-linker eBPF build regression`. Search scope: repo issues,
state=open, label=canary.

### If matching open issue exists

- MUST post a new comment (not edit the body) containing:
  ```
  ## Canary run <run_id> — <timestamp UTC>

  Installed bpf-linker version: <version>
  Failing step: <step-name>
  Failing command: <command>

  Log tail (last 200 lines): <collapsible summary>
  Run URL: <run_url>
  ```
- MUST NOT change the issue title or labels.
- MUST NOT reopen a closed issue (open a new one instead — see below).

### If NO matching open issue exists

- MUST create issue with:
  - **Title**: `[canary] bpf-linker eBPF build regression`
  - **Labels**: `canary`, `ebpf`, `regression`
  - **Body**:
    ```markdown
    The eBPF-build canary at .github/workflows/ebpf-canary.yml has
    detected a regression in the bpf-linker install / build path.

    ## Details

    - Installed bpf-linker version: <version>
    - Runner OS: <runner_os> (<runner_image>)
    - Failing step: <step-name>
    - Failing command: <command>
    - Run URL: <run_url>

    ## Log tail (last 200 lines)

    <collapsible details>

    ## Next steps

    1. Reproduce locally: `scripts/verify-ebpf.sh --version <version>`
    2. If confirmed, file upstream at aya-rs/bpf-linker with this log.
    3. Track upstream response; if unresponsive within the
       [FR-011 fallback window](../specs/234-fix-ebpf-linker-regression/spec.md)
       (30 days per Phase 0 R2), execute downstream mitigation.

    ---
    _This issue was auto-opened by the eBPF canary. Comments will be
    appended by subsequent canary runs. Do not manually edit the
    body — canary uses title match for dedupe._
    ```

## Contract: Success behavior (`report-success`)

- If a matching open issue exists AND the canary succeeded:
  - MUST post a closing comment:
    `Canary green as of <timestamp> (run <run_url>). Closing.`
  - MUST close the issue with reason `completed`.
- If no matching issue exists AND canary succeeded: no-op (no
  side effects on repeated success).

## Failure modes

| Condition | Expected behavior |
|---|---|
| Canary fails during cargo install → cargo build | Issue create/update per FR-003 |
| Canary itself has a bug (e.g., helper step fails) | Workflow status shows failure; no auto-issue (helper couldn't run) — maintainer notices via workflow-status email |
| `latest` bpf-linker install times out on network | Issue opened with `installed-version: <unknown, install timed out>` — accepted noise; better than missing a real regression |

## Compatibility

- Runs only on `ubuntu-latest` (canary purpose is to test the same OS
  the release + CI run on).
- Requires `GITHUB_TOKEN` with `issues: write` — the default token has
  this, no PAT needed.
- Runs regardless of whether the repo has any open PRs (canary is
  time-driven, not push-driven).
