#!/usr/bin/env bash
# m234 US3 — Un-pin readiness check for bpf-linker.
#
# Verify the eBPF build path (kernel-side object + userspace binary
# with --features ebpf-tracing) works with a given bpf-linker version.
# Exit 0 iff both build steps succeed.
#
# Contract: specs/234-fix-ebpf-linker-regression/contracts/verify-ebpf-script.md
# Spec:     specs/234-fix-ebpf-linker-regression/spec.md (FR-006, FR-007)

set -euo pipefail

# ---- CLI parsing -----------------------------------------------------------

usage() {
  cat <<'EOF'
Usage: scripts/verify-ebpf.sh [OPTIONS]

Verify the eBPF build path works with a given bpf-linker version.

Options:
  --version <VER>   Override the pinned bpf-linker version.
                    Default: value of BPF_LINKER_VERSION from
                    .github/env/bpf-linker.env.
  --container       Force use of Dockerfile.ebpf-test harness.
                    Default: auto-detect (Linux native if kernel is
                    Linux AND rustup is available; container path
                    otherwise).
  --help, -h        Print this help and exit.

Environment:
  WAYBILL_BPF_LINKER_VERSION   Same as --version (--version wins).

Examples:
  scripts/verify-ebpf.sh
  scripts/verify-ebpf.sh --version 0.12.0
  scripts/verify-ebpf.sh --container
EOF
}

version_override=""
force_container=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      shift
      [ "$#" -gt 0 ] || { echo "error: --version requires an argument" >&2; exit 2; }
      version_override="$1"
      ;;
    --version=*)
      version_override="${1#--version=}"
      ;;
    --container)
      force_container=1
      ;;
    --help|-h)
      usage; exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      echo "" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

# ---- Resolve target version -----------------------------------------------

repo_root=$(cd "$(dirname "$0")/.." && pwd)
env_file="$repo_root/.github/env/bpf-linker.env"

if [ -n "$version_override" ]; then
  target_version="$version_override"
elif [ -n "${WAYBILL_BPF_LINKER_VERSION:-}" ]; then
  target_version="$WAYBILL_BPF_LINKER_VERSION"
else
  if [ ! -f "$env_file" ]; then
    echo "error: $env_file not found — cannot resolve default version" >&2
    exit 1
  fi
  target_version=$(grep -E '^BPF_LINKER_VERSION=' "$env_file" | head -1 | cut -d= -f2)
  if [ -z "$target_version" ]; then
    echo "error: BPF_LINKER_VERSION not set in $env_file" >&2
    exit 1
  fi
fi

# ---- Path selection: native vs container ----------------------------------

is_linux=0
if [ "$(uname -s)" = "Linux" ]; then
  is_linux=1
fi

use_container=0
if [ "$force_container" -eq 1 ]; then
  use_container=1
elif [ "$is_linux" -eq 0 ]; then
  use_container=1
elif ! command -v rustup >/dev/null 2>&1; then
  # Linux but no rustup — fall through to container.
  use_container=1
fi

# BSD mktemp requires the X's at the very end (no suffix). Rename after.
log_file_base=$(mktemp "${TMPDIR:-/tmp}/verify-ebpf-XXXXXXXX")
log_file="${log_file_base}.log"
mv "$log_file_base" "$log_file"
trap 'rm -f "$log_file"' EXIT INT TERM

echo "verify-ebpf: target bpf-linker version = $target_version"
if [ "$use_container" -eq 1 ]; then
  echo "verify-ebpf: mode = container (Dockerfile.ebpf-test)"
else
  echo "verify-ebpf: mode = Linux native"
fi
echo "verify-ebpf: log = $log_file"
echo ""

# ---- Container path -------------------------------------------------------

if [ "$use_container" -eq 1 ]; then
  if ! command -v docker >/dev/null 2>&1; then
    cat <<EOF >&2
error: docker not available on \$PATH — install Docker Desktop or Colima
       to run the container-path verification.

       On macOS + Colima:
         brew install colima docker
         colima start
       On macOS + Docker Desktop:
         https://www.docker.com/products/docker-desktop/
       On Windows:
         https://docs.docker.com/desktop/install/windows-install/
EOF
    exit 1
  fi
  # docker info detects daemon status; give a clearer error if it's not running.
  if ! docker info >/dev/null 2>"$log_file"; then
    echo "error: docker daemon not reachable — start Docker Desktop or Colima and retry." >&2
    echo "       last docker info stderr:" >&2
    sed 's/^/       /' <"$log_file" >&2
    exit 1
  fi

  image_tag="waybill-ebpf-verify:${target_version//[^a-zA-Z0-9._-]/_}"
  echo "verify-ebpf: docker build --build-arg BPF_LINKER_VERSION=$target_version -t $image_tag ..."
  if docker build -f "$repo_root/Dockerfile.ebpf-test" \
      --build-arg "BPF_LINKER_VERSION=$target_version" \
      -t "$image_tag" \
      "$repo_root" 2>&1 | tee "$log_file"; then
    cat <<EOF

verify-ebpf: PASS
  Tool: bpf-linker
  Version: $target_version
  Mode: container (Dockerfile.ebpf-test)
  Image: $image_tag
EOF
    exit 0
  else
    cat <<EOF >&2

verify-ebpf: FAIL
  Tool: bpf-linker
  Version: $target_version
  Mode: container (Dockerfile.ebpf-test)
  Log: $log_file
  Next step: file upstream at https://github.com/aya-rs/bpf-linker/issues
             with this log attached.
EOF
    # Keep the log around for the operator by clearing the trap on FAIL.
    trap - EXIT INT TERM
    exit 1
  fi
fi

# ---- Linux native path ----------------------------------------------------

# Ensure nightly toolchain + rust-src are available.
if ! rustup toolchain list | grep -q '^nightly'; then
  echo "verify-ebpf: installing nightly toolchain (missing)..."
  rustup toolchain install nightly --profile minimal
fi
rustup component add rust-src --toolchain nightly >/dev/null 2>&1 || true

# Install bpf-linker at target version.
if [ "$target_version" = "latest" ]; then
  echo "verify-ebpf: cargo +nightly install bpf-linker --locked (latest, unpinned)..."
  install_ok=1
  cargo +nightly install bpf-linker --locked 2>&1 | tee "$log_file" || install_ok=0
else
  echo "verify-ebpf: cargo +nightly install bpf-linker --locked --version $target_version..."
  install_ok=1
  cargo +nightly install bpf-linker --locked --version "$target_version" 2>&1 | tee "$log_file" || install_ok=0
fi

if [ "$install_ok" -eq 0 ]; then
  installed_report=$(bpf-linker --version 2>/dev/null | awk '{print $2}' || echo "<install failed>")
  cat <<EOF >&2

verify-ebpf: FAIL — cargo install failed
  Tool: bpf-linker
  Version requested: $target_version
  Version currently on PATH: $installed_report
  Log: $log_file
  Next step: inspect the log; if it names LLVM path drift or a linker
             error, file upstream at https://github.com/aya-rs/bpf-linker
EOF
  trap - EXIT INT TERM
  exit 1
fi

installed_version=$(bpf-linker --version 2>&1 | awk '{print $2}')
echo "verify-ebpf: installed bpf-linker $installed_version"
echo ""

# Build eBPF kernel object.
echo "verify-ebpf: cargo run -p xtask -- ebpf..."
if ! (cd "$repo_root" && cargo run -p xtask -- ebpf) 2>&1 | tee -a "$log_file"; then
  cat <<EOF >&2

verify-ebpf: FAIL — eBPF object build failed
  Tool: bpf-linker
  Installed version: $installed_version
  Failing command: cargo run -p xtask -- ebpf
  Log: $log_file
  Next step: likely a bpf-linker regression. File upstream with this log.
EOF
  trap - EXIT INT TERM
  exit 1
fi
echo ""

# Build userspace binary.
echo "verify-ebpf: cargo build --release --features ebpf-tracing --bin waybill..."
if ! (cd "$repo_root" && cargo build --release --features ebpf-tracing --bin waybill) 2>&1 | tee -a "$log_file"; then
  cat <<EOF >&2

verify-ebpf: FAIL — userspace build failed
  Tool: bpf-linker
  Installed version: $installed_version
  Failing command: cargo build --release --features ebpf-tracing --bin waybill
  Log: $log_file
EOF
  trap - EXIT INT TERM
  exit 1
fi

cat <<EOF

verify-ebpf: PASS — bpf-linker v$installed_version works end-to-end.
  Kernel-side build: cargo run -p xtask -- ebpf
  Userspace build:   cargo build --release --features ebpf-tracing --bin waybill
EOF
