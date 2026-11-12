"""#1455 (second rule): which runtime-closure references exist only because of
a darwin `lib/links` symlink, and which of those a Mach-O file actually loads.

usage: python3 -I -B analyze.py <pathinfo.json> <drvs.json> <root store path>...

For every runtime path O that has a `lib/links` directory, and every reference
O -> T, decides from the files in the local store:
  - other:   T's hash appears in O outside `lib/links` (file contents, or a
             symlink target outside `lib/links`). The reference stands.
  - links-only: only `lib/links` entries point into T. Whether the reference
             stands depends on what is loaded, below.
A links-only reference stands when one of its entry names is an `@rpath/`
install name some Mach-O file loads (LC_LOAD_DYLIB and friends, by
`otool -L`) in an output that ships. dyld resolves an `@rpath/` name against
the rpaths of every image in the load chain, so a name a library loads can be
satisfied by the executable's `lib/links`; the loaded names are therefore
pooled over every shipping output, to a fixpoint.
Then follows the references from the roots, with and without the dropped
ones, reports what the rule removes, and checks every dylib the dyld load graph
reaches (each Mach-O's @rpath names resolved through its own output's
`lib/links` first, then through the target's) is still kept.
"""
import json, os, re, subprocess, sys
from collections import defaultdict

B32 = set(b"0123456789abcdfghijklmnpqrsvwxyz")
pi = json.load(open(sys.argv[1]))
if isinstance(pi, dict) and "info" in pi:
    pi = pi["info"]
if isinstance(pi, list):
    pi = {x["path"]: x for x in pi}
drvs = json.load(open(sys.argv[2]))
drvs = drvs.get("derivations", drvs)
roots = [os.path.basename(p) for p in sys.argv[3:]]
base = os.path.basename
refs = {base(p): sorted({base(r) for r in v["references"]} - {base(p)}) for p, v in pi.items()}
outname, dname = {}, {}
for k, v in drvs.items():
    for n, o in (v.get("outputs") or {}).items():
        if o.get("path"):
            outname[base(o["path"])] = n
            dname[base(o["path"])] = v.get("name", k)
DOC = {"doc", "devdoc", "man", "info"}
S = "/nix/store/"
h = lambda p: p[:32]


def hashes_in(data):
    found, run = set(), 0
    for i, c in enumerate(data):
        if c in B32:
            run += 1
            if run >= 32:
                found.add(data[i - 31:i + 1].decode())
        else:
            run = 0
    return found


MAGICS = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce", b"\xca\xfe\xba\xbe"}


def macho_loads(path):
    try:
        with open(path, "rb") as fh:
            if fh.read(4) not in MAGICS:
                return None
        out = subprocess.run(["otool", "-L", path], capture_output=True, text=True).stdout
    except OSError:
        return None
    return [l.strip().split(" (")[0] for l in out.splitlines()[1:] if l.strip()]


classes = {}         # (O, T) -> other | links-only
entries = {}         # (O, T) -> lib/links entry names pointing into T
loads_of = defaultdict(set)     # O -> @rpath names its Mach-O files load
macho_count = 0
for o in sorted(refs):
    od = S + o
    links = os.path.join(od, "lib", "links")
    if not os.path.isdir(links):
        # No link farm: only the @rpath names its Mach-O files load matter.
        for dp, dns, fns in os.walk(od):
            for f in fns:
                fp = os.path.join(dp, f)
                if os.path.islink(fp) or not os.path.isfile(fp):
                    continue
                loads = macho_loads(fp)
                if loads is not None:
                    macho_count += 1
                    loads_of[o] |= {l[len("@rpath/"):] for l in loads if l.startswith("@rpath/")}
        continue
    via_links = defaultdict(list)   # T hash -> entry names
    for e in os.listdir(links):
        t = os.readlink(os.path.join(links, e))
        if t.startswith(S):
            via_links[h(t[len(S):])].append(e)
    other = set()
    for dp, dns, fns in os.walk(od):
        if os.path.realpath(dp) == os.path.realpath(links):
            dns[:] = []
            continue
        for f in fns + [d for d in dns if os.path.islink(os.path.join(dp, d))]:
            fp = os.path.join(dp, f)
            if os.path.islink(fp):
                other |= hashes_in(os.readlink(fp).encode())
                continue
            with open(fp, "rb") as fh:
                other |= hashes_in(fh.read())
            loads = macho_loads(fp)
            if loads is not None:
                macho_count += 1
                loads_of[o] |= {l[len("@rpath/"):] for l in loads if l.startswith("@rpath/")}
    for t in refs[o]:
        th = h(t)
        if th in other:
            classes[(o, t)] = "other"
        elif th in via_links:
            classes[(o, t)] = "links-only"
            entries[(o, t)] = set(via_links[th])
        else:
            classes[(o, t)] = "unexplained"

from collections import Counter
print("outputs with lib/links:", len({o for o, _ in classes}), " Mach-O files read:", macho_count)
print("reference classes:", dict(Counter(classes.values())))
for (o, t), c in sorted(classes.items()):
    if c == "unexplained":
        print("  unexplained", o, "->", t)


def reach(skip):
    seen, st = set(), [r for r in roots if outname.get(r) not in DOC]
    while st:
        p = st.pop()
        if p in seen or outname.get(p) in DOC:
            continue
        seen.add(p)
        st += [t for t in refs.get(p, []) if not skip(p, t)]
    return seen


before = reach(lambda o, t: False)
names = set()
while True:
    after = reach(lambda o, t: classes.get((o, t)) == "links-only" and not (entries[(o, t)] & names))
    grown = names | {n for o in after for n in loads_of.get(o, ())}
    if grown == names:
        break
    names = grown
print("@rpath names loaded by shipping outputs:", len(names))
lost = sorted(before - after)
print(f"\nshipping paths (doc rule): {len(before)}; with the lib/links rule: {len(after)}; dropped {len(lost)}")
for p in lost:
    print("  ", p[33:], f"[{outname.get(p, '?')}]")

# dyld check: every library a Mach-O in a shipping output loads through
# lib/links must still be shipping.
missing = sorted({t for (o, t), es in entries.items() if o in after and es & names and t not in after})
print("\nlib/links targets loaded by a shipping output but dropped:", missing or "none")

# WHY=<name>,<name>: each shipping referrer of the named packages, and how it
# refers to them.
for want in os.environ.get("WHY", "").split(","):
    for t in sorted(after) if want else []:
        if dname.get(t, "").rsplit("-", 1)[0] != want:
            continue
        for o in sorted(after):
            if t not in refs.get(o, []):
                continue
            c = classes.get((o, t), "referrer has no lib/links")
            if c == "links-only" and not (entries[(o, t)] & names):
                continue
            extra = f" loaded entries={sorted(entries[(o, t)] & names)}" if c == "links-only" else ""
            print(f"WHY {t[33:]} <- {o[33:]} [{c}]{extra}")
print("\nkept:", " ".join(sorted(p[33:] for p in after)))
