#!/usr/bin/env bash
# ABBA: after, before, before, after — balances CD's server-side warming.
S="$(cd "$(dirname "$0")" && pwd)"
T="$1"
for spec in after:1 before:1 before:2 after:2; do
  bin=${spec%%:*}; n=${spec##*:}
  out="$S/$bin-$n.cdx.json"; log="$S/$bin-$n.log"
  start=$(python3 -c 'import time;print(time.time())')
  WAYBILL_CLEARLY_DEFINED_NO_CACHE=1 RUST_LOG=info "$S/wb-$bin" sbom scan --path "$T" --no-deep-hash \
    --enrich-sources clearly-defined --format cyclonedx-json --output "cyclonedx-json=$out" > "$log" 2>&1
  rc=$?
  end=$(python3 -c 'import time;print(time.time())')
  lic=$(python3 -c "import json;d=json.load(open('$out'));print(sum(1 for c in d.get('components',[]) for l in c.get('licenses',[]) if l.get('license',{}).get('acknowledgement')=='concluded' or l.get('acknowledgement')=='concluded'))" 2>/dev/null)
  echo "$bin-$n rc=$rc wall=$(python3 -c "print(round($end-$start,1))")s concluded=$lic"
done
