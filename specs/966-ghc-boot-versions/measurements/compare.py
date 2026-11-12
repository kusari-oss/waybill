import sys, re, glob, os
hints={}; cur=None
for line in open(sys.argv[1]):
    m=re.match(r'^(ghc-[\d.]+):\s*$',line)
    if m: cur=m.group(1); hints[cur]={}; continue
    m=re.match(r'^\s+(\S+):\s*\'?([^\'\s]+)',line)
    if m and cur: hints[cur][m.group(1)]=m.group(2)
for sp in sys.argv[2:]:
    v=sp.rsplit('-ghc-',1)[1]
    db=f"{sp}/lib/ghc-{v}/lib/package.conf.d"
    store={}
    for c in glob.glob(db+'/*.conf'):
        t=open(c).read()
        n=re.search(r'^name:\s*(\S+)',t,re.M).group(1); ver=re.search(r'^version:\s*(\S+)',t,re.M).group(1)
        store.setdefault(n,set()).add(ver)
    h=hints.get('ghc-'+v,{})
    both=sorted(set(store)&set(h))
    agree=[n for n in both if store[n]=={h[n]}]
    disagree=[(n,sorted(store[n]),h[n]) for n in both if store[n]!={h[n]}]
    print(f"{os.path.basename(sp)}: store {len(store)} names, hints {len(h)}; shared {len(both)}, agree {len(agree)}, disagree {disagree}")
    print(f"   only in store: {sorted(set(store)-set(h))}")
    print(f"   only in hints: {sorted(set(h)-set(store))}")
