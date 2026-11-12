"""#1455 lib/links rule — moat's Haskell library closure, from the compiled
packages' own `package.conf.d` entries (what GHC links against), and which of
those the rule sets aside.

usage: python3 -I -B libclosure.py <runtime listing (nix path-info -r)> <root store path> <analysis.txt>

Reads every `lib/ghc-*/lib/package.conf.d/*.conf` in the runtime closure's
outputs (GHC's own package database included). Each entry's `id`, `name`,
`version` and `depends` (unit ids). Follows `depends` from the root's entry
transitively, and reports each unit in the closure: whether its store path
is set aside by the rule (from analyze.py's dropped list), and how the
root's `out` refers to it.
"""
import glob, os, re, sys

paths = [l.strip() for l in open(sys.argv[1]) if l.strip()]
root = sys.argv[2]
dropped_lines = open(sys.argv[3]).read().split("\nshipping paths")[1].split("\n\n")[0].splitlines()[1:]
dropped = {l.strip().split(" [")[0] for l in dropped_lines if l.startswith("   ")}


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


units = {}   # id -> (name, version, store path, depends)
for p in paths:
    for conf in glob.glob(os.path.join(p, "lib", "ghc-*", "lib", "package.conf.d", "*.conf")):
        f = fields(open(conf, errors="replace").read())
        if "id" not in f:
            continue
        units[f["id"]] = (f.get("name"), f.get("version"), p, f.get("depends", "").split())

root_ids = [i for i, u in units.items() if u[2] == root]
seen, stack = set(), list(root_ids)
while stack:
    i = stack.pop()
    if i in seen:
        continue
    seen.add(i)
    stack += units.get(i, (None, None, None, []))[3]
missing = sorted(i for i in seen if i not in units)
print(f"root units: {root_ids}")
print(f"library closure: {len(seen)} units ({len(missing)} with no conf in the closure: {missing})")
rows = []
for i in sorted(seen - set(root_ids)):
    if i not in units:
        continue
    name, ver, sp, _ = units[i]
    base = os.path.basename(sp)
    rows.append((name, ver, base[33:], "SET ASIDE" if base[33:] in dropped else "kept"))
for r in sorted(rows):
    print("  %-28s %-10s %-40s %s" % r)
print(f"\nset aside among them: {sum(1 for r in rows if r[3] == 'SET ASIDE')}")
