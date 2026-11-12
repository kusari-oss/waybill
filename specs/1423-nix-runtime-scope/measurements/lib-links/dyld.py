"""#1455 (second rule): the store paths dyld would load, starting from every
Mach-O file in the given outputs, resolved the way dyld resolves them.

usage: python3 -I -B dyld.py <store path>...

Each image's install names come from `otool -L`, its rpaths from
`otool -l` (LC_RPATH). An `@rpath/` name is searched in the rpaths of the
image itself and then of every image that loaded it, in that order, as dyld
does; `@loader_path/` and `@executable_path/` are resolved against the loader.
System paths (/usr/lib, /System) are not followed. Prints the store paths
holding a loaded image, one per line, sorted, and any name that resolved
nowhere.
"""
import os, subprocess, sys

S = "/nix/store/"
MAGICS = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce", b"\xca\xfe\xba\xbe"}


def is_macho(p):
    try:
        with open(p, "rb") as fh:
            return fh.read(4) in MAGICS
    except OSError:
        return False


def loads(p):
    out = subprocess.run(["otool", "-L", p], capture_output=True, text=True).stdout.splitlines()[1:]
    names = [l.strip().split(" (")[0] for l in out if l.strip()]
    # `otool -L` on a dylib lists its own install name (LC_ID_DYLIB) first.
    idn = subprocess.run(["otool", "-D", p], capture_output=True, text=True).stdout.splitlines()[1:]
    own = {l.strip() for l in idn}
    return [n for n in names if n not in own]


def rpaths(p):
    out = subprocess.run(["otool", "-l", p], capture_output=True, text=True).stdout.splitlines()
    rs = []
    for i, l in enumerate(out):
        if l.strip() == "cmd LC_RPATH":
            for m in out[i + 1:i + 4]:
                m = m.strip()
                if m.startswith("path "):
                    rs.append(m[5:].rsplit(" (offset", 1)[0])
    return rs


def store_path(p):
    r = os.path.realpath(p)
    return r[len(S):].split("/", 1)[0] if r.startswith(S) else None


loaded, unresolved, seen = set(), set(), set()
stack = []
for root in sys.argv[1:]:
    for dp, _, fns in os.walk(root):
        for f in fns:
            fp = os.path.join(dp, f)
            if not os.path.islink(fp) and is_macho(fp):
                stack.append((fp, ()))
while stack:
    img, chain = stack.pop()
    real = os.path.realpath(img)
    if real in seen:
        continue
    seen.add(real)
    sp = store_path(real)
    if sp:
        loaded.add(sp)
    search = rpaths(real) + [r for c in chain for r in c]
    here = rpaths(real)
    for n in loads(real):
        if n.startswith(("/usr/lib/", "/System/")):
            continue
        cands = []
        if n.startswith("@rpath/"):
            rest = n[len("@rpath/"):]
            for r in search:
                r = r.replace("@loader_path", os.path.dirname(real))
                cands.append(os.path.join(r, rest))
        elif n.startswith("@loader_path/"):
            cands.append(os.path.join(os.path.dirname(real), n[len("@loader_path/"):]))
        else:
            cands.append(n)
        hit = next((c for c in cands if os.path.exists(c)), None)
        if hit is None:
            unresolved.add(n)
            continue
        stack.append((hit, (here,) + chain))
for p in sorted(loaded):
    print(p)
for n in sorted(unresolved):
    print("UNRESOLVED", n, file=sys.stderr)
