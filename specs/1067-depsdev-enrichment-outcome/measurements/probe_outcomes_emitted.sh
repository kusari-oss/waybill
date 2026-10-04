#!/usr/bin/env bash
# #1058 post-implementation probe: what the emitted SBOM says. Prints the
# per-pass log line, the C192 document counts, and the C191 tallies over
# components, which must agree (SC-005). Runs with a fresh deps.dev disk
# cache so no earlier answer is replayed.
# Usage: probe_outcomes_emitted.sh <waybill-bin> <repo-dir> <label>
set -u
BIN=$1; REPO=$2; N=$3; W=$(mktemp -d)
WAYBILL_DEPS_DEV_CACHE_DIR="$W/cache" "$BIN" sbom scan --path "$REPO" --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json="$W/o.json" > "$W/log" 2>&1
line=$(sed 's/\x1b\[[0-9;]*m//g' "$W/log" | grep "deps.dev licence enrichment complete" | grep 'pass="initial"' \
  | grep -o 'attempted=[0-9]*\|network_lookups=[0-9]*\|matched=[0-9]*' | tr '\n' ' ')
c192=$(jq -r '[.metadata.properties[]? | select(.name=="waybill:deps-dev-outcomes") | .value] | first // "none"' "$W/o.json")
c191=$(jq -c '[.. | objects | select(has("properties")) | .properties[]? | select(.name=="waybill:deps-dev-outcome") | .value] | group_by(.) | map({(.[0]): length}) | add // {}' "$W/o.json")
echo "$N $line"
echo "  C192 $c192"
echo "  C191 $c191"
rm -rf "$W"
