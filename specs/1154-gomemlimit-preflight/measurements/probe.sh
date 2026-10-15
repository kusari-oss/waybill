#!/usr/bin/env bash
# #1154 ask 3 -- what `GOMEMLIMIT` is worth for the `go list all`
# preflight, and what value to set.
#
# Why this exists: the value waybill sets is derived from the numbers
# this script produced, so the numbers have to be re-runnable when the
# Go toolchain, the architecture, or the workload changes. An
# undocumented memory limit is one that can go stale without anyone
# noticing (CLAUDE.md, "Measure external behaviour before designing
# around it").
#
# Needs: docker on a Linux host or VM, and a large Go module graph.
# Measured with Kubernetes v1.31.0 (3613 packages resolved).
#
#   git clone --depth 1 --filter=blob:none --branch v1.31.0 \
#       https://github.com/kubernetes/kubernetes.git "$REPO"
#
# TRAP worth knowing: on macOS + Colima, a bind mount of a path the VM
# does not share (anything under /private/tmp, for instance) mounts
# EMPTY and silently. `go list all` then prints
# `warning: "all" matched no packages` and exits 0 -- a zero result
# that looks exactly like a clean one. Keep the repo under $HOME and
# assert the package count before trusting any measurement.
set -euo pipefail

REPO="${REPO:?set REPO to a large Go module checkout under \$HOME}"
IMAGE="${IMAGE:-golang:1.27-alpine}"
WORKERS="${WORKERS:-4}"
docker volume create wbgomod >/dev/null

BIN="$(mktemp -d)"
trap 'rm -rf "$BIN"' EXIT

# One `go list all`, reporting exit status, packages and cgroup peak.
cat > "$BIN/one.sh" <<'INNER'
#!/bin/sh
go list all >/tmp/o 2>/tmp/e; rc=$?
echo "rc=$rc pkgs=$(wc -l </tmp/o) peak=$(cat /sys/fs/cgroup/memory.peak 2>/dev/null || echo '?')"
head -c 120 /tmp/e | tr '\n' ' '
INNER

# N concurrent, as waybill's worker pool runs them: worker_count() =
# min(workspaces, available_parallelism()), all in ONE cgroup. This is
# the case an undivided limit gets wrong.
cat > "$BIN/many.sh" <<INNER
#!/bin/sh
i=1; while [ \$i -le $WORKERS ]; do
  ( go list all >/tmp/o\$i 2>/dev/null; echo \$? >/tmp/rc\$i ) &
  i=\$((i+1))
done
wait
echo "rcs=\$(cat /tmp/rc* | tr '\n' ',') killed=\$(grep -c 137 /tmp/rc* | grep -c ':1')"
INNER
chmod +x "$BIN"/*.sh

run() { # $1=cgroup cap  $2=GOMEMLIMIT or '-'  $3=one.sh|many.sh
  local env=()
  [ "$2" != "-" ] && env=(-e "GOMEMLIMIT=$2")
  printf 'cap=%-6s GOMEMLIMIT=%-9s ' "$1" "$2"
  docker run --rm --memory="$1" "${env[@]}" \
    -v "$REPO":/src:ro -v wbgomod:/go/pkg/mod -v "$BIN":/pb:ro \
    -w /src "$IMAGE" sh "/pb/$3" 2>&1 | tr '\n' ' '
  echo
}

echo "### warm the module cache (downloads; must report a real package count)"
run 8g - one.sh

echo
echo "### single process: where does it die, and does a limit rescue it?"
for cap in 320m 256m 192m 160m; do run "$cap" - one.sh; done
for lim in 144MiB 128MiB 112MiB;   do run 160m "$lim" one.sh; done
echo "# 90% of cap, descending -- finds the floor a limit cannot move"
run 144m 130MiB one.sh; run 128m 115MiB one.sh; run 112m 100MiB one.sh
echo "# at a cap where 90% died, does a tighter limit still rescue it?"
run 112m 78MiB one.sh; run 112m 67MiB one.sh

echo
echo "### $WORKERS concurrent: undivided vs divided (the formula waybill uses)"
for cap in 384m 320m 288m 256m; do run "$cap" - many.sh; done
run 384m 345MiB many.sh; run 320m 288MiB many.sh   # undivided 90%
run 384m 80MiB  many.sh; run 320m 64MiB  many.sh   # cap/workers - 16MiB
