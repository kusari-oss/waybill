#!/usr/bin/env python3
"""SC-001b: the recorded sets must differ only by removals attributable
to a failed open request.

Random per-run names are masked first. Without that the diff is dominated
by rustc's and cargo's temp directories, whose names change every run:
an honest comparison showed 52 "unexplained" removals that were simply
`rmetaTmtspV` in one run and `rmeta6OxaCo` in the next.
"""
import re, sys

MASK = [
    (re.compile(r'/rmeta[A-Za-z0-9]{6}(/|$)'), r'/rmetaXXXXXX\1'),
    (re.compile(r'/rustc[A-Za-z0-9]{6}(/|$)'), r'/rustcXXXXXX\1'),
    (re.compile(r'/\.tmp[A-Za-z0-9]{6}\.temp-archive(/|$)'), r'/.tmpXXXXXX.temp-archive\1'),
    (re.compile(r'/target[A-Za-z0-9]{6}(/|$)'), r'/targetXXXXXX\1'),
]

def norm(p):
    for rx, rep in MASK:
        p = rx.sub(rep, p)
    return p

def load(f):
    return {norm(l.strip()) for l in open(f) if l.strip()}

before_r, after_r = load('read-all.txt'), load('after-read.txt')
before_w, after_w = load('write-all.txt'), load('after-write.txt')
disc = load('discarded.txt')
short = lambda p: p.replace(
    '/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge', '…')

ok = True
for name, before, after in (('READ', before_r, after_r), ('WRITE', before_w, after_w)):
    removed, added = before - after, after - before
    unexplained = removed - disc
    print(f"--- {name} (masked) ---")
    print(f"  before={len(before)} after={len(after)} "
          f"removed={len(removed)} added={len(added)}")
    print(f"  attributable to a failed request : {len(removed & disc)}")
    print(f"  UNEXPLAINED removals             : {len(unexplained)}")
    for p in sorted(unexplained):
        print("     ? ", short(p))
    for p in sorted(added):
        print("     + ", short(p))
    if unexplained or added:
        ok = False

print()
print("SC-001b:", "PASS — every removal is attributable, nothing was added"
      if ok else "FAIL — see the entries above")
sys.exit(0 if ok else 1)
