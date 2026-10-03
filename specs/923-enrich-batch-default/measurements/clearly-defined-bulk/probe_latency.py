"""Per-batch latency of POST /definitions on (mostly) cold coordinates.

Usage: probe_latency.py <cdx.json> [concurrency]
Sends batches of 100 with a 120 s client timeout, BULK_CONCURRENCY at a
time, and prints each batch's latency and outcome, then a distribution.
"""
import json, sys, time
from concurrent.futures import ThreadPoolExecutor
sys.path.insert(0, __import__("os").path.dirname(__file__) or ".")
from probe_semantics import post, declared
from probe_batches import coord

coords = []
for c in json.load(open(sys.argv[1])).get("components", []):
    k = coord(c["purl"]) if c.get("purl") else None
    if k and k not in coords:
        coords.append(k.replace("%40", "@"))
batches = [coords[i:i + 100] for i in range(0, len(coords), 100)]
workers = int(sys.argv[2]) if len(sys.argv) > 2 else 4
print(f"coords={len(coords)} batches={len(batches)} workers={workers}", flush=True)

def run(i):
    s, b, dt = post(batches[i])
    got = sum(1 for c in batches[i] if isinstance(b, dict) and declared(b.get(c)))
    print(f"batch {i:2} status={s} {dt:6.2f}s declared={got}", flush=True)
    return dt, s

t = time.time()
with ThreadPoolExecutor(workers) as ex:
    res = list(ex.map(run, range(len(batches))))
ok = sorted(dt for dt, s in res if s == 200)
bad = sorted(dt for dt, s in res if s != 200)
q = lambda xs, p: xs[min(len(xs) - 1, int(p * len(xs)))] if xs else None
print(f"wall={time.time()-t:.1f}s ok={len(ok)} p50={q(ok,.5)} p90={q(ok,.9)} max={max(ok) if ok else None} failed={len(bad)} failed_at={bad}")
