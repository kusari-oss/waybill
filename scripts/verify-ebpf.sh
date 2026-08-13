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

# Install bpf-linker at target version — pre-built binary path.
# Matches .github/actions/install-bpf-linker/action.yml default
# (install-method: binary). Rationale: from v0.11.0 upstream, cargo
# install requires system LLVM 22 which most hosts don't ship; the
# pre-built musl binary statically links LLVM 22 in.

os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
  Linux-x86_64|Linux-amd64)    triple="x86_64-unknown-linux-musl" ;;
  Linux-aarch64|Linux-arm64)   triple="aarch64-unknown-linux-musl" ;;
  Darwin-x86_64)               triple="x86_64-apple-darwin" ;;
  Darwin-arm64|Darwin-aarch64) triple="aarch64-apple-darwin" ;;
  *)
    echo "error: unsupported host platform $os-$arch for pre-built bpf-linker binary" >&2
    exit 1
    ;;
esac

resolve_version="$target_version"
if [ "$resolve_version" = "latest" ]; then
  echo "verify-ebpf: resolving 'latest' via GitHub API..."
  tag=$(curl -sSL --retry 3 \
      -H "Accept: application/vnd.github+json" \
      https://api.github.com/repos/aya-rs/bpf-linker/releases/latest \
      | grep '"tag_name"' | head -1 | cut -d'"' -f4)
  if [ -z "$tag" ]; then
    echo "error: failed to resolve 'latest' tag from GitHub API" >&2
    exit 1
  fi
  resolve_version="${tag#v}"
  echo "verify-ebpf: 'latest' → v${resolve_version}"
fi

url="https://github.com/aya-rs/bpf-linker/releases/download/v${resolve_version}/bpf-linker-${triple}.tar.zst"
echo "verify-ebpf: downloading pre-built bpf-linker v${resolve_version} for ${triple}..."

if ! command -v zstd >/dev/null 2>&1; then
  echo "error: zstd not on PATH — install via 'brew install zstd' or 'apt install zstd'" >&2
  exit 1
fi

install_dir="${HOME}/.cargo/bin"
mkdir -p "$install_dir"
tmp_archive=$(mktemp "${TMPDIR:-/tmp}/bpf-linker-XXXXXXXX")
trap 'rm -f "$tmp_archive"' EXIT INT TERM

install_ok=1
if ! curl -sSL --retry 3 --fail -o "$tmp_archive" "$url" 2>&1 | tee "$log_file"; then
  install_ok=0
fi
if [ "$install_ok" -eq 1 ]; then
  if ! tar --zstd -xf "$tmp_archive" -C "$install_dir" bpf-linker 2>&1 | tee -a "$log_file"; then
    install_ok=0
  fi
fi
if [ "$install_ok" -eq 1 ]; then
  chmod +x "$install_dir/bpf-linker"
fi

if [ "$install_ok" -eq 0 ]; then
  installed_report=$(bpf-linker --version 2>/dev/null | awk '{print $2}' || echo "<install failed>")
  cat <<EOF >&2

verify-ebpf: FAIL — pre-built binary install failed
  Tool: bpf-linker
  Version requested: $resolve_version
  Platform: $triple
  URL: $url
  Version currently on PATH: $installed_report
  Log: $log_file
  Next step: check that the release tag + platform triple exist at
             https://github.com/aya-rs/bpf-linker/releases/tag/v${resolve_version}
EOF
  trap - EXIT INT TERM
  exit 1
fi

# The pre-built binary reports "bpf-linker 0.0.0" (upstream quirk —
# release-build doesn't propagate the crate version). Use the resolved
# request version instead.
installed_version="$resolve_version"
echo "verify-ebpf: installed bpf-linker $installed_version (pre-built binary)"
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
