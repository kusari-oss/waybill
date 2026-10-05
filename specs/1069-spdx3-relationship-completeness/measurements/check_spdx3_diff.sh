#!/usr/bin/env bash
# #878 / m1069: an SPDX 3 document changed by this feature must be unchanged
# once its dependsOn relationships are removed. Usage: check_spdx3_diff.sh OLD NEW
strip='del(.["@graph"][] | select(.relationshipType=="dependsOn"))'
if diff <(jq -S "$strip" "$1") <(jq -S "$strip" "$2") >/dev/null; then
  echo "OK  $(basename "$(dirname "$2")")/$(basename "$2"): only dependsOn relationships differ"
else
  echo "BAD $(basename "$(dirname "$2")")/$(basename "$2"): changes outside dependsOn relationships"
  diff <(jq -S "$strip" "$1") <(jq -S "$strip" "$2") | head -20
  exit 1
fi
