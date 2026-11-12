import sys, re, collections
blocks = collections.OrderedDict(); cur=None
for line in open(sys.argv[1]):
    m = re.match(r'^(ghc-[\d.]+):\s*$', line)
    if m: cur=m.group(1); blocks[cur]={}; continue
    m = re.match(r'^\s+(\S+):\s*\'?([^\'\s]+)', line)
    if m and cur: blocks[cur][m.group(1)]=m.group(2)
series = collections.defaultdict(list)
for g in blocks:
    v=g[4:].split('.')
    if v[0]=='9' and int(v[1])>=0 and len(v)==3: series['.'.join(v[:2])].append(g)
for s, gs in series.items():
    names=set().union(*(blocks[g] for g in gs))
    varying=sorted(n for n in names if len({blocks[g].get(n) for g in gs})>1 and n not in ('ghc','ghc-boot','ghc-boot-th','ghc-heap','ghci','libiserv','ghc-internal','ghc-toolchain','ghc-platform','ghc-experimental'))
    print(f"{s}: {len(gs)} patch releases, {len(names)} pkgs, non-ghc-versioned pkgs that vary within series: {len(varying)} {varying}")
