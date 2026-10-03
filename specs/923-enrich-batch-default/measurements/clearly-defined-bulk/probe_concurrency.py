"""Warm bulk batches sent with 1, 2, 4 and 7 workers.

Usage: probe_concurrency.py <cdx.json>...   (same coordinate set as probe_batches.py)
"""
import json, sys, time
from concurrent.futures import ThreadPoolExecutor
from probe_semantics import post
from probe_batches import coord

coords = []
for f in sys.argv[1:]:
    for c in json.load(open(f)).get("components", []):
        k = coord(c["purl"]) if c.get("purl") else None
        if k and k not in coords:
            coords.append(k)
batches = [coords[i:i + 100] for i in range(0, len(coords), 100)]
for w in (1, 2, 4, 7):
    t = time.time()
    with ThreadPoolExecutor(w) as ex:
        res = list(ex.map(post, batches))
    print(f"workers={w} wall={time.time()-t:6.2f}s per-batch=[{', '.join(f'{r[2]:.2f}' for r in res)}] statuses={[r[0] for r in res]}", flush=True)
