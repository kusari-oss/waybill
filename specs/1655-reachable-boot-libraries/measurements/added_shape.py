# usage: added_shape.py <before public_corpus> <after public_corpus>
# Per closure target: the property-key set, CPE, scope and C247 `absent` of
# every component added (PURL not present before), grouped; and which other
# top-level CycloneDX sections changed.
import json, sys, os, collections
b_dir, a_dir = sys.argv[1], sys.argv[2]
def walk(cs):
    for c in cs:
        yield c; yield from walk(c.get('components', []))
for t in sorted(os.listdir(a_dir)):
    if not t.startswith('nix-closure'): continue
    B = json.load(open(os.path.join(b_dir, t, 'cdx.json'))); A = json.load(open(os.path.join(a_dir, t, 'cdx.json')))
    bp = {c.get('purl') for c in walk(B['components'])}
    shapes = collections.Counter()
    for c in walk(A['components']):
        if c.get('purl') in bp: continue
        p = {x['name']: x['value'] for x in c.get('properties', [])}
        shapes[(tuple(sorted(p)), 'cpe' in c, c.get('scope'), 'absent' in json.loads(p.get('waybill:boot-library-dependencies', '{}')))] += 1
    top = sorted(k for k in set(A) | set(B) if k not in ('components', 'dependencies', 'metadata') and A.get(k) != B.get(k))
    print(f"{t}: added (keys, has_cpe, scope, absent) {dict(shapes)}; other top-level sections changed: {top}")
