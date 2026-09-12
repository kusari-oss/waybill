import json,time,http.client,threading,sys
from concurrent.futures import ThreadPoolExecutor
sys.path.insert(0, str(__import__('pathlib').Path(__file__).parent))
from measure import load_coords
uniq=load_coords(); N=3000
coords=(uniq*3)[:N]
local=threading.local()
def batched(size,workers):
    chunks=[coords[i:i+size] for i in range(0,len(coords),size)]
    def w(chunk):
        if not hasattr(local,'c'): local.c=http.client.HTTPSConnection('api.deps.dev',timeout=120)
        tok='';pages=0;got=0
        reqs=[{"versionKey":{"system":s,"name":n,"version":v}} for s,n,v in chunk]
        while True:
            b={"requests":reqs}
            if tok:b["pageToken"]=tok
            try:
                local.c.request('POST','/v3alpha/versionbatch',body=json.dumps(b),headers={'Content-Type':'application/json'})
                r=local.c.getresponse();d=json.loads(r.read())
            except Exception:
                local.c=http.client.HTTPSConnection('api.deps.dev',timeout=120);return 0,0
            pages+=1;got+=sum(1 for x in d.get('responses',[]) if 'version' in x)
            tok=d.get('nextPageToken','')
            if not tok:break
        return got,pages
    t=time.perf_counter()
    with ThreadPoolExecutor(max_workers=workers) as ex:res=list(ex.map(w,chunks))
    return time.perf_counter()-t,sum(x[1] for x in res),len(chunks)
print(f"workload: {N} components ({len(uniq)} unique, repeated) -> extrapolate to 7,592\n")
print(f"{'batch':>6} {'workers':>8} {'batches':>8} {'HTTP':>6} {'wall':>8} {'→7,592':>9}")
for size,w in ((100,8),(100,16),(100,32),(250,16),(500,16)):
    el,pages,nb=batched(size,w)
    print(f"{size:>6} {w:>8} {nb:>8} {pages:>6} {el:>7.2f}s {el*(7592/N):>8.1f}s")
