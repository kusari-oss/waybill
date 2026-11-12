"""Run argv[2:], writing <prefix>.out/.err/.exit/.secs. Wall time via time.perf_counter in one process."""
import subprocess, sys, time
prefix, argv = sys.argv[1], sys.argv[2:]
with open(prefix + ".out", "wb") as o, open(prefix + ".err", "wb") as e:
    t0 = time.perf_counter()
    rc = subprocess.run(argv, stdout=o, stderr=e).returncode
    dt = time.perf_counter() - t0
open(prefix + ".exit", "w").write(f"{rc}\n")
open(prefix + ".secs", "w").write(f"{dt:.3f}\n")
