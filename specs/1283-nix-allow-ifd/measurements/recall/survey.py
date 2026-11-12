#!/usr/bin/env python3
"""Per package/arm: component count, grype/osv finding counts, C190/C216. Run: python3 -I -B survey.py <R>"""
import json, sys, collections
R = sys.argv[1]
PKGS = "shellcheck pandoc hledger cachix hlint hadolint postgrest haskell-language-server".split()
def load(p):
    with open(p) as f: return json.load(f)
def comps(d):
    out, st = [], list(d.get("components", []) or [])
    while st:
        c = st.pop(); out.append(c); st.extend(c.get("components", []) or [])
    return out
res = {}
for p in PKGS:
    for a in ("W", "WR", "S"):
        d = load(f"{R}/out/{p}/{a}.cdx.json")
        cs = comps(d)
        mp = {x["name"]: x["value"] for x in d.get("metadata", {}).get("properties", []) or []}
        g = load(f"{R}/out/{p}/{a}.cdx.json.grype.json").get("matches", [])
        o = load(f"{R}/out/{p}/{a}.cdx.json.osv.json")
        on = sum(len(pk.get("vulnerabilities", []) or []) for r in o.get("results", []) or [] for pk in r.get("packages", []) or [])
        hsec = sorted({v["id"] for r in o.get("results", []) or [] for pk in r.get("packages", []) or [] for v in pk.get("vulnerabilities", []) or [] if v["id"].startswith("HSEC")})
        r = {"components": len(cs), "scope": dict(collections.Counter(c.get("scope", "<unset>") for c in cs)),
             "grype": len(g), "osv": on, "hsec": hsec}
        if a != "S":
            r["C190"] = mp.get("waybill:nix-closure-degraded"); r["C216"] = mp.get("waybill:closure-scope-method")
        res.setdefault(p, {})[a] = r
json.dump(res, open(f"{R}/out/survey.json", "w"), indent=1)
for p in res:
    for a, r in res[p].items():
        print(p, a, json.dumps(r))
