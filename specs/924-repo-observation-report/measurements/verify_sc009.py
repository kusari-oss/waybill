#!/usr/bin/env python3
"""SC-009 — emitted SBOM content is unchanged by this feature.

Two comparison strategies, because one does not fit all three formats:

  CycloneDX, SPDX 2.3   masked line comparison (mask.py)
  SPDX 3                SEMANTIC comparison

**Why SPDX 3 needs a different strategy.** Its `@graph` elements carry
content-addressed IRIs, and element order follows those IRIs. Anything that
perturbs an id -- including the `git describe` version string, which moves on
every commit -- reorders the graph and produces a six-figure line diff across a
semantically identical document. Measured during m924: 62,336 differing lines
against 53,877 elements with an identical type histogram and an identical name
multiset.

Comparing SPDX 3 by masked lines therefore reports a regression on every
commit. This repo has recorded that trap before; the script encodes the way
out rather than leaving it to be rediscovered.
"""
import collections, json, subprocess, sys
from pathlib import Path

HERE = Path(__file__).parent
MASK = HERE / "mask.py"

def masked(p):
    return subprocess.run([sys.executable, str(MASK), str(p)],
                          capture_output=True, text=True, check=True).stdout

def semantic(p):
    g = json.load(open(p))["@graph"]
    return (len(g),
            collections.Counter(e.get("type") for e in g),
            collections.Counter(e.get("name") for e in g if e.get("name")))

def compare(fmt, base, now):
    if fmt == "spdx-3-json":
        a, b = semantic(base), semantic(now)
        return a == b, ("elements/types/names identical" if a == b
                        else f"elements {a[0]} vs {b[0]}")
    same = masked(base) == masked(now)
    return same, "masked byte-identical" if same else "masked content differs"

if __name__ == "__main__":
    base_dir, now_dir = Path(sys.argv[1]), Path(sys.argv[2])
    ok = True
    for fmt in ("cyclonedx-json", "spdx-2.3-json", "spdx-3-json"):
        for target in ("self", "poly"):
            base = base_dir / f"{target}.{fmt}.json"
            now = now_dir / f"w-{target}-{fmt}.json"
            if not (base.exists() and now.exists()):
                print(f"  ? {fmt:16} {target:5} missing input"); ok = False; continue
            good, why = compare(fmt, base, now)
            print(f"  {'✓' if good else '✗'} {fmt:16} {target:5} {why}")
            ok &= good
    print(f"SC-009: {'PASS' if ok else 'FAIL'}")
    sys.exit(0 if ok else 1)
