"""Cold vs warm bulk batches of 100 over real coordinates from an SBOM."""
import json, sys, time, urllib.parse
from probe_semantics import post, declared

def enc(s):
    return "".join(c if (c.isascii() and (c.isalnum() or c in "-._~@")) else "".join("%%%02X" % b for b in c.encode()) for c in s)

def coord(purl):
    p = purl[4:].split("?")[0].split("#")[0]
    typ, rest = p.split("/", 1)
    rest, _, ver = rest.rpartition("@")
    ver = urllib.parse.unquote(ver)
    parts = [urllib.parse.unquote(x) for x in rest.split("/")]
    name, ns = parts[-1], "/".join(parts[:-1])
    if not ver: return None
    m = {"npm": ("npm", "npmjs"), "cargo": ("crate", "cratesio"), "gem": ("gem", "rubygems"),
         "pypi": ("pypi", "pypi"), "maven": ("maven", "mavencentral"), "golang": ("go", "golang")}
    if typ not in m: return None
    t, prov = m[typ]
    if typ == "pypi": name = name.lower()
    if typ == "golang" and not ver.startswith("v"): ver = "v" + ver
    if typ == "maven" and not ns: return None
    return "/".join(enc(x) for x in (t, prov, ns or "-", name, ver))

if __name__ == "__main__":
    coords = []
    for f in sys.argv[1:]:
        for c in json.load(open(f)).get("components", []):
            k = coord(c.get("purl", "")) if c.get("purl") else None
            if k and k not in coords: coords.append(k)
    print("coords:", len(coords))
    batches = [coords[i:i+100] for i in range(0, len(coords), 100)]
    for rnd in ("pass1", "pass2"):
        tot = time.time(); n_decl = 0
        for i, b in enumerate(batches):
            s, r, dt = post(b)
            got = sum(1 for c in b if isinstance(r, dict) and declared(r.get(c)))
            n_decl += got
            print(f"{rnd} batch {i:2} n={len(b)} status={s} {dt:6.2f}s declared={got} missing_keys={sum(1 for c in b if not (isinstance(r, dict) and c in r))}", flush=True)
        print(f"{rnd} total {time.time()-tot:.1f}s declared={n_decl}/{len(coords)}", flush=True)
