#!/usr/bin/env bash
# Run the eBPF integration container with a deadline, and say why when it
# misses one (#991).
#
# The harness script is the container's PID 1, so the container should exit
# the moment the script does. #991 caught a run where the script printed its
# last line and the container then sat for 10m25s until the CI wrapper killed
# the step, leaving nothing to separate "harness hung" from "container would
# not reap". A plain `docker run` cannot tell those apart; this can:
#
#   - the container is started detached and waited on with a deadline;
#   - on a missed deadline it dumps the container's state and processes, and
#     any host process in uninterruptible sleep (a process stuck in D state
#     blocks PID-namespace teardown, which matches the #991 signature);
#   - the container is always removed, so a retry starts clean.
#
# Usage: ebpf-container-run.sh <image> <deadline-seconds> [entrypoint [args...]]
#
# With no entrypoint the image's own ENTRYPOINT runs, which is the
# integration harness. #1185's handshake-rate probe needs the same
# privileged container and the same deadline-with-diagnostics treatment
# while running a different script, so it is passed explicitly rather
# than given a second copy of this logic to drift from.

set -uo pipefail

image="$1"
deadline="$2"
shift 2

entrypoint_args=()
if [[ $# -gt 0 ]]; then
    entrypoint_args=(--entrypoint "$1")
    shift
fi

# `${a[@]+"${a[@]}"}` rather than `"${a[@]}"`: under `set -u` an empty
# array expansion is an unbound-variable error before bash 4.4, and this
# script is reachable from a macOS host whose /bin/bash is 3.2. Measured,
# not assumed -- bash 3.2 errors here and bash 5.3 does not.
cid=$(docker run -d --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    ${entrypoint_args[@]+"${entrypoint_args[@]}"} \
    "$image" "$@") || exit 1

docker logs -f "$cid" &
logs_pid=$!

# Bounded: if the container will not reap, removing it may not either.
trap 'timeout 60 docker rm -f "$cid" > /dev/null 2>&1 || true' EXIT

if code=$(timeout "$deadline" docker wait "$cid"); then
    wait "$logs_pid" 2> /dev/null || true
    exit "$code"
fi

kill "$logs_pid" 2> /dev/null || true
echo "::error::eBPF integration container did not exit within ${deadline}s"
echo "--- container state"
docker inspect -f '{{json .State}}' "$cid" || true
echo "--- container processes"
docker top "$cid" -eo pid,ppid,stat,wchan:32,etime,cmd || true
echo "--- host processes in uninterruptible sleep (D)"
ps -eo pid,ppid,stat,wchan:32,etime,cmd | awk 'NR == 1 || $3 ~ /^D/'
echo "--- kernel log tail"
sudo dmesg | tail -n 50 || true
exit 1
