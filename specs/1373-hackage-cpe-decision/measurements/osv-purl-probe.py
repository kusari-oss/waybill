#!/usr/bin/env python3 -I
"""Query OSV with the exact PURL waybill emits, which is what a consumer does.

The name+ecosystem form answers "does OSV know this package". This answers
the question that actually matters: does the identifier *in waybill's
document* reach the advisory, version range and all. A name that resolves
while the versioned PURL does not would mean the document is answerable in
principle and not in practice.

  python3 -I osv-purl-probe.py <document.cdx.json> <purl-prefix>
"""
import json
import sys
import urllib.error
import urllib.request

API = "https://api.osv.dev/v1/query"


def query(purl: str):
    body = json.dumps({"package": {"purl": purl}}).encode()
    req = urllib.request.Request(
        API, data=body, headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.load(r).get("vulns", []), None
    except urllib.error.HTTPError as e:
        return None, f"HTTP {e.code}"
    except Exception as e:  # noqa: BLE001
        return None, type(e).__name__


def main() -> None:
    doc = json.load(open(sys.argv[1]))
    prefix = sys.argv[2]
    purls = sorted(
        {c["purl"] for c in doc.get("components", []) if (c.get("purl") or "").startswith(prefix)}
    )
    hits = 0
    for p in purls:
        vulns, err = query(p)
        if err is not None:
            print(f"{p}\tERR\t{err}")
        elif vulns:
            hits += 1
            print(f"{p}\t{len(vulns)}\t{','.join(v['id'] for v in vulns[:3])}")
    print(f"# queried {len(purls)} PURLs, {hits} returned at least one advisory", file=sys.stderr)


if __name__ == "__main__":
    main()
