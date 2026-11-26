"""Spec 1660 FR-001, second variant: start N processes from N threads released
by one barrier, closer to nextest's parallel launch than bash's serial `&`.
Usage: python3 -I -B collide_threads.py BIN N BATCHES [--fixed-seed]"""
import subprocess, sys, threading
binpath, n, batches = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
extra = sys.argv[4:]
names = dups = 0
for _ in range(batches):
    out = [None] * n
    gate = threading.Barrier(n)
    def go(i):
        gate.wait()
        out[i] = subprocess.run([binpath, *extra], capture_output=True, text=True).stdout.strip()
    ts = [threading.Thread(target=go, args=(i,)) for i in range(n)]
    for t in ts: t.start()
    for t in ts: t.join()
    names += len(out)
    dups += len(out) - len(set(out))
print(f"{'fixed-seed' if extra else 'real'}\t{n}\t{batches}\t{names}\t{dups}")
