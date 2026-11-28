"""Spec 1668: per fixture, each waybill-fixture-{frozen,unused,loose} PURL,
who depends on it, its C45, and its source-files; plus main-module properties."""
import json, sys, pathlib
out = pathlib.Path(sys.argv[1])
print("fixture\tpurl\tdepended-on-by\tC45\tsource-files")
for f in sorted(out.glob("*.cdx.json")):
    d = json.loads(f.read_text())
    comps = []
    def walk(cs):
        for c in cs:
            comps.append(c); walk(c.get("components", []))
    walk(d.get("components", []))
    if "component" in d.get("metadata", {}):
        walk([d["metadata"]["component"]])
    by_ref = {c["bom-ref"]: c for c in comps}
    users = {}
    for dep in d.get("dependencies", []):
        for t in dep.get("dependsOn", []):
            users.setdefault(t, []).append(by_ref.get(dep["ref"], {}).get("name", dep["ref"]))
    prop = lambda c, n: ";".join(p["value"] for p in c.get("properties", []) if p["name"] == n)
    for c in sorted(comps, key=lambda c: c.get("purl", "")):
        if c.get("name", "") in ("waybill-fixture-frozen", "waybill-fixture-unused", "waybill-fixture-loose"):
            print(f"{f.name.split('.')[0]}\t{c.get('purl')}\t{','.join(sorted(users.get(c['bom-ref'], []))) or '-'}"
                  f"\t{prop(c, 'waybill:orphan-reason') or '-'}\t{prop(c, 'waybill:source-files') or '-'}")
    for c in comps:
        if c.get("name", "").startswith("waybill-fixture-") and c["name"] not in ("waybill-fixture-frozen", "waybill-fixture-unused", "waybill-fixture-loose"):
            print(f"{f.name.split('.')[0]}\tMAIN {c.get('purl')}\t-\t" +
                  json.dumps(sorted((p['name'], p['value']) for p in c.get('properties', []))), file=sys.stderr)
