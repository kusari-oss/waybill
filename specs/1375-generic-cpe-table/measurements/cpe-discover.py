#!/usr/bin/env python3 -I
"""Discover which NVD vendor files a product, rather than guessing the pair.

`scripts/probe-cpe-dictionary.sh` answers "does vendor:product exist", which
needs the vendor known. For a `pkg:generic/<slug>` component the vendor is
exactly what is unknown -- `bash` is filed under `gnu`, not `bash`. This asks
NVD for the product and reports the vendors it comes back under.

  python3 -I cpe-discover.py <name> [<name> ...]
"""
import collections, json, sys, time, urllib.parse, urllib.request

BASE = "https://services.nvd.nist.gov/rest/json/cpes/2.0"


def query(match: str, per_page: int = 100):
    url = f"{BASE}?{urllib.parse.urlencode({'cpeMatchString': match, 'resultsPerPage': per_page})}"
    try:
        with urllib.request.urlopen(url, timeout=45) as r:
            return json.load(r), None
    except Exception as e:  # noqa: BLE001 - a probe reports, it does not raise
        return None, f"{type(e).__name__}"


def main() -> None:
    first = True
    for name in sys.argv[1:]:
        if not first:
            time.sleep(7)          # unauthenticated: 5 requests / 30s
        first = False
        d, err = query(f"cpe:2.3:a:*:{name}")
        if err:
            print(f"{name}\tERR\t{err}")
            continue
        total = d.get("totalResults", 0)
        vendors = collections.Counter(
            p["cpe"]["cpeName"].split(":")[3] for p in d.get("products", [])
        )
        top = ",".join(f"{v}({n})" for v, n in vendors.most_common(3))
        print(f"{name}\t{total}\t{top}")


if __name__ == "__main__":
    main()
