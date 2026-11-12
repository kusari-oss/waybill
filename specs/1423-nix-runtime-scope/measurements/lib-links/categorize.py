"""#1455 lib/links rule — the members the rule newly marks, by category.

usage: python3 -I -B categorize.py <runtime listing> <root store path> <kept paths file> <added marks file> <cdx>

Categories, from the compiled packages' `package.conf.d` entries:
  - test framework: hspec*, tasty*, QuickCheck, quickcheck-*, test-framework*,
    HUnit, doctest, ChasingBottoms, smallcheck (the moat-scan.sh list);
  - moat library closure: reachable from moat's unit through `depends`;
  - shipping library closure: reachable through `depends` from any kept
    output's unit (a library a shipped Haskell library links);
  - no package.conf.d entry in the runtime closure: a C library, or a
    package that is not in the runtime closure at all;
  - other Haskell: none of the above; reached only through link farms, and
    in no shipping library's `depends` closure.
"""
import glob, os, re, sys

paths = [l.strip() for l in open(sys.argv[1]) if l.strip()]
root = sys.argv[2]
kept = {l.strip() for l in open(sys.argv[3]) if l.strip()}
added = [l.rstrip("\n") for l in open(sys.argv[4]) if l.strip()]
TF = re.compile(r"^(hspec.*|tasty.*|QuickCheck|quickcheck-.*|test-framework.*|HUnit|doctest|ChasingBottoms|smallcheck)$")


def fields(text):
    out, key = {}, None
    for line in text.splitlines():
        if not line.strip():
            continue
        if line[0] in " \t" and key:
            out[key] += " " + line.strip()
        elif ":" in line:
            key, _, v = line.partition(":")
            key = key.strip()
            out[key] = v.strip()
    return out


units, by_path = {}, {}
for p in paths:
    for conf in glob.glob(os.path.join(p, "lib", "ghc-*", "lib", "package.conf.d", "*.conf")):
        f = fields(open(conf, errors="replace").read())
        if "id" in f:
            units[f["id"]] = (f.get("name"), f.get("version"), p, f.get("depends", "").split())
            by_path.setdefault(p, []).append(f["id"])


def closure(starts):
    seen, st = set(), list(starts)
    while st:
        i = st.pop()
        if i in seen:
            continue
        seen.add(i)
        st += units.get(i, (0, 0, 0, []))[3]
    return {(units[i][0], units[i][1]) for i in seen if i in units}


moat = closure(by_path.get(root, []))
shipping = closure([i for p in kept for i in by_path.get(p, [])])
haskell = {(u[0], u[1]) for u in units.values()}
cats = {}
for a in added:
    name, _, ver = a.rpartition(" ")
    nv = (name, ver)
    if TF.match(name):
        c = "test framework"
    elif nv in moat:
        c = "moat library closure"
    elif nv in shipping:
        c = "shipping library closure"
    elif nv not in haskell:
        c = "no package.conf.d entry in the runtime closure (a C library, or a package outside the runtime closure)"
    else:
        c = "other Haskell: reached only through link farms, in no shipping library's depends closure"
    cats.setdefault(c, []).append(a)
print(f"newly marked: {len(added)}")
for c in sorted(cats):
    print(f"\n{c}: {len(cats[c])}")
    for a in sorted(cats[c]):
        print("  ", a)
print(f"\nmoat library closure: {len(moat)} packages; shipping library closure: {len(shipping)} packages")
print("moat library closure members newly marked:", sorted(f"{n} {v}" for n, v in moat if f"{n} {v}" in added) or "none")
