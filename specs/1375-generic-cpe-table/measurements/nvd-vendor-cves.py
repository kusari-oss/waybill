#!/usr/bin/env python3 -I
"""#1490 part 2: every NVD dictionary vendor of each curated product, with
the number of CVEs NVD files under each (vendor, product) pair.

Usage:  python3 -I -B nvd-vendor-cves.py <cpe.rs> > nvd-vendor-cves-<date>.tsv

Prints `product<TAB>vendor<TAB>dictionary-entries<TAB>cves` per pair, then a
positive control (openssl:openssl, must be > 0) and a negative control
(a fixture pair, must be 0). A query that fails three times prints ERR.

Vendors come from one dictionary query per product with a wildcard vendor
and `resultsPerPage=10000`, so a product with many entries is not cut off at
the first page (cpe-titles.py reads only 60). Unauthenticated NVD allows 5
requests per rolling 30 s, so requests are 7 s apart; NVD_API_KEY shortens
that to 1 s.
"""
import json, os, re, sys, time, urllib.parse, urllib.request
from collections import Counter

BASE = "https://services.nvd.nist.gov/rest/json"
KEY = os.environ.get("NVD_API_KEY")
GAP = 1 if KEY else 7
last = [0.0]


def get(path, params):
    for attempt in range(3):
        wait = last[0] + GAP - time.time()
        if wait > 0:
            time.sleep(wait)
        last[0] = time.time()
        req = urllib.request.Request(f"{BASE}/{path}?{urllib.parse.urlencode(params)}")
        if KEY:
            req.add_header("apiKey", KEY)
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.load(r)
        except Exception as e:
            print(f"# {path} {params} attempt {attempt + 1}: {type(e).__name__}", file=sys.stderr)
            time.sleep(15 * (attempt + 1))
    return None


def cves(vendor, product):
    d = get("cves/2.0", {"virtualMatchString": f"cpe:2.3:a:{vendor}:{product}:*:*:*:*:*:*:*:*", "resultsPerPage": 1})
    return "ERR" if d is None else d["totalResults"]


src = open(sys.argv[1]).read()
table = re.search(r"GENERIC_LIBRARY_CPES[^=]*= &\[(.*?)\n\];", src, re.S).group(1)
products = sorted({p for _, p in re.findall(r'\("([^"]+)", "([^"]+)"\)', table)})

print("# product\tvendor\tdictionary-entries\tcves")
for product in products:
    d = get("cpes/2.0", {"cpeMatchString": f"cpe:2.3:a:*:{product}", "resultsPerPage": 10000})
    if d is None:
        print(f"{product}\tERR\tERR\tERR", flush=True)
        continue
    vendors = Counter(
        p["cpe"]["cpeName"].split(":")[3]
        for p in d.get("products", [])
        if p["cpe"]["cpeName"].split(":")[4] == product
    )
    if d.get("totalResults", 0) > len(d.get("products", [])):
        print(f"# {product}: {d['totalResults']} entries, {len(d['products'])} returned", flush=True)
    for vendor, n in sorted(vendors.items()):
        print(f"{product}\t{vendor}\t{n}\t{cves(vendor, product)}", flush=True)

print(f"# control+\topenssl:openssl\t{cves('openssl', 'openssl')}")
print(f"# control-\twaybill-fixture-vendor:waybill-fixture-product\t{cves('waybill-fixture-vendor', 'waybill-fixture-product')}")
