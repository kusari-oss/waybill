"""Compare CycloneDX scope marks of two waybill runs of one target.

usage: scopediff.py TRUTH.cdx.json OTHER.cdx.json
TRUTH is the runtime-closure run, OTHER the walk run. Components are walked
recursively (CDX components nest) and keyed by purl.
"""
import json, sys


def comps(path):
    doc = json.load(open(path))

    def walk(cs):
        for c in cs or []:
            yield c
            yield from walk(c.get("components"))

    out = {}
    for c in walk(doc.get("components")):
        props = {p["name"]: p["value"] for p in c.get("properties", []) or []}
        if "waybill:closure-role" not in props:
            continue
        out[c["purl"]] = (c.get("name"), c.get("version"), c.get("scope", "required"),
                          props.get("waybill:scope-classification-source"))
    method = [p["value"] for p in doc["metadata"].get("properties", []) if p["name"] == "waybill:closure-scope-method"]
    return out, method


t, tm = comps(sys.argv[1])
o, om = comps(sys.argv[2])
print("truth C216:", tm)
print("other C216:", om)
common = set(t) & set(o)
rt_t = {p for p in common if t[p][2] != "excluded"}
rt_o = {p for p in common if o[p][2] != "excluded"}
print(f"members={len(common)} runtime(truth)={len(rt_t)} runtime(other)={len(rt_o)} "
      f"tp={len(rt_t & rt_o)} false-excluded={len(rt_t - rt_o)} false-runtime={len(rt_o - rt_t)}")
for p in sorted(rt_t - rt_o, key=lambda p: t[p][0] or ""):
    print("  FALSE-EXCLUDED", t[p][0], t[p][1], "other:", o[p][3])
for p in sorted(rt_o - rt_t, key=lambda p: o[p][0] or ""):
    print("  FALSE-RUNTIME ", o[p][0], o[p][1], "other:", o[p][3])
