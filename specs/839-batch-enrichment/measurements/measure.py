"""Measure real deps.dev latency. No predictions - only observations."""
import json, re, time, statistics, http.client, urllib.parse, glob, sys, threading
from concurrent.futures import ThreadPoolExecutor

HOST = 'api.deps.dev'
SYS = {'cargo':'CARGO','npm':'NPM','pypi':'PYPI','maven':'MAVEN',
       'nuget':'NUGET','golang':'GO','gem':'RUBYGEMS'}

def load_coords():
    coords = []
    for blk in open('Cargo.lock').read().split('[[package]]'):
        n = re.search(r'^name = "([^"]+)"', blk, re.M)
        v = re.search(r'^version = "([^"]+)"', blk, re.M)
        if n and v:
            coords.append(('CARGO', n.group(1), v.group(1)))
    purls = set()
    for f in glob.glob('waybill-cli/tests/fixtures/public_corpus/*/cdx.json'):
        purls |= set(re.findall(r'"purl"\s*:\s*"([^"]+)"', open(f).read()))
    for p in purls:
        m = re.match(r'pkg:([a-z]+)/(.+?)@([^?#]+)', p)
        if not m:
            continue
        s = SYS.get(m.group(1))
        if not s:
            continue
        name = urllib.parse.unquote(m.group(2))
        ver = urllib.parse.unquote(m.group(3))
        if s == 'MAVEN':
            name = name.replace('/', ':')
        coords.append((s, name, ver))
    seen, out = set(), []
    for c in coords:
        if c not in seen:
            seen.add(c); out.append(c)
    return out

def get_one(conn, c):
    sysname, name, ver = c
    path = f"/v3/systems/{sysname.lower()}/packages/{urllib.parse.quote(name, safe='')}/versions/{urllib.parse.quote(ver, safe='')}"
    conn.request('GET', path)
    r = conn.getresponse()
    body = r.read()
    return r.status, len(body)

def seq_measure(coords, n):
    conn = http.client.HTTPSConnection(HOST, timeout=30)
    lat, hits = [], 0
    for c in coords[:n]:
        t = time.perf_counter()
        try:
            st, _ = get_one(conn, c)
        except Exception:
            conn.close(); conn = http.client.HTTPSConnection(HOST, timeout=30)
            continue
        lat.append((time.perf_counter() - t) * 1000)
        hits += (st == 200)
    conn.close()
    return lat, hits

def conc_measure(coords, n, workers):
    local = threading.local()
    def work(c):
        if not hasattr(local, 'conn'):
            local.conn = http.client.HTTPSConnection(HOST, timeout=30)
        try:
            return get_one(local.conn, c)[0]
        except Exception:
            local.conn = http.client.HTTPSConnection(HOST, timeout=30)
            return 0
    t = time.perf_counter()
    with ThreadPoolExecutor(max_workers=workers) as ex:
        res = list(ex.map(work, coords[:n]))
    return (time.perf_counter() - t), sum(1 for r in res if r == 200)

def batch_measure(coords, size, reps=2):
    lat, sizes, found = [], [], []
    pool = (coords * ((size // len(coords)) + 2))[:size]
    body = json.dumps({"requests": [{"versionKey": {"system": s, "name": n, "version": v}}
                                     for s, n, v in pool]})
    for _ in range(reps):
        conn = http.client.HTTPSConnection(HOST, timeout=120)
        t = time.perf_counter()
        conn.request('POST', '/v3alpha/versionbatch', body=body,
                     headers={'Content-Type': 'application/json'})
        r = conn.getresponse(); raw = r.read()
        lat.append((time.perf_counter() - t) * 1000)
        sizes.append(len(raw))
        if r.status == 200:
            d = json.loads(raw)
            found.append(sum(1 for x in d.get('responses', []) if 'version' in x))
        conn.close()
    return lat, sizes, found, r.status

if __name__ == '__main__':
    coords = load_coords()
    print(f"dataset: {len(coords)} unique real coordinates")
    print(f"  by system: " + ", ".join(f"{s}={sum(1 for c in coords if c[0]==s)}"
          for s in sorted({c[0] for c in coords})))
    print()

    print("=== 1. SEQUENTIAL (v3 GetVersion, one connection reused) ===")
    lat, hits = seq_measure(coords, 40)
    print(f"  n={len(lat)}  hits={hits}  median={statistics.median(lat):.0f}ms  "
          f"mean={statistics.mean(lat):.0f}ms  p90={sorted(lat)[int(len(lat)*.9)]:.0f}ms")
    seq_median = statistics.median(lat)
    print()

    print("=== 2. CONCURRENT (v3 GetVersion, 8 workers, pooled) ===")
    for w in (8, 16):
        el, ok = conc_measure(coords, 240, w)
        print(f"  workers={w:2d}  n=240  ok={ok}  wall={el:.2f}s  "
              f"rate={240/el:.1f} req/s  effective={el/240*1000:.0f}ms/req")
    print()

    print("=== 3. BATCH (v3alpha versionbatch, by size) ===")
    print(f"  {'size':>6} {'median ms':>10} {'ms/entry':>10} {'resp KB':>9} {'with data':>10}")
    for size in (50, 100, 250, 500, 1000):
        lat, sizes, found, st = batch_measure(coords, size)
        if st != 200:
            print(f"  {size:>6}  HTTP {st}")
            continue
        m = statistics.median(lat)
        print(f"  {size:>6} {m:>10.0f} {m/size:>10.2f} {statistics.median(sizes)/1024:>9.0f} "
              f"{statistics.median(found) if found else 0:>10.0f}")
