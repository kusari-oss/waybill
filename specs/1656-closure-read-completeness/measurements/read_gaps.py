# usage: read_gaps.py <public_corpus dir>
# Per closure golden: C104, C105, the two closure-read disclosures
# (waybill:nix-closure-read, waybill:nix-closure-contents), and whether the
# CycloneDX root compositions record carries a dependency claim.
import json, sys, os
corpus = sys.argv[1]
for t in sorted(os.listdir(corpus)):
    if not t.startswith('nix-closure'): continue
    d = json.load(open(os.path.join(corpus, t, 'cdx.json')))
    m = {p['name']: p['value'] for p in d['metadata'].get('properties', [])}
    r = json.loads(m.get('waybill:nix-closure-read', '{}'))
    c = json.loads(m.get('waybill:nix-closure-contents', '{}'))
    unread = c.get('absent', 0) + c.get('partial', 0) + c.get('failed', 0)
    root = [x for x in d.get('compositions', []) if x.get('aggregate') != 'complete']
    print(f"{t}\tC104={m.get('waybill:graph-completeness')}\t"
          f"declarations-unconsulted={r.get('declarations-unconsulted')}\tderivations-affected={r.get('derivations-affected')}\t"
          f"carrier-disagreements={r.get('carrier-disagreements', 0)}\t"
          f"contents members={c.get('members')} unread={unread} (absent {c.get('absent')} partial {c.get('partial')} failed {c.get('failed')})\t"
          f"root-record-dependencies={[len(x.get('dependencies', [])) for x in root]}\t"
          f"C105={m.get('waybill:graph-completeness-reason')}")
