import json,sys
from collections import Counter
def build(p):
    d=json.load(open(p)); g=d.get('@graph',[])
    idmap={e.get('spdxId'): (e.get('software_packageUrl') or e.get('name')) for e in g if 'Package' in str(e.get('type',''))}
    flat=Counter()
    for e in g:
        if 'Relationship' in e.get('type',''):
            frm=idmap.get(e.get('from'),e.get('from'))
            for x in e.get('to',[]):
                flat[(e.get('relationshipType'), e.get('scope'), frm, idmap.get(x,x))]+=1
    return flat
o,n=build(sys.argv[1]),build(sys.argv[2])
add,rem=n-o,o-n
print('flattened edges: added', sum(add.values()), 'removed', sum(rem.values()))
print('  added by (type,scope):', dict(Counter((t,s) for (t,s,_,_),v in add.items() for _ in range(v))))
print('  removed by (type,scope):', dict(Counter((t,s) for (t,s,_,_),v in rem.items() for _ in range(v))))
# are the removed and added the same (from,to) pairs, differing only in scope?
ap={(f,t) for (_,_,f,t) in add}; rp={(f,t) for (_,_,f,t) in rem}
print('  endpoint pairs: added-only', len(ap-rp), 'removed-only', len(rp-ap), 'retyped', len(ap&rp))
for x in list(ap-rp)[:4]: print('   NEW EDGE', x)
for x in list(rp-ap)[:4]: print('   LOST EDGE', x)
