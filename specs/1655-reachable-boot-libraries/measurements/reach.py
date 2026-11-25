# usage: reach.py <tsv> <public_corpus dir>
# Per closure golden and compiler (C218): boot libraries present, the first-hop
# names in C247 `absent`, and the transitive boot-dependency closure of the
# present set minus what is present (what "emit the reachable boot libraries"
# would add), from the committed table.
import json, sys, os, re
tsv, corpus = sys.argv[1], sys.argv[2]
dep = {}
for line in open(tsv):
    if line.startswith('#') or line.startswith('ghc\t'): continue
    g, lib, ver, deps, _ = line.rstrip('\n').split('\t')
    dep[(g, lib)] = (ver, [] if deps in ('', '-') else deps.split())
def walk(cs):
    for c in cs:
        yield c
        yield from walk(c.get('components', []))
for t in sorted(os.listdir(corpus)):
    if not t.startswith('nix-closure'): continue
    d = json.load(open(os.path.join(corpus, t, 'cdx.json')))
    per = {}
    for c in walk(d.get('components', [])):
        p = {x['name']: x['value'] for x in c.get('properties', [])}
        comp = p.get('waybill:boot-library-compiler')
        if not comp: continue
        s = per.setdefault(comp, {'present': set(), 'absent': set(), 'reason': set()})
        s['present'].add(c['name']); s.setdefault('scope', {}).setdefault(c.get('scope','required'), 0); s['scope'][c.get('scope','required')] += 1
        j = json.loads(p.get('waybill:boot-library-dependencies', '{}'))
        s['absent'].update(j.get('absent', []))
        if 'reason' in j: s['reason'].add(j['reason'])
    for comp, s in sorted(per.items()):
        rel = re.search(r'(\d+\.\d+\.\d+)$', comp).group(1)
        seen, stack, missing = set(s['present']), list(s['present']), set()
        while stack:
            lib = stack.pop()
            e = dep.get((rel, lib))
            if e is None: missing.add(lib); continue
            for n in e[1]:
                if n not in seen: seen.add(n); stack.append(n)
        add = sorted(seen - s['present'])
        whole = sorted(l for (g, l) in dep if g == rel)
        print(f"{t}\t{comp}\tpresent={len(s['present'])}\tfirst-hop-absent={len(s['absent'])}\treachable-add={len(add)}\twhole-set={len(whole)}\tnot-in-table={sorted(missing)}\treasons={sorted(s['reason'])}\tscope={s['scope']}")
        print(f"\t\tadd: {' '.join(add)}")
