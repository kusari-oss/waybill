#!/usr/bin/env python3 -I
"""Ask OSV whether it has advisories for a package, by ecosystem.

The counter-argument in #1375 is that `pkg:hackage` components do not need
CPEs because OSV already indexes Hackage from the Haskell Security Advisory
DB. That is an assertion about an external service, so it is measured here
rather than quoted.

Controls are mandatory and not optional decoration: a zero from OSV means
nothing without a pair that must be non-zero and a pair that must be zero.
The first version of this probe returned ERR for every input *including* its
positive control, which is the signature of a broken harness rather than an
empty ecosystem.

  python3 -I osv-probe.py <ecosystem> <name> [<name> ...]
"""
import json
import sys
import urllib.error
import urllib.request

API = "https://api.osv.dev/v1/query"


def query(name: str, ecosystem: str):
    body = json.dumps({"package": {"name": name, "ecosystem": ecosystem}}).encode()
    req = urllib.request.Request(
        API, data=body, headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.load(r).get("vulns", []), None
    except urllib.error.HTTPError as e:
        return None, f"HTTP {e.code}"
    except Exception as e:  # noqa: BLE001 - a probe reports, it does not raise
        return None, type(e).__name__


def main() -> None:
    ecosystem, names = sys.argv[1], sys.argv[2:]
    for n in names:
        vulns, err = query(n, ecosystem)
        if err is not None:
            print(f"{n}\tERR\t{err}")
        else:
            ids = ",".join(v["id"] for v in vulns[:3])
            print(f"{n}\t{len(vulns)}\t{ids}")


if __name__ == "__main__":
    main()
