# Spec 1653 FR-001: per-release coverage, agreement with m966, per-target yield on the goldens.
import json, os, sys, re, collections
probe_dir, tsv, corpus = sys.argv[1], sys.argv[2], sys.argv[3]
vkey = lambda v: [int(x) for x in v.split('.')]
deps = {}
for f in os.listdir(probe_dir):
    if f.endswith('.json'):
        j = json.load(open(os.path.join(probe_dir, f))); deps[j['release']] = j
m966 = collections.defaultdict(dict)
for line in open(tsv):
    if line.startswith('#') or not line.strip() or not line[0].isdigit(): continue
    rel, lib, ver, src, plat = line.rstrip('\n').split('\t')[:5]
    if plat != 'windows': m966[rel][lib] = ver
print('## Per release')
print('| release | layout | libraries | edges | m966 libraries | only in probe | only in m966 | version disagreements |')
print('|---|---|---|---|---|---|---|---|')
for rel in sorted(m966, key=vkey):
    j = deps.get(rel)
    if not j: print(f'| {rel} | MISSING | | | {len(m966[rel])} | | | |'); continue
    L = j['libraries']
    only_p = sorted(set(L) - set(m966[rel])); only_m = sorted(set(m966[rel]) - set(L))
    vd = sorted(f"{k} {L[k]['version']}≠{m966[rel][k]}" for k in set(L) & set(m966[rel]) if L[k]['version'] and L[k]['version'] != m966[rel][k])
    print(f"| {rel} | {j['layout']} | {len(L)} | {sum(len(v['depends']) for v in L.values())} | {len(m966[rel])} | {', '.join(only_p) or '—'} | {', '.join(only_m) or '—'} | {'; '.join(vd) or '—'} |")
def walk(cs):
    for c in cs or []:
        yield c; yield from walk(c.get('components'))
print('\n## Per closure target (goldens at main)')
print('| target | compiler | boot libs | in table | with ≥1 edge after | edges after | deps not in document | known leaves |')
print('|---|---|---|---|---|---|---|---|')
for t in ['moat', 'shellcheck', 'slack-web', 'openai', 'slacklinker']:
    d = json.load(open(f'{corpus}/nix-closure-{t}/cdx.json'))
    by = collections.defaultdict(dict)
    for c in walk(d.get('components')):
        p = {x['name']: x['value'] for x in c.get('properties', [])}
        if 'waybill:boot-library-compiler' in p:
            by[p['waybill:boot-library-compiler']][c.get('name')] = c
    for comp in sorted(by):
        rel = re.sub(r'^ghc-(binary-)?', '', comp)
        j = deps.get(rel); libs = by[comp]
        if not j:
            print(f'| {t} | {comp} | {len(libs)} | release absent | 0 | 0 | — | — |'); continue
        L = j['libraries']; intable = [n for n in libs if n in L]
        edges = 0; withe = 0; missing = collections.Counter(); leaves = 0
        for n in intable:
            ds = L[n]['depends']
            if not ds: leaves += 1
            present = [x for x in ds if x in libs]
            for x in ds:
                if x not in libs: missing[x] += 1
            edges += len(present); withe += bool(present)
        print(f"| {t} | {comp} | {len(libs)} | {len(intable)} | {withe} | {edges} | {', '.join(f'{k}×{v}' for k, v in sorted(missing.items())) or '—'} | {leaves} |")
