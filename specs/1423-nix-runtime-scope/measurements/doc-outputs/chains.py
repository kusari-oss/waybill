"""#1455: the shortest reference chain from the roots to each named derivation,
avoiding documentation outputs, and the non-documentation paths that directly
reference it.

usage: python3 -I -B chains.py <collect.sh outdir> <name regex> <root store path>...
"""
import json, os, re, sys
from collections import deque

DOC = {"doc", "devdoc", "man", "info"}
d, pat = sys.argv[1], re.compile(sys.argv[2])
roots = [os.path.basename(p) for p in sys.argv[3:]]
base = os.path.basename
pi = json.load(open(os.path.join(d, "pathinfo.json")))
if isinstance(pi, dict) and "info" in pi:
    pi = pi["info"]
if isinstance(pi, list):
    pi = {x["path"]: x for x in pi}
refs = {base(p): [base(r) for r in (v or {}).get("references", []) if base(r) != base(p)] for p, v in pi.items()}
drvs = json.load(open(os.path.join(d, "drvs.json")))
drvs = drvs.get("derivations", drvs)
named, dname = {}, {}
for k, v in drvs.items():
    for n, o in (v.get("outputs") or {}).items():
        if o.get("path"):
            named[base(o["path"])] = n
            dname[base(o["path"])] = v.get("name", k)
is_doc = lambda p: named.get(p) in DOC
strip = lambda p: re.sub(r"^[a-z0-9]{32}-", "", p)
prev, q = {r: None for r in roots if not is_doc(r)}, deque(r for r in roots if not is_doc(r))
while q:
    p = q.popleft()
    for r in refs.get(p, []):
        if r not in prev and not is_doc(r):
            prev[r] = p
            q.append(r)
rev = {}
for p, rs in refs.items():
    for r in rs:
        rev.setdefault(r, []).append(p)
targets = sorted({dname[p] for p in refs if p in dname and pat.search(dname[p])})
print(f"derivations matching {pat.pattern!r}: {len(targets)}")
for t in targets:
    ps = sorted(p for p in refs if dname.get(p) == t)
    for p in ps:
        if p in prev:
            chain, x = [], p
            while x:
                chain.append(f"{strip(x)}[{named.get(x, '?')}]")
                x = prev[x]
            via = sorted({strip(s) + "[" + named.get(s, "?") + "]" for s in rev.get(p, []) if not is_doc(s)})
            print(f"{t} {named.get(p)}: " + " -> ".join(reversed(chain)) + f"  | non-doc referrers: {len(via)}")
        else:
            print(f"{t} {named.get(p)}: reached only through a documentation output")
