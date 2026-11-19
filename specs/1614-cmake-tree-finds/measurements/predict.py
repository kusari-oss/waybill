"""Spec 1614: predict SC-001's count from the census and a TheRock document.

Usage: python3 -I -B predict.py <includes.json> <therock.cdx.json>
  includes.json    census-includes artifact (fixture repo census/includes.py)
  therock.cdx.json measure-source's branch.sub.cdx.json for the base commit

Applies the spec's rules in order (FR-006 tools, FR-005 pairing with a fetch,
submodule or installed-library record, FR-007 built in tree) to each
find_package name; whatever remains is a new component. A prediction, not a
waybill run: the reader decides, and the implementation measures it.
"""
import collections
import json
import re
import sys

cen = json.load(open(sys.argv[1]))
doc = json.load(open(sys.argv[2]))


def walk(cs):
    for c in cs or []:
        yield c
        yield from walk(c.get("components"))


def prop(c, n):
    return next((p["value"] for p in c.get("properties", []) if p["name"] == n), None)


sub_names, inst_names = set(), set()
for c in walk(doc["components"]):
    m = prop(c, "waybill:source-mechanism") or ""
    p = c.get("purl") or ""
    if m == "git-submodule":
        for f in json.loads(prop(c, "waybill:source-files") or "[]"):
            sub_names.add(f.rstrip("/").split("/")[-1].lower())
        sub_names.add(re.sub(r"@.*", "", p).split("/")[-1].lower())
    if m in ("cmake-package-version", "pkg-config"):
        inst_names.add(re.sub(r"@.*", "", p).split("/")[-1].lower())
rows = cen["E_per_name"]["rows"]
fetch_names = {r["name"] for r in rows if r["kind"] == "fetch"}
TOOLS = {"threads", "pkgconfig", "git", "doxygen"}


def variants(n):
    return {n, n.replace("_", "-"), n.replace("-", "_")}


out, keep = collections.Counter(), []
for r in (r for r in rows if r["kind"] == "find"):
    n = r["name"]
    if n in TOOLS:
        out["tool (FR-006)"] += 1
    elif n in ("python", "python3"):
        out["python, decided per call (FR-006)"] += 1
    elif variants(n) & fetch_names:
        out["fetch pair (FR-005)"] += 1
    elif variants(n) & sub_names:
        out["submodule (FR-005)"] += 1
    elif variants(n) & inst_names:
        out["installed record (FR-005)"] += 1
    elif r["class"] in ("first-party-project", "first-party-config"):
        out["built in tree (FR-007)"] += 1
    else:
        out["new component"] += 1
        keep.append(r)
for k, v in out.most_common():
    print(f"{v:4d}  {k}")
print("new components declared only under conditions:", sum(r["only_conditional_effective"] for r in keep))
print("new components by class:", dict(collections.Counter(r["class"] for r in keep)))
