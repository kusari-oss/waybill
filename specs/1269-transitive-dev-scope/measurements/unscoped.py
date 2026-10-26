import json,sys
from collections import deque
d=json.load(open(sys.argv[1]))
comps=d.get('components',[])
def ref(c): return c.get('bom-ref') or c.get('purl')
def nonrt(c): return any(p.get('name')=='waybill:lifecycle-scope' for p in c.get('properties',[]))
marked={ref(c) for c in comps if nonrt(c)}
root=d.get('metadata',{}).get('component',{}); root_ref=root.get('bom-ref') or root.get('purl')
adj={dep['ref']: dep.get('dependsOn',[]) for dep in d.get('dependencies',[])}
# runtime-reachable from the subject, not through a non-runtime component
seen={root_ref}; q=deque([root_ref])
while q:
    cur=q.popleft()
    for n in adj.get(cur,[]):
        if n in marked: continue
        if n not in seen: seen.add(n); q.append(n)
rt=seen
# reachable from ANY declared non-runtime component
seen2=set(); q=deque(marked)
for m in marked: seen2.add(m)
while q:
    cur=q.popleft()
    for n in adj.get(cur,[]):
        if n not in seen2: seen2.add(n); q.append(n)
unscoped=sorted(r for r in seen2 if r not in marked and r not in rt)
print(f"marked(non-runtime)={len(marked)}  reachable-from-a-non-runtime={len(seen2)}")
print(f"UNSCOPED TRANSITIVES = {len(unscoped)}")
for u in unscoped[:12]: print("  ", u)
