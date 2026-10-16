"""Invariant-major corpus check: every invariant against every target.

Each invariant returns (ok, detail). The point is the matrix, not any
one cell: a defect class gets checked everywhere, not only on the
target where somebody noticed it.
"""
import json, glob, os, collections

INVERSE = {'DEV_DEPENDENCY_OF','OPTIONAL_DEPENDENCY_OF','TEST_DEPENDENCY_OF',
           'BUILD_DEPENDENCY_OF','PROVIDED_DEPENDENCY_OF','RUNTIME_DEPENDENCY_OF',
           'DEPENDENCY_OF'}
NONRUNTIME_SCOPES = {'optional','development','dev','test','build'}

def load(d):
    c = glob.glob(d+'/*-cdx.json'); s = glob.glob(d+'/*-spdx.json')
    return (json.load(open(c[0])) if c else None,
            json.load(open(s[0])) if s else None)

def props(c): return {p['name']:p['value'] for p in c.get('properties',[]) or []}
def is_file_tier(c):
    return c.get('type')=='file' or props(c).get('waybill:component-tier')=='file'
def nonruntime(c):
    return (c.get('scope')=='excluded'
            or props(c).get('waybill:lifecycle-scope','').lower() in NONRUNTIME_SCOPES)

def graph(cdx):
    deps = {e['ref']: e.get('dependsOn',[]) for e in cdx.get('dependencies',[])}
    subj = cdx.get('metadata',{}).get('component',{}).get('bom-ref')
    return deps, subj

def reach(deps, seeds):
    seen=set(seeds); q=list(seeds)
    while q:
        for t in deps.get(q.pop(),[]):
            if t not in seen: seen.add(t); q.append(t)
    return seen

# ---------------- invariants ----------------

def I1_no_dependson_file_tier(cdx, spdx):
    """A dependency edge must not target a file-tier component."""
    byref={c['bom-ref']:c for c in cdx.get('components',[])}
    deps,_=graph(cdx)
    bad=[(r,t) for r,ts in deps.items() for t in ts
         if t in byref and is_file_tier(byref[t])]
    return (not bad, f"{len(bad)} edges target file-tier components"
                     + (f"; e.g. -> {byref[bad[0][1]].get('name')}" if bad else ""))

def I2_identity_unique(cdx, spdx):
    """(name, version, type) identifies at most one component."""
    ident=collections.Counter((c.get('name'),c.get('version',''),c.get('type'))
                              for c in cdx.get('components',[]))
    dups={k:v for k,v in ident.items() if v>1}
    worst=max(dups.items(), key=lambda kv:kv[1]) if dups else None
    return (not dups, f"{len(dups)} duplicated identities"
                      + (f"; worst {worst[0][0]} x{worst[1]}" if worst else ""))

def I3_nonruntime_propagated(cdx, spdx):
    """A component reachable only via non-runtime paths must carry a
    non-runtime marker."""
    comps=cdx.get('components',[]); byref={c['bom-ref']:c for c in comps}
    deps,subj=graph(cdx)
    if not subj: return (True,"no subject")
    runtime_seeds=[subj]+[t for t in deps.get(subj,[])
                          if t in byref and not nonruntime(byref[t])]
    runtime_reach=reach(deps,runtime_seeds)
    nr_seeds=[c['bom-ref'] for c in comps if nonruntime(c)]
    nr_reach=reach(deps,nr_seeds) if nr_seeds else set()
    bad=[byref[r] for r in (nr_reach - runtime_reach)
         if r in byref and not nonruntime(byref[r]) and not is_file_tier(byref[r])]
    return (not bad, f"{len(bad)} dev-only transitives unmarked"
                     + (f"; e.g. {bad[0].get('name')}@{bad[0].get('version')}" if bad else ""))

def I4_cross_format_root_parity(cdx, spdx):
    """CDX and SPDX must agree on the root's direct dependency set."""
    if not spdx: return (True,"no spdx")
    byref={c['bom-ref']:c for c in cdx.get('components',[])}
    deps,subj=graph(cdx)
    cdx_set={byref[t].get('purl') for t in deps.get(subj,[]) if t in byref and byref[t].get('purl')}
    roots={r['relatedSpdxElement'] for r in spdx.get('relationships',[])
           if r['relationshipType']=='DESCRIBES'}
    pkg={p['SPDXID']:p for p in spdx.get('packages',[])}
    sp=set()
    for r in spdx.get('relationships',[]):
        t=r['relationshipType']
        if t=='DEPENDS_ON' and r['spdxElementId'] in roots: o=r['relatedSpdxElement']
        elif t in INVERSE and r['relatedSpdxElement'] in roots: o=r['spdxElementId']
        else: continue
        for ref in pkg.get(o,{}).get('externalRefs',[]) or []:
            if ref.get('referenceType')=='purl': sp.add(ref['referenceLocator'])
    only_c, only_s = cdx_set-sp, sp-cdx_set
    return (not only_c and not only_s,
            f"cdx={len(cdx_set)} spdx={len(sp)} only-cdx={len(only_c)} only-spdx={len(only_s)}")

def I5_complete_means_complete(cdx, spdx):
    """A `complete` verdict implies no component was excluded from the
    reachability judgement."""
    v=None
    for p in cdx.get('metadata',{}).get('properties',[]) or []:
        if p.get('name')=='waybill:graph-completeness': v=p.get('value')
    if v!='complete': return (True, f"verdict={v} (n/a)")
    comps=cdx.get('components',[]); deps,subj=graph(cdx)
    rr=reach(deps,[subj]) if subj else set()
    unreach=[c for c in comps if c['bom-ref'] not in rr]
    return (not unreach, f"verdict=complete but {len(unreach)} unreachable"
                         + (f" ({sum(1 for c in unreach if is_file_tier(c))} file-tier)" if unreach else ""))

def I6_one_unknown_spelling(cdx, spdx):
    """A package must not appear under several 'unknown version' spellings."""
    SENT=('0.0.0-unknown','unresolved','unknown','')
    byname=collections.defaultdict(set)
    for c in cdx.get('components',[]):
        if is_file_tier(c): continue
        v=(c.get('version') or '')
        if v in SENT or 'unknown' in v or 'unresolved' in v:
            byname[c.get('name','').replace('_','-').lower()].add(v or '(absent)')
    bad={k:v for k,v in byname.items() if len(v)>1}
    return (not bad, f"{len(bad)} packages with mixed unknown-version spellings"
                     + (f"; e.g. {list(bad)[0]} {sorted(list(bad.values())[0])}" if bad else ""))

def I7_no_silent_orphans(cdx, spdx):
    """Every component is reachable from the subject, or carries a
    marker explaining why it is not."""
    comps=cdx.get('components',[]); deps,subj=graph(cdx)
    if not subj: return (True,"no subject")
    rr=reach(deps,[subj])
    bad=[c for c in comps if c['bom-ref'] not in rr
         and not nonruntime(c) and not is_file_tier(c)]
    return (not bad, f"{len(bad)} unreachable and unexplained"
                     + (f"; e.g. {bad[0].get('name')}" if bad else ""))

INVARIANTS=[("I1 no dependsOn->file-tier",I1_no_dependson_file_tier),
            ("I2 identity unique",I2_identity_unique),
            ("I3 non-runtime propagated",I3_nonruntime_propagated),
            ("I4 cross-format root parity",I4_cross_format_root_parity),
            ("I5 complete means complete",I5_complete_means_complete),
            ("I6 one unknown spelling",I6_one_unknown_spelling),
            ("I7 no silent orphans",I7_no_silent_orphans)]

targets=sorted(glob.glob('all/waybill-nightly-*'))
names=[os.path.basename(t).replace('waybill-nightly-','').replace('-image','') for t in targets]
docs=[load(t) for t in targets]

w=max(len(n) for n,_ in INVARIANTS)+2
hdr=' '*w+''.join(f"{n[:11]:>13}" for n in names)
print(hdr); print('-'*len(hdr))
details=[]
for iname,fn in INVARIANTS:
    row=f"{iname:<{w}}"
    for n,(cdx,spdx) in zip(names,docs):
        try: ok,det=fn(cdx,spdx)
        except Exception as e: ok,det=False,f"ERROR {e}"
        row+=f"{('  PASS' if ok else '  FAIL'):>13}"
        if not ok: details.append((iname,n,det))
    print(row)
print()
print("FAILURES")
for i,n,d in details: print(f"  {i:<30} {n:<22} {d}")
tot=len(INVARIANTS)*len(names)
print(f"\n{len(details)} failing cells of {tot} ({tot-len(details)} pass)")
