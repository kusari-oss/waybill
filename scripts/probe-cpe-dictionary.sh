#!/usr/bin/env bash
# Ask NVD whether each `(vendor, product)` pair in the curated CPE table
# (`waybill-cli/src/generate/cpe_table.toml`)
# exists in its CPE dictionary (#1246).
#
# Usage:  scripts/probe-cpe-dictionary.sh [vendor:product ...]
#         scripts/probe-cpe-dictionary.sh            # every pair in the table
# Prints: one `vendor:product<TAB>entries` line per pair, plus a positive
#         and a negative control. Exit 1 if any pair returned zero.
#
# WHY THIS EXISTS. The curated table is what makes a binary-derived
# `pkg:generic/<slug>@<version>` component reachable by a CPE matcher, and
# a wrong vendor or product there fails *silently*: the matcher returns
# zero, indistinguishable from a clean component. Upstream has shipped
# this defect repeatedly — syft derived `expat:expat` from a Conan recipe
# name where NVD files the records under `libexpat` (anchore/syft#4788),
# and built Apple bundle CPEs from display names, so `Google Chrome`
# matched nothing while `google:chrome` matches thousands (#5429).
#
# So a row may only exist in the table if NVD's dictionary has the pair.
# This script is how that is established. Its output is recorded on each
# pair in `cpe_table.toml` (`dictionary_entries`, `observed`, and the
# committed `dictionary_evidence` file), so the test suite can enforce it
# without a network call (spec 1491).
#
# WHAT THIS DOES NOT ESTABLISH. A pair in the dictionary may carry no
# advisories at all — dictionary presence and advisory coverage are
# different questions, and conflating them is the trap recorded in #1245
# ("a zero from OSV means nothing without a positive control"). Use
# `--cves` to ask the second question; it is not what the test asserts.
#
# Unauthenticated NVD allows 5 requests per rolling 30s, so this sleeps
# between calls. Set NVD_API_KEY to go faster.
set -euo pipefail

CVES=0
if [ "${1:-}" = "--cves" ]; then CVES=1; shift; fi

pairs=("$@")
if [ "${#pairs[@]}" -eq 0 ]; then
  # Read the table rather than restating it: a pair added to the source
  # without being probed is the thing this script exists to catch.
  src="$(dirname "$0")/../waybill-cli/src/generate/cpe_table.toml"
  mapfile -t pairs < <(python3 -I - "$src" <<'PY'
# One pair per line, `{ vendor = "...", product = "...", ... }`; read by
# pattern because `tomllib` needs Python 3.11, and the shape is fixed by
# the tests in `generate/cpe.rs`.
import re, sys
seen = set()
for vendor, product in re.findall(
    r'^\s*\{\s*vendor\s*=\s*"([^"]+)",\s*product\s*=\s*"([^"]+)"',
    open(sys.argv[1], encoding="utf-8").read(),
    re.M,
):
    key = f"{vendor}:{product}"
    if key not in seen:
        seen.add(key)
        print(key)
PY
  )
fi

# A pair NVD certainly has, and one it certainly does not: without both,
# a run of zeroes looks the same as a broken query or a throttled key.
pairs+=("openssl:openssl" "waybill-fixture-vendor:waybill-fixture-product")

sleep_s=7
auth=()
if [ -n "${NVD_API_KEY:-}" ]; then auth=(-H "apiKey: ${NVD_API_KEY}"); sleep_s=1; fi

base=https://services.nvd.nist.gov/rest/json
failed=0
first=1
for pair in "${pairs[@]}"; do
  vendor="${pair%%:*}"
  product="${pair##*:}"
  [ "$first" = 1 ] || sleep "$sleep_s"
  first=0

  if [ "$CVES" = 1 ]; then
    url="$base/cves/2.0"
    arg="virtualMatchString=cpe:2.3:a:${vendor}:${product}:*:*:*:*:*:*:*:*"
  else
    url="$base/cpes/2.0"
    arg="cpeMatchString=cpe:2.3:a:${vendor}:${product}"
  fi

  total=$(curl -sS --max-time 40 --get "$url" "${auth[@]+"${auth[@]}"}" \
            --data-urlencode "$arg" --data-urlencode 'resultsPerPage=1' \
          | python3 -I -c 'import sys,json; print(json.load(sys.stdin)["totalResults"])' \
          || echo "ERR")
  printf '%s\t%s\n' "$pair" "$total"
  if [ "$total" = "ERR" ] || [ "$total" = "0" ]; then
    case "$pair" in
      waybill-fixture-*) ;;   # the negative control must be 0
      *) failed=1 ;;
    esac
  elif [ "${pair#waybill-fixture-}" != "$pair" ]; then
    echo "negative control matched: the query is not filtering" >&2
    failed=1
  fi
done

exit "$failed"
