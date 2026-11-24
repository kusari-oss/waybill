#!/bin/bash
# usage: golden_parity.sh <waybill> <public_corpus dir>
# parity-check every corpus golden and list the rows whose `verdict` is `fail`
# (spec 1654). G1 is reported in its own column: the golden harness masks
# SPDX 2.3's whole creationInfo, which removes the tool creator G1 reads, so a
# G1 failure on a golden is a masking artefact, not a document defect (R3).
W=$1; C=$2; T=$(mktemp -d)
echo "| target | parity_gaps | failing rows | G1 (masking, R3) |"
echo "|---|---|---|---|"
for d in "$C"/*/; do t=$(basename "$d"); mkdir -p "$T/$t"
  cp "$d/cdx.json" "$T/$t/waybill.cdx.json"; cp "$d/spdx-2.3.json" "$T/$t/waybill.spdx.json"; cp "$d/spdx-3.json" "$T/$t/waybill.spdx3.json"
  "$W" sbom parity-check --format json --scan-dir "$T/$t" 2>/dev/null | python3 -I -B -c "
import json,sys
r=json.load(sys.stdin); s=r['summary']
fail=[x['row_id'] for x in r['rows'] if x['verdict']=='fail']
assert len(fail)==s['parity_gaps'], ('verdict count drifted from parity_gaps', len(fail), s['parity_gaps'])
rest=[x for x in fail if x!='G1']
print(f\"| $t | {s['parity_gaps']} | {', '.join(rest) or '—'} | {'fail' if 'G1' in fail else 'pass'} |\")"
done; rm -rf "$T"
