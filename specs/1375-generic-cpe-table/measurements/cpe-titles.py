#!/usr/bin/env python3 -I
import json, sys, time, urllib.parse, urllib.request
BASE = "https://services.nvd.nist.gov/rest/json/cpes/2.0"
first = True
for name in sys.argv[1:]:
    if not first: time.sleep(7)
    first = False
    url = f"{BASE}?{urllib.parse.urlencode({'cpeMatchString': f'cpe:2.3:a:*:{name}', 'resultsPerPage': 60})}"
    try:
        with urllib.request.urlopen(url, timeout=45) as r:
            d = json.load(r)
    except Exception as e:
        print(f"{name}\tERR\t{type(e).__name__}"); continue
    seen = {}
    for p in d.get("products", []):
        v = p["cpe"]["cpeName"].split(":")[3]
        t = (p["cpe"].get("titles") or [{}])[0].get("title", "")
        seen.setdefault(v, t)
    print(f"=== {name} ({d.get('totalResults')} entries) ===")
    for v, t in seen.items():
        print(f"    {v:28} {t[:72]}")
