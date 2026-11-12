import sys, re, json
hints={}; cur=None
for line in open(sys.argv[1]):
    m=re.match(r'^(ghc-[\d.]+):\s*$',line)
    if m: cur=m.group(1)[4:]; hints[cur]={}; continue
    m=re.match(r'^\s+(\S+):\s*\'?([^\'\s]+)',line)
    if m and cur: hints[cur][m.group(1)]=m.group(2)
def walk(o):
    if isinstance(o,dict):
        yield o
        for v in o.values(): yield from walk(v)
    elif isinstance(o,list):
        for v in o: yield from walk(v)
for path in sys.argv[2:]:
    d=json.load(open(path))
    comps=[o for o in walk(d) if 'purl' in o and 'name' in o]
    ghcs={o['version'] for o in comps if o['name']=='ghc' and o.get('version')}
    vl=sorted({o['name'] for o in comps if o['purl'].startswith('pkg:hackage/') and not o.get('version')})
    for g in ghcs:
        t=hints.get(g,{})
        hit=[n for n in vl if n in t]; miss=[n for n in vl if n not in t]
        print(f"{path.split('/')[0]} ghc {g}: versionless hackage {len(vl)}, table resolves {len(hit)}, not in table {miss}")
    if not ghcs: print(f"{path.split('/')[0]}: no versioned ghc component; versionless {len(vl)}")
