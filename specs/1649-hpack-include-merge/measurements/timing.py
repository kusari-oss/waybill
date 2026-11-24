"""Spec 1649 T020 (SC-007): wall-clock per scan, interleaved arms.

usage: python3 -I -B timing.py RUNS HOME_DIR OUT_DIR MAIN_BIN BRANCH_BIN COPY...
"""
import statistics
import subprocess
import sys
import time

runs = int(sys.argv[1]); home = sys.argv[2]; out = sys.argv[3]
arms = {"main": sys.argv[4], "branch": sys.argv[5]}
copies = sys.argv[6:]
t = {(c, a): [] for c in copies for a in arms}
for r in range(runs):
    for c in copies:
        for a, b in (arms.items() if r % 2 == 0 else reversed(list(arms.items()))):
            s = time.perf_counter()
            subprocess.run([b, "--offline", "sbom", "scan", "--path", c, "--no-deep-hash",
                            "--format", "cyclonedx-json", "--output", f"cyclonedx-json={out}/t.json"],
                           env={"HOME": home, "PATH": "/usr/bin:/bin"}, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            t[(c, a)].append(time.perf_counter() - s)
print("| Copy | main min / median / max (s) | branch min / median / max (s) | median ratio |")
print("|---|---|---|---|")
for c in copies:
    m, b = t[(c, "main")], t[(c, "branch")]
    f = lambda xs: f"{min(xs):.3f} / {statistics.median(xs):.3f} / {max(xs):.3f}"
    print(f"| {c.rsplit('/', 1)[-1]} | {f(m)} | {f(b)} | {statistics.median(b) / statistics.median(m):.3f} |")
