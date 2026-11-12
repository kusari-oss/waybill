"""#1455 lib/links rule — findings present before and absent after.

usage: python3 -I -B diff.py <before outdir> <after outdir>

Each outdir is what scan.sh wrote. A finding is (scanner, canonical id,
package name, version); the canonical id is `canon` from
specs/1283-nix-allow-ifd/measurements/recall/classify.py, copied verbatim.
"""
import json, sys


# --- copied from specs/1283-nix-allow-ifd/measurements/recall/classify.py ---
def canon(vid, aliases):
    if vid and vid.upper().startswith("CVE-"):
        return vid.upper()
    for a in aliases or []:
        if a and a.upper().startswith("CVE-"):
            return a.upper()
    return (vid or "").upper()
# --- end copy ---


def findings(d):
    out = set()
    g = json.load(open(f"{d}/grype.json"))
    for m in g.get("matches", []) or []:
        a, v = m.get("artifact", {}), m.get("vulnerability", {})
        rel = [r.get("id") for r in m.get("relatedVulnerabilities", []) or []]
        out.add(("grype", canon(v.get("id"), rel), v.get("id"), a.get("name"), a.get("version")))
    o = json.load(open(f"{d}/osv.json"))
    for r in o.get("results", []) or []:
        for pk in r.get("packages", []) or []:
            p = pk.get("package", {})
            for v in pk.get("vulnerabilities", []) or []:
                out.add(("osv", canon(v.get("id"), v.get("aliases")), v.get("id"), p.get("name"), p.get("version")))
    return out


before, after = findings(sys.argv[1]), findings(sys.argv[2])
key = lambda f: (f[0], f[1], f[3], f[4])
bk, ak = {key(f): f for f in before}, {key(f): f for f in after}
print(f"findings before: {len(bk)}  after: {len(ak)}")
for s in ("grype", "osv"):
    print(f"  {s}: before {sum(1 for k in bk if k[0] == s)}, after {sum(1 for k in ak if k[0] == s)}")
lost = sorted(set(bk) - set(ak))
gained = sorted(set(ak) - set(bk))
print(f"\npresent before, absent after: {len(lost)}")
for k in lost:
    f = bk[k]
    print(f"  {f[0]:5} {f[1]:22} ({f[2]}) {f[3]} {f[4]}")
print(f"\nabsent before, present after: {len(gained)}")
for k in gained:
    f = ak[k]
    print(f"  {f[0]:5} {f[1]:22} ({f[2]}) {f[3]} {f[4]}")
