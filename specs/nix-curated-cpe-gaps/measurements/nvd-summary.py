#!/usr/bin/env python3
"""Reduce nvd-probe.sh output.

For each `<product>=<nixpkgs version>` argument, prints:
  - the title block in the format the cpe.rs title-query test reads
    (`=== <product> (<N> entries) ===`, one line per vendor with its first title);
  - per vendor: entry count, deprecated count, and whether the exact nixpkgs
    version is a dictionary entry, plus a sample of the version values, so the
    version encoding can be compared with nixpkgs'.

Usage: OUT_DIR=<dir> python3 -I nvd-summary.py unbound=1.25.0 libssh=0.12.0 ...
"""
import collections, json, os, sys

O = os.environ.get("OUT_DIR") or sys.exit("set OUT_DIR")
titles_out, detail_out = [], []
for arg in sys.argv[1:]:
    product, _, want = arg.partition("=")
    d = json.load(open(f"{O}/star-{product}.json"))
    ps = d["products"]
    if d["totalResults"] != len(ps):
        sys.exit(f"{product}: totalResults={d['totalResults']} but {len(ps)} returned; paginate")
    by_vendor = collections.OrderedDict()
    for p in ps:
        c = p["cpe"]
        f = c["cpeName"].split(":")
        if f[4] != product:
            continue
        v = by_vendor.setdefault(f[3], {"n": 0, "dep": 0, "title": None, "versions": [], "updates": set()})
        v["n"] += 1
        v["dep"] += 1 if c.get("deprecated") else 0
        if v["title"] is None:
            v["title"] = (c.get("titles") or [{}])[0].get("title", "")
        v["versions"].append(f[5])
        if f[6] not in ("*", "-"):
            v["updates"].add(f[6])
    titles_out.append(f"=== {product} ({d['totalResults']} entries) ===")
    for vendor, v in by_vendor.items():
        titles_out.append(f"    {vendor:28} {v['title'][:72]}")
    detail_out.append(f"=== {product}: totalResults={d['totalResults']} (nixpkgs version {want})")
    for vendor, v in by_vendor.items():
        vs = sorted(set(v["versions"]))
        exact = want in v["versions"]
        detail_out.append(
            f"  {vendor}:{product}  entries={v['n']} deprecated={v['dep']} "
            f"exact-version-{want}={'yes' if exact else 'no'}")
        detail_out.append(f"    title: {v['title']}")
        detail_out.append(f"    versions ({len(vs)} distinct), last 8 sorted: {vs[-8:]}")
        if v["updates"]:
            detail_out.append(f"    update-field values: {sorted(v['updates'])[:12]}")
print("\n".join(titles_out))
print()
print("\n".join(detail_out))
