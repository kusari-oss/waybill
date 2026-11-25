# usage: before_after.py <before public_corpus> <after public_corpus>
# Per closure target, CycloneDX before vs after: C218 components, those added
# (PURLs after minus before), C247 entries listing `absent`, C104 and C105,
# the orphan count C105 reports, C209, and boot-library edges that existed
# before but are gone after (research R4).
import json, sys, os, re
b_dir, a_dir = sys.argv[1], sys.argv[2]
def walk(cs):
    for c in cs:
        yield c; yield from walk(c.get('components', []))
def load(d, t):
    doc = json.load(open(os.path.join(d, t, 'cdx.json')))
    comps = list(walk(doc.get('components', [])))
    props = lambda c: {x['name']: x['value'] for x in c.get('properties', [])}
    boot = {c['purl']: props(c) for c in comps if 'waybill:boot-library-compiler' in props(c)}
    m = props(doc['metadata'])
    edges = {(d['ref'], t) for d in doc.get('dependencies', []) for t in d.get('dependsOn', [])}
    return comps, boot, m, edges
for t in sorted(os.listdir(a_dir)):
    if not t.startswith('nix-closure'): continue
    cb, bb, mb, eb = load(b_dir, t)
    ca, ba, ma, ea = load(a_dir, t)
    added = sorted(set(ba) - set(bb)); removed = sorted(set(bb) - set(ba))
    absent = [p for p, v in ba.items() if 'absent' in json.loads(v.get('waybill:boot-library-dependencies', '{}'))]
    orph = lambda m: (re.search(r'orphaned-components-detected: (\d+)', m.get('waybill:graph-completeness-reason', '')) or [None, '0'])[1]
    boot_edges_lost = {e for e in eb - ea if e[0] in bb}
    print(f"== {t}")
    print(f"  components {len(cb)} -> {len(ca)} (+{len(ca)-len(cb)}); C218 {len(bb)} -> {len(ba)}; added {len(added)}; removed {len(removed)}")
    print(f"  C247 entries with absent: {len(absent)}")
    print(f"  C104 {mb.get('waybill:graph-completeness')} -> {ma.get('waybill:graph-completeness')}")
    print(f"  C105 before: {mb.get('waybill:graph-completeness-reason')}")
    print(f"  C105 after:  {ma.get('waybill:graph-completeness-reason')}")
    print(f"  orphans {orph(mb)} -> {orph(ma)}")
    cv = lambda m: json.loads(m.get('waybill:cpe-coverage', '{}'))
    b9, a9 = cv(mb), cv(ma)
    keys = ['with_candidates', 'addressable_without_cpe', 'unaddressable', 'excluded_members', 'excluded_files']
    print(f"  C209 before {[b9.get(k) for k in keys]} sum {sum(b9.get(k,0) for k in keys)}; after {[a9.get(k) for k in keys]} sum {sum(a9.get(k,0) for k in keys)}")
    print(f"  boot-library edges present before and gone after: {len(boot_edges_lost)} {sorted(boot_edges_lost)[:5]}")
    print(f"  added: {' '.join(p.removeprefix('pkg:hackage/') for p in added)}")
