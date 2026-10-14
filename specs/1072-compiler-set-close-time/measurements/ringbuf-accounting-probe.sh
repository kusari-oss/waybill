#!/usr/bin/env bash
# Q8 — is trace event loss fully accounted for by the integrity counters?
#
# Q7 (findings.md) found that two traces of one build observe different
# amounts while both integrity counters read zero, and #1185 was filed from
# it naming the userspace drain window (#618) as the leading candidate. This
# probe tests that hypothesis against a workload with EXACT ground truth
# instead of run-to-run comparison.
#
# Ground truth: a C control opens N distinct pre-created files. Every open
# fires both the openat2 and do_filp_open kprobes, so the trace must record
# 2*N events. Paths live under /root/probe/ because that prefix matches none
# of the 19 m213 filter patterns -- /etc /proc /sys /dev /lib /usr /bin /var
# /tmp by prefix, and .cargo/c, Cargo.to, Cargo.lo, -toolcha, /fingerp,
# /.finger, /increme, /.cache/, /.local/ by contains-scan. An earlier draft
# used /etc/hostname, a CAT_SYSTEM prefix, and measured zero for the wrong
# reason -- the trap #1185 documents in its own reproduction notes.
#
# ASSERTIONS, in both directions:
#   A1. recorded <= expected. The trace cannot record more probe-path opens
#       than the control issued.
#   A2. recorded + ring_buffer_overflows >= expected. Every probe-path event
#       that went missing is covered by the overflow counter, so no loss is
#       undisclosed.
#       Note this is an INEQUALITY on purpose. ring_buffer_overflows is a
#       GLOBAL count of reserve failures, while `recorded` is filtered to
#       /root/probe/, so the two are not commensurable and exact equality is
#       not expected: the control's own loader and libc opens are also
#       eligible to be dropped. An earlier revision asserted equality and
#       passed twice by luck -- those non-probe events occur at trace start,
#       before the buffer fills -- then failed by 29 events on the third run.
#       The slack is the honest form of the claim.
#   B. The overflow scenario MUST report ring_buffer_overflows > 0.
#      A probe that never observes loss cannot distinguish a sound pipeline
#      from a blind one, and would pass vacuously -- see the "zero result can
#      mean broken, not clean" lesson behind #1050.
#
# Exits 0 iff every assertion holds. Requires root + eBPF (CAP_BPF,
# CAP_PERFMON) and a mounted tracefs, so it does not run in ordinary CI;
# `waybill trace` is experimental and this probe is on-demand evidence, not
# a merge gate.
#
# Usage, from the repository root:
#   docker build -f Dockerfile.ebpf-test \
#     --build-arg BPF_LINKER_VERSION=$(grep -E '^BPF_LINKER_VERSION=' \
#        .github/env/bpf-linker.env | cut -d= -f2) -t waybill-ebpf-test .
#   docker run --rm --privileged -i --entrypoint /bin/bash waybill-ebpf-test \
#     -s < specs/1072-compiler-set-close-time/measurements/ringbuf-accounting-probe.sh
#
# Measured 2026-10-14 on Colima aarch64, Ubuntu 24.04.4, kernel 6.8.0-136.
set -uo pipefail

WAYBILL=${WAYBILL:-/waybill/target/release/waybill}
fail=0

mkdir -p /sys/kernel/debug /sys/kernel/tracing
mount -t debugfs debugfs /sys/kernel/debug 2>/dev/null || true
mount -t tracefs tracefs /sys/kernel/tracing 2>/dev/null || true
# Without these, sched_process_exec/fork/exit and sys_enter_chdir fail to
# attach with "tracefs not found" and the trace aborts.
if [ ! -d /sys/kernel/tracing/events ]; then
  echo "FATAL: tracefs unavailable; rerun with --privileged" >&2
  exit 2
fi

cat > /tmp/ctl.c <<'C'
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
/* argv: n make parallelism */
int main(int argc, char **argv) {
    if (argc < 3) return 2;
    long n = atol(argv[1]);
    int mk = atoi(argv[2]);
    int par = argc > 3 ? atoi(argv[3]) : 1;
    char p[256];
    if (mk) {
        mkdir("/root/probe", 0755);
        for (long i = 0; i < n; i++) {
            snprintf(p, sizeof p, "/root/probe/f%06ld", i);
            int fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0644);
            if (fd >= 0) { write(fd, "x", 1); close(fd); }
        }
        return 0;
    }
    if (par > 1) {
        for (int k = 0; k < par; k++)
            if (fork() == 0) {
                for (long i = 0; i < n; i++) {
                    snprintf(p, sizeof p, "/root/probe/f%06ld", i);
                    int fd = open(p, O_RDONLY);
                    if (fd >= 0) close(fd);
                }
                _exit(0);
            }
        for (int k = 0; k < par; k++) wait(NULL);
        return 0;
    }
    for (long i = 0; i < n; i++) {
        snprintf(p, sizeof p, "/root/probe/f%06ld", i);
        int fd = open(p, O_RDONLY);
        if (fd >= 0) close(fd);
    }
    return 0;
}
C
gcc -O2 -o /tmp/ctl /tmp/ctl.c || { echo "FATAL: control did not compile" >&2; exit 2; }
/tmp/ctl 100000 1
echo "setup: $(ls /root/probe | wc -l) files created (untraced)"

printf '\n%-30s %-10s %-10s %-10s %-10s %s\n' SCENARIO EXPECTED RECORDED DROPPED OVERFLOWS ACCOUNTED

# run <label> <ring_bytes> <n> <parallelism> <expect_overflow:0|1>
run() {
  local lbl=$1 rb=$2 n=$3 par=$4 want_ovf=$5
  "$WAYBILL" trace capture --attestation-format waybill-v1 \
      --ring-buffer-size "$rb" --output /tmp/a.json \
      -- /tmp/ctl "$n" 0 "$par" >/dev/null 2>&1
  if [ ! -f /tmp/a.json ]; then
    printf '%-30s %s\n' "$lbl" "NO ATTESTATION"; fail=1; return
  fi
  local exp rec ed rbo id
  exp=$(( n * par * 2 ))
  rec=$(jq '[.predicate.file_access.operations[]? | select(.path|test("/root/probe/"))] | length' /tmp/a.json)
  ed=$(jq -r '.predicate.trace_integrity.events_dropped' /tmp/a.json)
  rbo=$(jq -r '.predicate.trace_integrity.ring_buffer_overflows' /tmp/a.json)

  # Assertions A1 + A2: nothing recorded that was not issued, and every
  # missing probe-path event is covered by the counters. See the header for
  # why A2 is an inequality rather than an equality.
  id="ok"
  if [ "$rec" -gt "$exp" ]; then
    id="A1-VIOLATED"; fail=1
    echo "  A1 FAILED: recorded ($rec) exceeds issued ($exp) for '$lbl'" >&2
  elif [ $(( rec + rbo + ed )) -lt "$exp" ]; then
    id="A2-VIOLATED"; fail=1
    echo "  A2 FAILED: '$lbl' lost $(( exp - rec - rbo - ed )) probe-path events" >&2
    echo "  that no counter accounts for. This is the undisclosed-loss shape #1185 describes." >&2
  fi
  printf '%-30s %-10s %-10s %-10s %-10s %s\n' "$lbl" "$exp" "$rec" "$ed" "$rbo" "$id"

  # Assertion B: the positive control must actually lose events.
  if [ "$want_ovf" = "1" ] && [ "$rbo" -eq 0 ]; then
    echo "  ASSERTION B FAILED: '$lbl' is the positive control and must report" >&2
    echo "  ring_buffer_overflows > 0. Reporting zero means this probe cannot" >&2
    echo "  observe loss at all, so its clean rows prove nothing." >&2
    fail=1
  fi
  rm -f /tmp/a.json
}

# Clean rows: the drain is expected to keep up, so loss must be zero AND
# the identity must hold trivially.
run "seq      N=100k rb=64KiB"   65536    100000 1 0
run "seq      N=100k rb=64MiB"   67108864 100000 1 0
run "par=8    N=15k  rb=64MiB"   67108864 15000  8 0
# Positive control: 1.6M events from 8 producers outruns the drain, the
# buffer stays full, and the kernel drops at reserve.
run "par=8    N=100k rb=8MiB"    8388608  100000 8 1

echo
if [ "$fail" -eq 0 ]; then
  echo "PASS — every scenario accounted for all events, and the positive control lost some."
else
  echo "FAIL — see assertions above." >&2
fi
exit "$fail"
