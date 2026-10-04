#!/usr/bin/env bash
# #853 probe 3: end-to-end waybill wall clock for N go.sum modules that are
# not in the module cache, under a given GOPROXY. `go` is hidden from PATH so
# ladder step 1 (`go mod graph`) cannot do its own network work, and
# GOMODCACHE is empty so step 2 misses. Only step 3 (proxy fetch) remains.
# Usage: probe_e2e.sh <waybill-bin> <N> <GOPROXY> <label>
set -u
BIN=$1; N=$2; PROXY=$3; LABEL=$4
W=$(mktemp -d); tag=$RANDOM$RANDOM
{ echo "module example.com/probe853"; echo; echo "go 1.22"; echo; echo "require ("
  for i in $( [ "$N" -gt 0 ] && seq 1 "$N"); do echo "  github.com/waybill-probe-853/m$tag-$i v1.0.0"; done; echo ")"; } > "$W/go.mod"
for i in $( [ "$N" -gt 0 ] && seq 1 "$N"); do
  echo "github.com/waybill-probe-853/m$tag-$i v1.0.0 h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
  echo "github.com/waybill-probe-853/m$tag-$i v1.0.0/go.mod h1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
done > "$W/go.sum"
printf 'package main\nfunc main() {}\n' > "$W/main.go"
mkdir -p "$W/.modcache"
start=$(python3 -c 'import time;print(time.time())')
env -i HOME="$HOME" PATH="/usr/bin:/bin" GOPROXY="$PROXY" GOMODCACHE="$W/.modcache" GOFLAGS= \
  "$BIN" sbom scan --path "$W" --no-deep-hash --no-deps-dev --no-go-mod-why --format cyclonedx-json \
  --output cyclonedx-json="$W/out.cdx.json" > "$W/log.txt" 2>&1
rc=$?
end=$(python3 -c 'import time;print(time.time())')
fetch_fail=$(grep -c "go-mod proxy fetch failed" "$W/log.txt")
classes=$(grep -o 'error_class="[a-z0-9_-]*"\|error_class=[a-z0-9_-]*' "$W/log.txt" | sort | uniq -c | tr -s ' ' | tr '\n' ';')
python3 -c "print('%-14s N=%-4s rc=%s wall=%6.1fs failed_fetch_logs=%s %s' % ('$LABEL',$N,$rc,$end-$start,'$fetch_fail','$classes'))"
[ -n "${KEEP:-}" ] && cp "$W/out.cdx.json" "$KEEP"; rm -rf "$W"
