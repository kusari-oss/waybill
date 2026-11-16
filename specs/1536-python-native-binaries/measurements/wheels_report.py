"""Report for measure-wheels (waybill spec 1536, #1567).

Reads out/<wheel>.<ref>.<with|without>.cdx.json and prints, per scan:
components (recursive), nested, binary file-level components, content
identifications, linkage components, file-tier components, C230 and wall
time. Then SC-004 for the ref under test: every identification and linkage
component the no-.dist-info scan yields is present with the .dist-info, and
no binary path is reported by two components.

Classification is by PURL shape, and is this report's, not waybill's:
- binary file-level: pkg:generic with a file-sha256 qualifier;
- identification: pkg:cargo, pkg:golang, pkg:deb, pkg:rpm, pkg:apk, or
  pkg:generic with a version;
- linkage: pkg:generic, no version, no file-sha256, not file tier.
"""
import json
import os
import sys

out = sys.argv[1]
times = {}
if os.path.exists(f"{out}/times.txt"):
    for line in open(f"{out}/times.txt"):
        k, t = line.split()
        times[os.path.basename(k)] = float(t)


def walk(cs, depth=0):
    for c in cs or []:
        yield c, depth
        yield from walk(c.get("components"), depth + 1)


def prop(c, name):
    for p in c.get("properties", []) or []:
        if p.get("name") == name:
            return p.get("value")
    return None


def kind(c):
    purl = c.get("purl") or ""
    if prop(c, "waybill:component-tier") == "file":
        return "file-tier"
    if purl.startswith("pkg:generic/") and "file-sha256=" in purl:
        return "binary"
    if purl.split("/", 1)[0] in ("pkg:cargo", "pkg:golang", "pkg:deb", "pkg:rpm", "pkg:apk"):
        return "identified"
    if purl.startswith("pkg:generic/"):
        name = purl.split("?", 1)[0]
        return "identified" if "@" in name else "linkage"
    return "other"


def summarise(path):
    d = json.load(open(path))
    rows = list(walk(d.get("components")))
    meta = {p["name"]: p["value"] for p in d["metadata"].get("properties", [])}
    by_kind = {}
    for c, _ in rows:
        by_kind.setdefault(kind(c), []).append(c)
    occ = {}
    for c, _ in rows:
        if kind(c) in ("binary", "file-tier"):
            for o in (c.get("evidence") or {}).get("occurrences", []) or []:
                occ.setdefault(o.get("location"), []).append(c.get("bom-ref"))
    return {
        "components": len(rows),
        "nested": sum(1 for _, dep in rows if dep > 0),
        "binary": len(by_kind.get("binary", [])),
        "binary_nested": sum(1 for c, dep in rows if dep > 0 and kind(c) == "binary"),
        "identified": sorted({c["purl"] for c in by_kind.get("identified", [])}),
        "linkage": sorted({c["purl"] for c in by_kind.get("linkage", [])}),
        "file_tier": len(by_kind.get("file-tier", [])),
        "c230": meta.get("waybill:python-owned-binaries", "absent"),
        "paths_reported_twice": sorted(p for p, refs in occ.items() if len(refs) > 1),
    }


refs = dict(line.split() for line in open(f"{out}/refs.txt")) if os.path.exists(f"{out}/refs.txt") else {}
print("## Scans\n")
print("refs: " + ", ".join(f"{k} `{v[:12]}`" for k, v in refs.items()) + "\n")
wheels = sorted({f.split(".base.")[0] for f in os.listdir(out) if ".base." in f and f.endswith(".cdx.json")})
print("| wheel | ref | .dist-info | components | nested | binaries (nested) | identified | linkage | file tier | C230 | seconds |")
print("|---|---|---|---|---|---|---|---|---|---|---|")
summaries = {}
for w in wheels:
    for r in ("base", "new"):
        for v in ("with", "without"):
            f = f"{out}/{w}.{r}.{v}.cdx.json"
            if not os.path.exists(f):
                continue
            s = summarise(f)
            summaries[(w, r, v)] = s
            t = times.get(f"{w}.{r}.{v}", float("nan"))
            print(f"| `{w}` | {r} | {v} | {s['components']} | {s['nested']} | {s['binary']} ({s['binary_nested']}) | "
                  f"{len(s['identified'])} | {len(s['linkage'])} | {s['file_tier']} | `{s['c230']}` | {t:.1f} |")
print()

if any(r == "new" for _, r, _ in summaries):
    print("## SC-004 (ref under test)\n")
    for w in wheels:
        a, b = summaries.get((w, "new", "with")), summaries.get((w, "new", "without"))
        if not a or not b:
            continue
        miss_id = sorted(set(b["identified"]) - set(a["identified"]))
        miss_link = sorted(set(b["linkage"]) - set(a["linkage"]))
        ok = not miss_id and not miss_link and not a["paths_reported_twice"]
        print(f"### `{w}`: {'PASS' if ok else 'FAIL'}\n")
        print(f"- identifications without .dist-info: {len(b['identified'])}; missing with it: {len(miss_id)} {miss_id[:10]}")
        print(f"- linkage without .dist-info: {len(b['linkage'])}; missing with it: {len(miss_link)} {miss_link[:10]}")
        print(f"- paths reported by two components: {len(a['paths_reported_twice'])} {a['paths_reported_twice'][:10]}")
        print(f"- binaries nested: {a['binary_nested']} of {a['binary']}\n")
