"""T007: per-binary comparison for test binaries with lazy shared state. Usage: python3 -I -B lazy_state.py MEASUREMENTS_DIR"""
import os, sys, statistics as st
M = sys.argv[1]
sys.path.insert(0, M)
import count
direct = open(f"{M}/.direct").read().split()
users = open(f"{M}/.users").read().split()
def per(path, nextest):
    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    _, _, p = count.parse(lines)
    mx = {}
    if nextest:
        for l in lines:
            m = count.NX_TEST.match(l)
            if m:
                k = count.nextest_key(m.group(3)); mx[k] = max(mx.get(k, 0.0), float(m.group(2)))
    return p, mx
ct = [per(f"{M}/cargo-test-{n}.log", False)[0] for n in (1, 2, 3)]
nx = [per(f"{M}/nextest-{n}.log", True) for n in (1, 2, 3)]
def row(name):
    k = f"test:{name}"
    c = [r[k][1] for r in ct if k in r]; s = [r[0][k][1] for r in nx if k in r[0]]; x = [r[1][k] for r in nx if k in r[1]]
    if not c or not s:
        return f"| `{name}` | – | – | – | no tests run |", False
    cm, xm = st.median(c), st.median(x)
    bad = xm > cm + (max(c) - min(c)) + (max(x) - min(x))
    return f"| `{name}` | {min(c):.2f} / {cm:.2f} / {max(c):.2f} | {st.median(s):.2f} | {min(x):.2f} / {xm:.2f} / {max(x):.2f} | {'**yes**' if bad else 'no'} |", bad
out = ["# Lazy-state check (T007, FR-007)", "",
"Test files holding `OnceLock`, `LazyLock` or `lazy_static` under `waybill-cli/tests`: 12 (`lazy-files.txt`). The spec's 16 came from a grep that also matched non-`.rs` files. Under nextest each test is its own process, so lazy setup runs once per test instead of once per binary.", "",
"- **cargo-test binary wall**: the binary's `finished in`, min / median / max over 3 runs.",
"- **nextest summed**: the median sum of per-test durations. It is CPU-side cost and shows repeated setup; it is not in the same units as the binary wall, because `cargo test` runs one binary's tests on threads.",
"- **nextest slowest test**: what the binary contributes to wall time under nextest, min / median / max.",
"- **regresses**: the slowest nextest test's median exceeds the cargo-test median by more than both runners' spreads.", ""]
flagged = 0
for title, names in (("Files with their own lazy state", direct), ("Binaries including a shared lazy helper (`common/cdx_schema`, `sbom_edit_support`, `corpus_harness_195`)", users)):
    out += [f"## {title}", "", "| binary | cargo-test binary wall (s) | nextest summed (s) | nextest slowest test (s) | regresses |", "|---|---|---|---|---|"]
    for n in sorted(set(names)):
        r, b = row(n); out.append(r); flagged += b
    out.append("")
out.append(f"**Binaries that regress: {flagged}.**")
print("\n".join(out))
