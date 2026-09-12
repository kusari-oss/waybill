import json, time, http.client, statistics, threading, sys
from concurrent.futures import ThreadPoolExecutor
sys.path.insert(0, str(__import__('pathlib').Path(__file__).parent))
from measure import load_coords, get_one
HOST='api.deps.dev'; N=800
coords=load_coords()[:N]
local=threading.local()
def conn():
    if not hasattr(local,'c'): local.c=http.client.HTTPSConnection(HOST,timeout=120)
    return local.c

def per_component(workers):
    def w(c):
        try: return get_one(conn(),c)[0]
        except Exception:
            local.c=http.client.HTTPSConnection(HOST,timeout=120); return 0
    t=time.perf_counter()
    with ThreadPoolExecutor(max_workers=workers) as ex: r=list(ex.map(w,coords))
    return time.perf_counter()-t, sum(1 for x in r if x==200)

def batched(size, workers):
    """Issue batches concurrently; follow pagination within each batch serially."""
    chunks=[coords[i:i+size] for i in range(0,len(coords),size)]
    def w(chunk):
        got=0; tok=''; reqs=[{"versionKey":{"system":s,"name":n,"version":v}} for s,n,v in chunk]
        pages=0
        while True:
            b={"requests":reqs}
            if tok: b["pageToken"]=tok
            c=conn()
            try:
                c.request('POST','/v3alpha/versionbatch',body=json.dumps(b),
                          headers={'Content-Type':'application/json'})
                r=c.getresponse(); d=json.loads(r.read())
            except Exception:
                local.c=http.client.HTTPSConnection(HOST,timeout=120); return 0,0
            pages+=1
            got+=sum(1 for x in d.get('responses',[]) if 'version' in x)
            tok=d.get('nextPageToken','')
            if not tok: break
        return got,pages
    t=time.perf_counter()
    with ThreadPoolExecutor(max_workers=workers) as ex: res=list(ex.map(w,chunks))
    el=time.perf_counter()-t
    return el, sum(x[0] for x in res), sum(x[1] for x in res), len(chunks)

print(f"workload: {N} real components; extrapolation target 7,592\n")
rows=[]
el,ok=per_component(1)
rows.append(("per-component, sequential",el,ok,N))
for w in (8,16):
    el,ok=per_component(w); rows.append((f"per-component, {w}-way",el,ok,N))
for size,w in ((100,8),(100,16),(500,8),(5000,8)):
    el,ok,pages,nb=batched(size,w)
    rows.append((f"batch={size}, {w}-way  ({nb} batches, {pages} HTTP)",el,ok,N))

print(f"{'strategy':<46} {'wall':>8} {'enriched':>9} {'→7,592 est':>12}")
base=None
for name,el,ok,n in rows:
    est=el*(7592/n)
    if base is None: base=est
    print(f"{name:<46} {el:>7.2f}s {ok:>9} {est:>10.1f}s   {base/est:>5.1f}x")
