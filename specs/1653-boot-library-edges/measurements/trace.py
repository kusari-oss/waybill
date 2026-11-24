# Spec 1653 T019: per target and compiler, boot-library edges before/after; SC-002 base check;
# ghc library's edges preserved; no duplicate dependsOn; every changed edge is boot->boot same compiler.
import json, sys, re, collections
before_dir, after_dir, tsv = sys.argv[1], sys.argv[2], sys.argv[3]
table = collections.defaultdict(dict)
for line in open(tsv):
    if line.startswith('#') or line.startswith('ghc\t'): continue
    r, lib, ver, dep, lay = line.rstrip('\n').split('\t')
    table[r][lib] = [] if dep == '-' else dep.split(' ')
def walk(cs):
    for c in cs or []:
        yield c; yield from walk(c.get('components'))
def load(d, t):
    doc = json.load(open(f'{d}/nix-closure-{t}/cdx.json'))
    boots = {}
    for c in walk(doc.get('components')):
        p = {x['name']: x['value'] for x in c.get('properties', [])}
        if 'waybill:boot-library-compiler' in p:
            boots[c['purl']] = (p['waybill:boot-library-compiler'], c['name'])
    deps = {x['ref']: x.get('dependsOn') or [] for x in doc.get('dependencies', [])}
    mp = {x['name']: x['value'] for x in doc['metadata'].get('properties', [])}
    return doc, boots, deps, mp
ok = True
print('| target | compiler | boot libs with edges before → after | boot edges before → after | base edges = table | completeness after |')
print('|---|---|---|---|---|---|')
for t in ['moat', 'shellcheck', 'slack-web', 'openai', 'slacklinker']:
    _, bb, db, _ = load(before_dir, t); _, ba, da, ma = load(after_dir, t)
    for comp in sorted({v[0] for v in ba.values()}):
        rel = re.sub(r'^ghc-(binary-)?', '', comp)
        mine = {p for p, v in ba.items() if v[0] == comp}
        names = {ba[p][1]: p for p in mine}
        be = sum(1 for p in mine for q in db.get(p, []) if q in mine)
        ae = sum(1 for p in mine for q in da.get(p, []) if q in mine)
        bw = sum(1 for p in mine if any(q in mine for q in db.get(p, [])))
        aw = sum(1 for p in mine if any(q in mine for q in da.get(p, [])))
        base = names.get('base')
        want = sorted(names[d] for d in table[rel].get('base', []) if d in names)
        got = sorted(q for q in da.get(base, []) if q in mine) if base else []
        good = (got == want)
        ok &= good
        print(f'| {t} | {comp} | {bw} → {aw} | {be} → {ae} | {"yes" if good else "NO " + str(got) + " vs " + str(want)} | {ma.get("waybill:graph-completeness")} |')
    # ghc library's edges preserved, no duplicates, new edges only boot->boot same compiler
    for p, (comp, name) in ba.items():
        if name == 'ghc':
            if not set(db.get(p, [])) <= set(da.get(p, [])): ok = False; print('  ghc edges lost', t, comp)
    for ref, to in da.items():
        if len(to) != len(set(to)): ok = False; print('  duplicate dependsOn', t, ref)
        for q in set(to) - set(db.get(ref, [])):
            if not (ref in ba and q in ba and ba[ref][0] == ba[q][0]):
                ok = False; print('  unexplained new edge', t, ref, '->', q)
    for ref, to in db.items():
        if set(to) - set(da.get(ref, [])): ok = False; print('  edge removed', t, ref, set(to) - set(da.get(ref, [])))
print('\nALL CHECKS', 'PASS' if ok else 'FAIL')
