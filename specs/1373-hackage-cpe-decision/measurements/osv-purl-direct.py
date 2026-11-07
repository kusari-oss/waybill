#!/usr/bin/env python3 -I
"""Ask OSV about an arbitrary PURL. Used to establish which PURL *types* are
addressable at all, which is what distinguishes a real coverage gap from a
component that simply uses a different feed."""
import json, sys, urllib.error, urllib.request
for purl in sys.argv[1:]:
    body = json.dumps({"package": {"purl": purl}}).encode()
    req = urllib.request.Request("https://api.osv.dev/v1/query", data=body,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            v = json.load(r).get("vulns", [])
            print(f"{purl}\t{len(v)}\t{','.join(x['id'] for x in v[:2])}")
    except urllib.error.HTTPError as e:
        print(f"{purl}\tHTTP {e.code}\t{e.read()[:120].decode(errors='replace')}")
    except Exception as e:
        print(f"{purl}\t{type(e).__name__}\t")
