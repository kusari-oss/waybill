import json, http.client, sys
sys.path.insert(0, str(__import__('pathlib').Path(__file__).parent))
from measure import load_coords
coords = load_coords()
print(f"{'requested':>10} {'responses':>10} {'nextPageToken':>15}")
for size in (50, 99, 100, 101, 250, 1000):
    pool = (coords * 3)[:size]
    body = json.dumps({"requests":[{"versionKey":{"system":s,"name":n,"version":v}} for s,n,v in pool]})
    c = http.client.HTTPSConnection('api.deps.dev', timeout=120)
    c.request('POST','/v3alpha/versionbatch',body=body,headers={'Content-Type':'application/json'})
    r = c.getresponse(); d = json.loads(r.read()); c.close()
    tok = d.get('nextPageToken','')
    print(f"{size:>10} {len(d.get('responses',[])):>10} {('NON-EMPTY ('+str(len(tok))+' chars)') if tok else 'empty':>15}")
