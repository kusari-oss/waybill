"""#1455: how reliable is the output name as the documentation signal, and what
does the rule remove from one runtime closure?

usage: python3 -I -B analyze.py <collect.sh outdir> <root store path>...

Reads `pathinfo.json` (references) and `drvs.json` (each derivation's
`outputs.<name>.path`). Reports:
  - how many runtime paths a derivation output names, and with which name;
  - where the output name and the `-doc`/`-man`/... path suffix disagree;
  - the paths reachable from the roots without passing through a
    documentation output, and the derivations (by name) whose every runtime
    path lies outside that set.
"""
import json, os, re, sys
from collections import Counter

DOC = {"doc", "devdoc", "man", "info"}
d = sys.argv[1]
roots = [os.path.basename(p) for p in sys.argv[2:]]
base = lambda p: os.path.basename(p)
pi = json.load(open(os.path.join(d, "pathinfo.json")))
if isinstance(pi, dict) and "info" in pi:
    pi = pi["info"]
if isinstance(pi, list):
    pi = {x["path"]: x for x in pi}
refs = {base(p): [base(r) for r in (v or {}).get("references", []) if base(r) != base(p)] for p, v in pi.items()}
drvs = json.load(open(os.path.join(d, "drvs.json")))
drvs = drvs.get("derivations", drvs)
out_name, drv_of = {}, {}
for k, v in drvs.items():
    for n, o in (v.get("outputs") or {}).items():
        p = o.get("path")
        if p:
            out_name[base(p)] = n
            drv_of[base(p)] = v.get("name", k)
paths = set(refs)
named = {p: out_name[p] for p in paths if p in out_name}
print(f"runtime paths: {len(paths)}; named by a derivation output: {len(named)}; unnamed: {len(paths) - len(named)}")
print("output names:", dict(Counter(named.values()).most_common()))
suffix = lambda p: (m := re.search(r"-(doc|devdoc|man|info)$", p)) and m.group(1)
dis = [(p, named.get(p), suffix(p)) for p in sorted(paths) if (named.get(p) in DOC) != bool(suffix(p))]
print(f"name vs suffix disagreements: {len(dis)}")
for p, n, s in dis:
    print(f"  {p}: output name {n!r}, suffix {s!r}")
is_doc = lambda p: named.get(p) in DOC
seen, st = set(), [r for r in roots if not is_doc(r)]
while st:
    p = st.pop()
    if p in seen or is_doc(p):
        continue
    seen.add(p)
    st += refs.get(p, [])
docs = {p for p in paths if is_doc(p)}
print(f"documentation-output paths: {len(docs)}; reachable without them: {len(seen)}")
by_drv = {}
for p in paths:
    by_drv.setdefault(drv_of.get(p, "?" + p), set()).add(p)
only = sorted(n for n, ps in by_drv.items() if not (ps & seen))
print(f"derivations with every runtime path outside the non-doc set: {len(only)}")
for n in only:
    print("  ", n, sorted(named.get(p, "?") for p in by_drv[n]))
lost_nondoc = sorted(p for p in paths - seen if not is_doc(p))
print(f"non-documentation paths reachable only through a documentation output: {len(lost_nondoc)}")
for p in lost_nondoc:
    print("  ", p)
