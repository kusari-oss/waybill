# usage: cargo test -p waybill -- --list 2>&1 | python3 -I -B list_counts.py
# Per test binary: total tests, corpus_harness_195:: tests, dual_format_perf:: tests.
import sys, re, collections
cur = None; rows = collections.OrderedDict()
for line in sys.stdin:
    m = re.match(r'\s+Running (\S+) \((.*)\)', line)
    if m:
        cur = m.group(1); rows[cur] = [0, 0, 0]; continue
    if cur and line.rstrip().endswith(': test'):
        rows[cur][0] += 1
        name = line.split(':')[0]
        if 'corpus_harness_195::' in line: rows[cur][1] += 1
        if 'dual_format_perf::' in line: rows[cur][2] += 1
tot = [sum(r[i] for r in rows.values()) for i in range(3)]
print(f"binaries={len(rows)} tests={tot[0]} corpus_harness_195={tot[1]} dual_format_perf={tot[2]}")
print(f"binaries with corpus_harness_195 tests: {sum(1 for r in rows.values() if r[1])}")
print(f"binaries with dual_format_perf tests: {sum(1 for r in rows.values() if r[2])}")
for k, r in rows.items():
    if r[1] or r[2]: print(f"  {k}\ttotal={r[0]}\tharness={r[1]}\tdual={r[2]}")
