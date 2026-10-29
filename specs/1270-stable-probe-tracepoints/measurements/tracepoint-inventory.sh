#!/bin/bash
# Which syscall tracepoints does this kernel offer, and where do their
# arguments sit?
#
# The feature replaces two kprobes on internal kernel functions with
# syscall tracepoints. Two things have to be true for that to work, and
# neither is knowable by reading documentation:
#
#   1. The tracepoints this capture needs exist on the kernels the
#      project runs on — including the architectures, where the
#      open family differs.
#   2. The argument offsets the kernel-side code hardcodes are the
#      offsets this kernel actually uses.
#
# Run it anywhere the lane runs, and on each architecture. It needs
# tracefs, so run it as root or in a --privileged container:
#
#   docker run --rm -i --privileged alpine:3.20 sh -s \
#       < tracepoint-inventory.sh
#
# The `-i` is load-bearing. Without it docker does not attach stdin,
# so the shell reads EOF, runs nothing, and exits 0 — the probe reports
# success having measured nothing. If the output does not start with a
# `kernel:` line, it did not run.
#
# Record the output in findings.md with the kernel release and arch.
set -u

echo "kernel: $(uname -r) $(uname -m)"

mount -t tracefs tracefs /sys/kernel/tracing 2>/dev/null
mount -t debugfs none /sys/kernel/debug 2>/dev/null
D=/sys/kernel/tracing/events/syscalls
[ -d "$D" ] || D=/sys/kernel/debug/tracing/events/syscalls
if [ ! -d "$D" ]; then
    echo "FAIL: no syscalls tracepoint directory; tracefs not mounted?"
    exit 1
fi
echo "tracefs: $D"

# The open family, the connect pair, and the m1070 calls already in
# use — the last group is the control: it is known to attach today, so
# if it reads ABSENT the probe itself is wrong.
echo
echo "--- presence ---"
for e in \
    sys_enter_open sys_exit_open \
    sys_enter_openat sys_exit_openat \
    sys_enter_openat2 sys_exit_openat2 \
    sys_enter_creat sys_exit_creat \
    sys_enter_connect sys_exit_connect \
    sys_enter_chdir sys_exit_chdir sys_exit_fchdir \
    sys_enter_renameat sys_exit_renameat \
    sys_enter_renameat2 sys_exit_renameat2 \
    sys_enter_rename sys_exit_rename
do
    if [ -d "$D/$e" ]; then echo "PRESENT $e"; else echo "ABSENT  $e"; fi
done

# The kernel-side code reads arguments at fixed offsets (ARG0=16,
# ARG1=24, ARG2=32, ARG3=40, RET=16). Print the format files so a
# disagreement is visible rather than inferred from a wrong path.
echo
echo "--- argument layout ---"
for e in sys_enter_openat sys_enter_openat2 sys_exit_openat \
         sys_enter_open sys_enter_creat sys_enter_connect
do
    [ -d "$D/$e" ] || continue
    echo "### $e"
    grep -E 'field:' "$D/$e/format" | sed 's/^[[:space:]]*//'
    echo
done
