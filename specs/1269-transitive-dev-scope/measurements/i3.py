import json,sys
from collections import deque
d=json.load(open(sys.argv[1]))
comps=d.get('components',[])
def ref(c): return c.get('bom-ref') or c.get('purl')
def nonrt(c):
    return any(p.get('name')=='waybill:lifecycle-scope' for p in c.get('properties',[]))
marked={ref(c) for c in comps if nonrt(c)}
root=d.get('metadata',{}).get('component',{})
root_ref=root.get('bom-ref') or root.get('purl')
adj={}
for dep in d.get('dependencies',[]):
    adj[dep['ref']]=dep.get('dependsOn',[])
def bfs(skip_marked):
    seen={root_ref}; q=deque([root_ref])
    while q:
        cur=q.popleft()
        for n in adj.get(cur,[]):
            if skip_marked and n in marked: continue
            if n not in seen: seen.add(n); q.append(n)
    return seen
rt=bfs(True); allr=bfs(False)
off=sorted(r for r in allr if r not in rt and r not in marked)
derivs=sum(1 for c in comps if any(p.get('name')=='waybill:lifecycle-scope-derivation' for p in c.get('properties',[])))
print(f"components={len(comps)} marked={len(marked)} c199_transitive={derivs}")
print(f"reachable={len(allr)} runtime_reachable={len(rt)}")
print(f"I3 offenders={len(off)}")
for o in off[:10]: print("  ", o)
