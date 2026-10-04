#!/usr/bin/env bash
# #1058 probe: per-scan deps.dev outcome counts on cached public-corpus repos,
# online, with waybill's default enrichment. Reads the #877 pass log line
# ("deps.dev licence enrichment complete" pass="initial") and the emitted
# CycloneDX. Usage: probe_outcomes.sh <waybill-bin> <repo-dir> <label>
set -u
BIN=$1; REPO=$2; N=$3; W=$(mktemp -d)
"$BIN" sbom scan --path "$REPO" --no-deep-hash --format cyclonedx-json --output cyclonedx-json="$W/o.json" > "$W/log" 2>&1
comps=$(jq '.components|length' "$W/o.json")
line=$(sed 's/\x1b\[[0-9;]*m//g' "$W/log" | grep "deps.dev licence enrichment complete" | grep 'pass="initial"' \
  | grep -o 'attempted=[0-9]*\|network_lookups=[0-9]*\|cache_hits=[0-9]*\|unqueried_offline=[0-9]*\|matched=[0-9]*\|enriched=[0-9]*' | tr '\n' ' ')
eco=$(jq -r '[.components[].purl // "" | select(.!="") | capture("^pkg:(?<e>[^/]+)/").e] | group_by(.) | map("\(.[0])=\(length)") | join(",")' "$W/o.json")
echo "$N comps=$comps [$eco] $line"
rm -rf "$W"
