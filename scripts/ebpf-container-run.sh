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
# Usage: ebpf-container-run.sh <image> <deadline-seconds>

set -uo pipefail

image="$1"
deadline="$2"

cid=$(docker run -d --privileged \
    -v /sys/kernel/debug:/sys/kernel/debug \
    "$image") || exit 1

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
