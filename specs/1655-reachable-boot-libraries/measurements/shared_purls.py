# usage: shared_purls.py <ghc_boot_libraries.tsv> <ghc_boot_dependencies.tsv> <public_corpus dir>
# For each closure golden and compiler (C218), split the reachable-but-absent
# boot libraries (reach.py's "add") into those whose PURL
# pkg:hackage/<name>@<966 version for that release> is ALREADY a component of
# the document (held under another compiler's C218 label, because one version
# is one component), and those no component holds.
import json, sys, os, re
t966, t1653, corpus = sys.argv[1:4]
ver = {}
for l in open(t966):
    if l.startswith('#') or l.startswith('ghc\t'): continue
    g, lib, v, _src, plat = l.rstrip('\n').split('\t')
    if plat in ('all', 'linux', 'unix'): ver[(g, lib)] = v
dep = {}
for l in open(t1653):
    if l.startswith('#') or l.startswith('ghc\t'): continue
    g, lib, _v, d, _ = l.rstrip('\n').split('\t')
    dep[(g, lib)] = [] if d in ('', '-') else d.split()
def walk(cs):
    for c in cs:
        yield c; yield from walk(c.get('components', []))
for t in sorted(os.listdir(corpus)):
    if not t.startswith('nix-closure'): continue
    d = json.load(open(os.path.join(corpus, t, 'cdx.json')))
    purls = {c.get('purl') for c in walk(d['components'])}
    per = {}
    for c in walk(d['components']):
        p = {x['name']: x['value'] for x in c.get('properties', [])}
        if 'waybill:boot-library-compiler' in p:
            per.setdefault(p['waybill:boot-library-compiler'], set()).add(c['name'])
    for comp, present in sorted(per.items()):
        rel = re.search(r'(\d+\.\d+\.\d+)$', comp).group(1)
        seen, stack = set(present), list(present)
        while stack:
            for n in dep.get((rel, stack.pop()), []):
                if n not in seen: seen.add(n); stack.append(n)
        held, new = [], []
        for n in sorted(seen - present):
            v = ver.get((rel, n))
            (held if f'pkg:hackage/{n}@{v}' in purls else new).append(f'{n}@{v}')
        print(f"{t}\t{comp}\treachable-absent={len(held)+len(new)}\theld-under-other-label={len(held)}\tno-component={len(new)}")
        print(f"\t\theld: {' '.join(held)}\n\t\tnew:  {' '.join(new)}")
