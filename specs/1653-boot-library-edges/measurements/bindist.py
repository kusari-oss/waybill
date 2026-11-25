# Spec 1653 probe: boot-library dependencies from a GHC release's x86_64 Linux bindist.
# Hadrian bindists (9.4+) ship lib/package.conf.d/*.conf; make bindists (9.0, 9.2)
# ship libraries/<lib>/dist-install/package-data.mk with _DEP_NAMES, plus rts/package.conf.in.
import sys, tarfile, lzma, urllib.request, json, os, re
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from conf_deps import parse, field
v = sys.argv[1]; url = f"https://downloads.haskell.org/~ghc/{v}/ghc-{v}-x86_64-deb10-linux.tar.xz"
conf, mk, rts_in = {}, {}, None
with urllib.request.urlopen(url) as r, lzma.open(r) as x, tarfile.open(fileobj=x, mode="r|") as t:
    for m in t:
        if not m.isfile(): continue
        n = m.name
        if "/package.conf.d/" in n and n.endswith(".conf"):
            name, ver, deps = parse(t.extractfile(m).read().decode())
            conf[name] = {"version": ver, "depends": deps}
        elif (n.endswith("/dist-install/package-data.mk") and "/libraries/" in n) or n.endswith("/compiler/stage2/package-data.mk"):
            text = t.extractfile(m).read().decode()
            g = lambda k: (re.search(r"_" + k + r" = (.*)", text) or [None, ""])[1].split()
            lib = n.split("/libraries/")[1].split("/dist-install/")[0].split("/")[-1] if "/libraries/" in n else "ghc"
            pkg = (re.search(r"_PACKAGE = (\S+)", text) or [None, lib])[1]
            mk[pkg] = {"version": (g("VERSION") or [""])[0], "depends": sorted(set(g("DEP_NAMES")))}
        elif n.endswith("/rts/package.conf.in"):
            rts_in = t.extractfile(m).read().decode()
if conf:
    out, layout = conf, "package.conf.d"
else:
    out, layout = mk, "package-data.mk"
    if rts_in is not None and "rts" not in out:
        deps = [d for d in field(rts_in, "depends") if re.fullmatch(r"[a-z][a-z0-9-]*", d)]
        out["rts"] = {"version": "", "depends": sorted(set(deps)), "note": "rts/package.conf.in (cpp template)"}
json.dump({"release": v, "layout": layout, "libraries": out}, sys.stdout, indent=1, sort_keys=True)
