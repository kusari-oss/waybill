# Contract: `install-bpf-linker` Composite Action

**File**: `.github/actions/install-bpf-linker/action.yml`
**Type**: GitHub Actions composite action
**Consumers**: `ci.yml`, `release.yml`, `ebpf-canary.yml` (`nightly.yml` inherits the pin transitively via its `release.yml` dispatch and is not a direct consumer — verified 2026-08-12)

---

## Inputs

```yaml
inputs:
  version:
    description: 'bpf-linker version to install. Default: value of BPF_LINKER_VERSION from .github/env/bpf-linker.env. Special value "latest" installs unpinned (used by canary).'
    required: false
    default: ''            # empty string = "read from env file"
  toolchain:
    description: 'Rust toolchain channel to install bpf-linker under.'
    required: false
    default: 'nightly'
```

## Outputs

```yaml
outputs:
  installed-version:
    description: 'Actual bpf-linker version installed (as reported by `bpf-linker --version`).'
```

## Contract

1. **Env-file lookup**: If `inputs.version` is empty, the action MUST
   read `.github/env/bpf-linker.env` and extract the
   `BPF_LINKER_VERSION` value. If the file is missing or the key is
   absent, the action MUST fail with a clear message pointing at the
   env file.

2. **Special-value `latest`**: If `inputs.version == 'latest'`, the
   action MUST run `cargo install bpf-linker --locked` (no `--version`
   flag). This is the canary use case.

3. **Explicit version**: For any other `inputs.version` value, the
   action MUST run `cargo install bpf-linker --locked --version <that value>`.

4. **Locked installs**: The action MUST always pass `--locked` to
   `cargo install`. This is the guardrail against transitive-dep
   drift causing false-positive canary failures.

5. **Toolchain**: The action MUST install `${inputs.toolchain}` via
   rustup with the `minimal` profile and add the `rust-src` component
   before installing bpf-linker.

6. **Idempotent**: If bpf-linker is already installed at the requested
   version (checked via `cargo install --list`), the action MUST skip
   the install step and emit an INFO log line.

7. **Output**: The action MUST emit `installed-version` derived from
   `bpf-linker --version`. Consumers MAY assert against this in their
   own steps (e.g., the canary verifies that `installed-version !=
   BPF_LINKER_VERSION` when it passed `version: latest`).

8. **No mutation of `.github/env/bpf-linker.env`**: The action MUST
   NOT rewrite the env file at any point. Bumps happen only via
   explicit PR edits.

## Failure modes

| Condition | Expected behavior |
|---|---|
| `.github/env/bpf-linker.env` missing | Fail with `error: .github/env/bpf-linker.env not found — pin file missing` |
| `BPF_LINKER_VERSION` key missing in env file | Fail with `error: BPF_LINKER_VERSION not set in .github/env/bpf-linker.env` |
| `cargo install` fails | Bubble up the failure verbatim; the canary's `report-failure` step catches it and files the issue. |
| `rustup` step fails | Bubble up (runner-image regression; not bpf-linker's fault). |

## Compatibility

- Runs on `ubuntu-latest`, `ubuntu-24.04`, `ubuntu-22.04` (whichever
  the calling workflow selects). No macOS/Windows support intended
  (eBPF is Linux-only per Constitution Principle I ecosystem).
- Requires network access to crates.io.
- Requires ~200 MB disk for the LLVM linkage bpf-linker pulls in.
