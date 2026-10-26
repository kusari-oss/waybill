import json,sys
from collections import Counter
def load(p): return json.load(open(p))
def purl(pk):
    for r in pk.get('externalRefs',[]):
        if r.get('referenceType')=='purl': return r.get('referenceLocator')
    return pk.get('name')+'@'+str(pk.get('versionInfo'))
def anns(d):
    c=Counter()
    for pk in d.get('packages',[]):
        k=purl(pk)
        for a in pk.get('annotations',[]):
            try:
                env=json.loads(a.get('comment',''))
                c[(k,env.get('field'),json.dumps(env.get('value'),sort_keys=True))]+=1
            except Exception:
                c[(k,'<non-envelope>',a.get('comment','')[:60])]+=1
    return c
def rels(d):
    # map SPDXID -> purl so IDs (which are content hashes) don't dominate
    idmap={pk['SPDXID']: purl(pk) for pk in d.get('packages',[])}
    idmap[d.get('SPDXID','SPDXRef-DOCUMENT')]='<DOC>'
    c=Counter()
    for r in d.get('relationships',[]):
        c[(idmap.get(r['spdxElementId'],r['spdxElementId']),
           r['relationshipType'],
           idmap.get(r['relatedSpdxElement'],r['relatedSpdxElement']))]+=1
    return c
o,n=load(sys.argv[1]),load(sys.argv[2])
ao,an=anns(o),anns(n)
print('ANNOTATIONS added:', sum((an-ao).values()), ' removed:', sum((ao-an).values()))
addf=Counter(f for (_,f,_),v in (an-ao).items() for _ in range(v))
remf=Counter(f for (_,f,_),v in (ao-an).items() for _ in range(v))
print('  added by field:', dict(addf))
print('  removed by field:', dict(remf))
for item,v in list((ao-an).items())[:6]: print('   REMOVED', item)
ro,rn=rels(o),rels(n)
print('RELATIONSHIPS added:', sum((rn-ro).values()), ' removed:', sum((ro-rn).values()))
print('  added by type:', dict(Counter(t for (_,t,_),v in (rn-ro).items() for _ in range(v))))
print('  removed by type:', dict(Counter(t for (_,t,_),v in (ro-rn).items() for _ in range(v))))
