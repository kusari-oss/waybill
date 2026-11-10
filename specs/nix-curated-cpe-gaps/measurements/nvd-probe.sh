#!/usr/bin/env bash
# Which NVD (vendor, product) does each candidate live under, and how are its
# versions encoded? Extends specs/1395-nix-python-cpe/measurements/nvd-probe.sh.
#
# One unauthenticated request per product, vendor WILDCARDED
# (`cpe:2.3:a:*:<product>`), 7 s apart (NVD allows 5 per rolling 30 s without
# a key). Every product here has < 2000 entries, so one page is the whole
# answer; nvd-summary.py checks that and fails loudly if not. Raw JSON lands
# in $OUT_DIR; nvd-summary.py reduces it.
#
# Usage: OUT_DIR=<dir> nvd-probe.sh <product>...
set -euo pipefail
O="${OUT_DIR:?set OUT_DIR}"
mkdir -p "$O"
base=https://services.nvd.nist.gov/rest/json/cpes/2.0
first=1
for p in "$@"; do
  [ "$first" = 1 ] || sleep 7
  first=0
  curl -sS --max-time 90 --get "$base" \
    --data-urlencode "cpeMatchString=cpe:2.3:a:*:$p" --data-urlencode "resultsPerPage=2000" \
    -o "$O/star-$p.json"
  echo "$p http-ok bytes=$(wc -c < "$O/star-$p.json" | tr -d ' ')"
done
