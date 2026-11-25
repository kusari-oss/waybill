# usage: unchanged.py <before public_corpus> <after public_corpus>
# Per closure target: components present before whose CycloneDX object differs
# after, other than in the C247 property (grouped by which keys differ);
# CycloneDX metadata properties that changed; SPDX 2.3 / SPDX 3 package
# counts before and after.
import json, sys, os, collections
b_dir, a_dir = sys.argv[1], sys.argv[2]
C247 = 'waybill:boot-library-dependencies'
def walk(cs):
    for c in cs:
        yield c; yield from walk(c.get('components', []))
def norm(c):
    c = {k: v for k, v in c.items() if k != 'components'}
    c['properties'] = [p for p in c.get('properties', []) if p['name'] != C247]
    return c
def spdx3_pkgs(d):
    g = d.get('@graph', d.get('graph', []))
    return sum(1 for e in g if e.get('type') == 'software_Package')
for t in sorted(os.listdir(a_dir)):
    if not t.startswith('nix-closure'): continue
    B = json.load(open(os.path.join(b_dir, t, 'cdx.json'))); A = json.load(open(os.path.join(a_dir, t, 'cdx.json')))
    bc = {c['bom-ref']: c for c in walk(B['components'])}; ac = {c['bom-ref']: c for c in walk(A['components'])}
    diff = collections.Counter()
    for r, c in bc.items():
        if r not in ac: diff['missing'] += 1; continue
        x, y = norm(c), norm(ac[r])
        if x != y:
            keys = tuple(sorted(k for k in set(x) | set(y) if x.get(k) != y.get(k)))
            diff[keys] += 1
    mp = lambda d: {p['name']: p['value'] for p in d['metadata'].get('properties', [])}
    mb, ma = mp(B), mp(A)
    mchanged = sorted(k for k in set(mb) | set(ma) if mb.get(k) != ma.get(k))
    s2 = lambda d: len(json.load(open(os.path.join(d, t, 'spdx-2.3.json')))['packages'])
    s3 = lambda d: spdx3_pkgs(json.load(open(os.path.join(d, t, 'spdx-3.json'))))
    print(f"{t}: pre-existing components changed beyond C247: {dict(diff) or 0}; metadata properties changed: {mchanged}; SPDX 2.3 packages {s2(b_dir)} -> {s2(a_dir)}; SPDX 3 packages {s3(b_dir)} -> {s3(a_dir)}")
