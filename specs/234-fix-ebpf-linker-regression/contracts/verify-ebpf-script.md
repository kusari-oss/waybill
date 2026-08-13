# Contract: `verify-ebpf.sh` — Un-Pin Readiness Command

**File**: `scripts/verify-ebpf.sh`
**Type**: POSIX shell script (Bash-compatible, matches
`scripts/pre-pr.sh` style)

---

## Interface

```text
Usage: scripts/verify-ebpf.sh [OPTIONS]

Verify the eBPF build path works with a given bpf-linker version.
Exit 0 iff both the kernel-side eBPF object and the userspace binary
build cleanly.

Options:
  --version <VER>   Override the pinned bpf-linker version.
                    Default: value of BPF_LINKER_VERSION from
                    .github/env/bpf-linker.env.
  --container       Force use of Dockerfile.ebpf-test harness.
                    Default: auto-detect (Linux native if /proc/version
                    exists AND rustup is available; otherwise
                    container).
  --help            Print this help and exit 0.

Environment variables:
  WAYBILL_BPF_LINKER_VERSION   Same as --version (--version wins if both set).

Examples:
  # Verify current pin works on this host
  scripts/verify-ebpf.sh

  # Verify a candidate un-pin version
  scripts/verify-ebpf.sh --version 0.12.0

  # Force container path even on Linux
  scripts/verify-ebpf.sh --container
```

## Contract: Linux native path (auto-detected OR `--container` absent on Linux)

1. Verify prerequisites:
   - `rustup` is on PATH — else fail with
     `error: rustup not found — install from https://rustup.rs`.
   - `nightly` toolchain installed OR installable — else attempt
     `rustup toolchain install nightly --profile minimal` and
     `rustup component add rust-src --toolchain nightly`.

2. Read `BPF_LINKER_VERSION`:
   - If `--version` provided → use that.
   - Else if `WAYBILL_BPF_LINKER_VERSION` set → use that.
   - Else read `.github/env/bpf-linker.env`.
   - Else fail with clear message.

3. Install bpf-linker at target version:
   - `cargo +nightly install bpf-linker --locked --version <version>`
   - (Special case: `--version latest` runs `cargo install --locked`
     without `--version`.)

4. Run kernel-side build:
   - `cargo run -p xtask -- ebpf`
   - On failure: exit 1 with
     `error: eBPF object build failed with bpf-linker v<installed-version> — likely bpf-linker regression`.

5. Run userspace build:
   - `cargo build --release --features ebpf-tracing --bin waybill`
   - On failure: exit 1 with
     `error: waybill userspace build failed with bpf-linker v<installed-version>`.

6. Exit 0 on both success. Print:
   `verify-ebpf: PASS — bpf-linker v<installed-version> works end-to-end.`

## Contract: Container path (`--container` explicit OR macOS/Windows)

1. Verify Docker or Colima is available:
   - `docker info` succeeds — else fail with
     `error: docker not available — install Docker Desktop or Colima`.

2. Build the harness with the target version as build-arg:
   - `docker build -f Dockerfile.ebpf-test --build-arg BPF_LINKER_VERSION=<version> -t waybill-ebpf-verify:<version> .`

3. Exit 0 on successful build. Print:
   `verify-ebpf (container): PASS — bpf-linker v<version> builds under Dockerfile.ebpf-test.`

4. Exit 1 on any docker build failure with the full log tail.

## Failure output contract (both paths)

The failure message MUST:

1. Name the tool (`bpf-linker`) explicitly.
2. Include the installed version string.
3. Include the failing command.
4. Point at the log capture location (either printed inline or
   referenced by path).

Example FAIL output:

```
verify-ebpf: FAIL
  Tool: bpf-linker
  Installed version: 0.11.0
  Failing command: cargo run -p xtask -- ebpf
  Root-cause hint: LLVM library path drift (`-lLLVM` unresolved)
  Log: /tmp/verify-ebpf-<timestamp>.log
  Next step: file upstream at https://github.com/aya-rs/bpf-linker/issues
             with this log attached.
```

## Portability

- **Bash-only features**: MAY use `[[` and process substitution
  (matches `scripts/pre-pr.sh` conventions).
- **No non-standard tools**: `docker`, `rustup`, `cargo`, `grep`, `sed`,
  `mkdir`, `mktemp`. All present by default on macOS + Linux + WSL.
- **Windows native**: intentionally unsupported (eBPF is Linux-only;
  users on Windows use the container path via Docker Desktop).

## Idempotence

- Running the script twice with the same `--version` on the same host
  should be a no-op after the first install (the `cargo install`
  step is idempotent via the "already installed" check).
- Container-path builds hit Docker's layer cache; identical repeated
  invocations complete in ~30 seconds after the first.
