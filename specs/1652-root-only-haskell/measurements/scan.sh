#!/bin/bash
# usage: WAYBILL=<binary> SCAN_HOME=<empty dir> scan.sh <tree> <mode> <out.cdx.json>
# Offline, isolated HOME. The log lands beside the output as <out>.log.
HOME=${SCAN_HOME:?} "${WAYBILL:?}" sbom scan --path "$1" --offline --project-discovery="$2" \
  --format cyclonedx-json --output "$3" >"$3.log" 2>&1
echo "exit=$?"
