#!/usr/bin/env bash
# #853 probe 4: proxy-step cost on a HEALTHY proxy for a large real dependency
# set (kubernetes go.mod/go.sum at a pinned tag). Empty GOMODCACHE, `go`
# hidden: every go.sum module goes through the proxy step.
# Usage: probe_healthy_real.sh <waybill-bin> <tag> [GOPROXY]
set -u
BIN=$1; TAG=${2:-v1.31.0}; PROXY=${3:-https://proxy.golang.org}
W=$(mktemp -d)
for f in go.mod go.sum; do
  curl -sfL "https://raw.githubusercontent.com/kubernetes/kubernetes/$TAG/$f" -o "$W/$f" || { echo "fetch $f failed"; exit 1; }
done
printf 'package main\nfunc main() {}\n' > "$W/main.go"
mods=$(awk '{print $1" "$2}' "$W/go.sum" | sed 's#/go.mod$##' | sort -u | wc -l | tr -d ' ')
mkdir -p "$W/.modcache"
start=$(python3 -c 'import time;print(time.time())')
env -i HOME="$W" PATH="/usr/bin:/bin" GOPROXY="$PROXY" GOMODCACHE="$W/.modcache" \
  "$BIN" sbom scan --path "$W" --no-deep-hash --no-deps-dev --no-go-mod-why --format cyclonedx-json \
  --output cyclonedx-json="$W/out.cdx.json" > "$W/log.txt" 2>&1
end=$(python3 -c 'import time;print(time.time())')
fails=$(grep -c "go-mod proxy fetch failed" "$W/log.txt")
src=$(jq -r '[.components[].properties[]? | select(.name=="waybill:go-transitive-source") | .value] | group_by(.) | map("\(.[0])=\(length)") | join(",")' "$W/out.cdx.json")
python3 -c "print('kubernetes $TAG go.sum-modules=$mods wall=%.1fs failed-fetches=$fails sources=$src' % ($end-$start))"
rm -rf "$W"
