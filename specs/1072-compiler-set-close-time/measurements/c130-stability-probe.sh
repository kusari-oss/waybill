#!/usr/bin/env bash
# Q9 — is C130's claim stable across two traces of one build?
#
# #1197 asks for one bounded experiment to decide whether the eBPF tier earns
# further investment. This captures the same `cargo build` twice and prints
# both attestations so the C130 payloads can be diffed.
#
# The split matters: `waybill sbom generate` takes an attestation *file*, so
# only the capture needs a kernel. Run this in the container, then emit and
# diff on any host -- which is also why this script prints the attestations
# rather than emitting SBOMs itself (the image's binary predates the #1197
# wiring; the host's does not).
#
# Traps held constant, all of them previously measured the wrong thing:
#   - target dir under /root/, never /tmp: m213's Ephemeral category drops
#     /tmp in-kernel (Q6).
#   - `--trace-children`, or cargo's child rustc invocations are never seen
#     and `compiler_pipeline` comes back with nothing in it.
#   - waybill runs from /waybill/waybill-ebpf: the loader resolves the eBPF
#     object relative to CWD, so the traced build is pointed elsewhere with
#     --manifest-path rather than by changing directory.
#   - `--offline` against the image's prepopulated registry, so the two runs
#     cannot differ by a network fetch.
#
# Set DEP=1 to add one dependency (`cfg-if`). Both shapes are worth running:
# the no-dependency build cannot show whether a dependency component's path
# ever lands in a write set, and inferring that instead of measuring it is
# what the first draft of Q9 got wrong.
#
# Usage, from the repository root:
#   docker build -f Dockerfile.ebpf-test \
#     --build-arg BPF_LINKER_VERSION=$(grep -E '^BPF_LINKER_VERSION=' \
#        .github/env/bpf-linker.env | cut -d= -f2) -t waybill-ebpf-test .
#   docker run --rm --privileged -i --entrypoint /bin/bash waybill-ebpf-test \
#     -s < specs/1072-compiler-set-close-time/measurements/c130-stability-probe.sh
#
# Each attestation is emitted to stdout as `###ATT:<tag>###` followed by one
# base64 line of gzip, because Colima's virtiofs does not share /private/tmp
# and a bind mount for the output silently fails.
#
# Measured 2026-10-14 on Colima aarch64, Ubuntu 24.04.4, kernel 6.8.0-136.
set -uo pipefail

WAYBILL=${WAYBILL:-/waybill/target/release/waybill}
DEP=${DEP:-0}

mkdir -p /sys/kernel/debug /sys/kernel/tracing
mount -t debugfs debugfs /sys/kernel/debug 2>/dev/null || true
mount -t tracefs tracefs /sys/kernel/tracing 2>/dev/null || true
# Without these, sched_process_exec/fork/exit and sys_enter_chdir fail to
# attach with "tracefs not found" and the trace aborts.
if [ ! -d /sys/kernel/tracing/events ]; then
  echo "FATAL: tracefs unavailable; rerun with --privileged" >&2
  exit 2
fi

mkdir -p /root/build/src
cd /root/build
{
  echo '[package]'
  echo 'name = "waybill-fixture-tracee"'
  echo 'version = "0.1.0"'
  echo 'edition = "2021"'
  echo
  echo '[dependencies]'
  [ "$DEP" = "1" ] && echo 'cfg-if = "1.0.4"'
} > Cargo.toml
cat > src/main.rs <<'RS'
fn main() { println!("{}", 2 + 2); }
RS

run_once() {
  local tag="$1"
  rm -rf /root/build/target
  echo "### trace $tag starting" >&2
  cd /waybill/waybill-ebpf
  "$WAYBILL" trace capture \
      --attestation-format waybill-v1 \
      --output "/root/att-$tag.json" \
      --trace-children \
      -- cargo build --offline \
         --manifest-path /root/build/Cargo.toml \
         --target-dir /root/build/target \
      >/dev/null 2>"/root/trace-$tag.err"
  echo "### trace $tag exit=$? size=$(wc -c < "/root/att-$tag.json" 2>/dev/null || echo 0)" >&2
  tail -3 "/root/trace-$tag.err" >&2
  if [ -f "/root/att-$tag.json" ]; then
    echo "###ATT:$tag###"
    gzip -c "/root/att-$tag.json" | base64 -w0
    echo
  fi
}

run_once a
run_once b
echo "###DONE###"
