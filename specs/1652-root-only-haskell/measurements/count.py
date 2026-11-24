import json, sys, re, os
def walk(cs):
    for c in cs or []:
        yield c
        yield from walk(c.get("components"))
d = sys.argv[1]
names = sorted({f.split(".")[0] for f in os.listdir(d) if f.endswith(".cdx.json")})
print("target | mode | components | main modules (nested) | hackage | completeness | C244 count | log")
for n in names:
    for m in ("all","root-only","strict"):
        p = f"{d}/{n}.{m}.cdx.json"
        if not os.path.exists(p): print(n, m, "MISSING"); continue
        doc = json.load(open(p)); cs = list(walk(doc.get("components")))
        props = lambda c: {x["name"]: x["value"] for x in c.get("properties",[])}
        mains = [c for c in cs if props(c).get("waybill:component-role") == "main-module"]
        nested = [c for c in mains if props(c).get("waybill:workspace-member") not in (None, '["."]')]
        hk = sum(1 for c in cs if (c.get("purl") or "").startswith("pkg:hackage/"))
        dp = {x["name"]: x["value"] for x in doc["metadata"].get("properties",[])}
        log = open(p + ".log").read()
        log = re.sub(r"\x1b\[[0-9;]*m", "", log)
        mm = re.search(r"root_main_modules=\d+ workspace_members_followed=\d+ nested_projects_ignored=\d+", log)
        fb = "found zero root-level manifests" in log
        c244 = dp.get('waybill:project-discovery-dropped')
        c244n = json.loads(c244)['count'] if c244 else '-'
        print(f"{n} | {m} | {len(cs)} | {len(mains)} ({len(nested)}) | {hk} | {dp.get('waybill:graph-completeness')} | {c244n} | {'FALLBACK' if fb else (mm.group(0) if mm else '-')}")
