#!/usr/bin/env bash
# Milestone 1070 research probes (#614). Run on a Linux host with Docker;
# measured on Colima aarch64, Ubuntu 24.04, kernel 6.8.0-136, 2 CPUs.
#
#   docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .
#   OUT=<dir under $HOME, shared with the VM> ./probe.sh
#
# 1. eBPF trace, default and widened (relative_paths.txt).
# 2. strace of the fixture and of a larger build (strace_summary.txt):
#    relative opens by base (AT_FDCWD vs a directory fd), chdir/fchdir,
#    spawn style, write-mode opens, renames and links of outputs.
set -euo pipefail
: "${OUT:?set OUT}"
FX=/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
run() { docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug -v "$OUT":/out --entrypoint bash waybill-ebpf-test -c "$1"; }
for wide in "" "--include-system-reads"; do
  run "cd /waybill && target/release/waybill trace capture $wide --attestation-format waybill-v1 --output /out/att${wide:+-wide}.json -- cargo build --release --manifest-path $FX/Cargo.toml --target-dir /tmp/fx"
done
run "apt-get update -qq && apt-get install -y -qq strace >/dev/null
cd /waybill
strace -f -qq -s 300 -e trace=chdir,fchdir,openat,openat2,open,creat,clone,clone3,vfork,execve -o /out/fixture.strace cargo build --release --manifest-path $FX/Cargo.toml --target-dir $FX/target
strace -f -qq -s 300 -e trace=chdir,fchdir,openat,openat2,open,creat,clone,clone3,vfork,execve -o /out/common.strace cargo build --release -p waybill-common --target-dir /tmp/wc
strace -f -qq -s 300 -e trace=rename,renameat,renameat2,link,linkat,openat,execve -o /out/fixture2.strace cargo build --release --manifest-path $FX/Cargo.toml --target-dir $FX/target2"
