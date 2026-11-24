#!/bin/bash
# usage: golden_parity.sh <waybill> <public_corpus dir> — parity-check every golden; prints summary + rows whose presence/count differ.
W=$1; C=$2; T=$(mktemp -d)
for d in "$C"/*/; do t=$(basename "$d"); mkdir -p "$T/$t"
  cp "$d/cdx.json" "$T/$t/waybill.cdx.json"; cp "$d/spdx-2.3.json" "$T/$t/waybill.spdx.json"; cp "$d/spdx-3.json" "$T/$t/waybill.spdx3.json"
  "$W" sbom parity-check --format json --scan-dir "$T/$t" 2>/dev/null | python3 -I -B -c "
import json,sys
r=json.load(sys.stdin); s=r['summary']; F=('cdx','spdx23','spdx3')
diff=[x['row_id']+':'+'/'.join(str(x[f].get('value_count') if x[f].get('kind')=='present' else '-') for f in F) for x in r['rows'] if x['universal_parity'] and 'present' in [x[f].get('kind') for f in F] and (len({x[f].get('kind') for f in F})>1 or len({x[f].get('value_count') for f in F})>1)]
print(f\"$t | {s['parity_gaps']} | {' '.join(diff) or '-'}\")"
done; rm -rf "$T"
