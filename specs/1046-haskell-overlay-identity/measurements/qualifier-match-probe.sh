#!/usr/bin/env bash
# Milestone 1046 — does a `vcs_url` qualifier stop an advisory matcher keying
# on `pkg:hackage/<name>@<version>`? Positive control: HSEC-2024-0003 affects
# `process` from 1.0.0.0, fixed in 1.6.23.0. Negative control: 1.6.23.0.
# Read-only; queries the public OSV API and a local osv-scanner.
set -uo pipefail
R=$(printf '1%.0s' $(seq 40))
ENC="?vcs_url=git%2Bhttps://example.invalid/waybill-fixture.git%40$R"
RAW="?vcs_url=git+https://example.invalid/waybill-fixture.git@$R"
api() { curl -s --max-time 30 -X POST https://api.osv.dev/v1/query -d "{\"package\":{\"purl\":\"$1\"}}" \
  | python3 -I -B -c 'import json,sys; d=json.load(sys.stdin); print(sorted(x["id"] for x in d.get("vulns",[])))'; }
for v in 1.6.18.0 1.6.23.0; do
  for q in "" "$ENC" "$RAW"; do p="pkg:hackage/process@$v$q"; printf 'OSV API\t%s\t%s\n' "$p" "$(api "$p")"; done
done
D=$(mktemp -d)
for n in plain enc; do q=""; [ $n = enc ] && q="$ENC"
  printf '{"bomFormat":"CycloneDX","specVersion":"1.6","version":1,"components":[{"type":"library","bom-ref":"c1","name":"process","version":"1.6.18.0","purl":"pkg:hackage/process@1.6.18.0%s"}]}' "$q" > "$D/$n.cdx.json"
  printf 'osv-scanner %s\t%s\t%s\n' "$(osv-scanner --version 2>/dev/null | head -1 | awk '{print $NF}')" "$n" \
    "$(osv-scanner scan source -L "$D/$n.cdx.json" --format json 2>/dev/null | python3 -I -B -c 'import json,sys; d=json.load(sys.stdin); print(sorted({v["id"] for r in d.get("results",[]) for p in r.get("packages",[]) for v in p.get("vulnerabilities",[])}))')"
done
