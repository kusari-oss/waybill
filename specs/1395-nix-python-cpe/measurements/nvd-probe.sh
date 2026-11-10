#!/usr/bin/env bash
# #1395: which NVD CPE does CPython live under, and how are versions encoded?
#
# Three unauthenticated requests to NVD's CPE dictionary, 7 s apart (NVD
# allows 5 per rolling 30 s without a key). Raw JSON lands in $OUT_DIR.
#
# Usage: OUT_DIR=<dir> nvd-probe.sh
set -euo pipefail
O="${OUT_DIR:?set OUT_DIR}"
mkdir -p "$O"
base=https://services.nvd.nist.gov/rest/json/cpes/2.0

q() { # <outfile> <cpeMatchString> <resultsPerPage>
  curl -sS --max-time 60 --get "$base" \
    --data-urlencode "cpeMatchString=$2" --data-urlencode "resultsPerPage=$3" -o "$O/$1"
}

# 1. The guess, as the issue phrased it. A cpeMatchString version is matched
#    exactly, so a bare minor version returns 0: this is the negative control
#    that a version must be a full release.
q python-python-3.13.json 'cpe:2.3:a:python:python:3.13' 100
sleep 7
# 2. Every python:python entry.
q python-python-all.json 'cpe:2.3:a:python:python' 2000
sleep 7
# 3. The product with the vendor wildcarded: who else files `python`?
q star-python-all.json 'cpe:2.3:a:*:python' 2000

python3 -I - "$O" <<'PY'
import collections, json, sys
o = sys.argv[1]
d = json.load(open(f"{o}/python-python-3.13.json"))
print(f"python:python:3.13 (exact)  totalResults={d['totalResults']}")

d = json.load(open(f"{o}/python-python-all.json"))
ps = d["products"]
print(f"python:python               totalResults={d['totalResults']} returned={len(ps)} "
      f"deprecated={sum(1 for p in ps if p['cpe'].get('deprecated'))}")
names = [p["cpe"]["cpeName"] for p in ps]
for want in ("3.13.13", "3.12.13", "3.14.4"):
    hits = [n for n in names if n.split(":")[5] == want]
    print(f"  version {want}: {hits}")
pre = collections.Counter(n.split(":")[6] for n in names if n.split(":")[6] not in ("*", "-"))
print(f"  update-field values used for pre-releases: {sorted(pre)}")

d = json.load(open(f"{o}/star-python-all.json"))
ps = d["products"]
print(f"*:python                    totalResults={d['totalResults']} returned={len(ps)}")
count = collections.Counter(p["cpe"]["cpeName"].split(":")[3] for p in ps)
title = {}
for p in ps:
    title.setdefault(p["cpe"]["cpeName"].split(":")[3],
                     (p["cpe"].get("titles") or [{}])[0].get("title", ""))
for v, n in count.most_common():
    print(f"  {v:24} {n:5}  {title[v][:70]}")
PY
