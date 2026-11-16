"""Summarise embedded-paths.py output as Markdown (waybill spec 1534). Usage: summarize.py <census.json>"""

import collections
import json
import sys

recs = [r for r in json.load(open(sys.argv[1])) if not r.get("missing")]
gb = lambda n: f"{n / 1e9:.3f} GB"
total = sum(r["size"] for r in recs)
print(f"## Embedded paths in {len(recs)} unattributed binaries ({gb(total)})\n")
for label, pred in [
    ("GNU build-id", lambda r: r["build_id"]),
    ("`.debug_info` present", lambda r: r["debug_info"]),
    ("DWARF absolute paths", lambda r: bool(r["dwarf_paths"])),
    ("absolute source paths in strings", lambda r: r["string_paths"] > 0),
    ("either kind of path", lambda r: bool(r["dwarf_paths"]) or r["string_paths"] > 0),
]:
    sel = [r for r in recs if pred(r)]
    print(f"- {label}: {len(sel)} files, {gb(sum(r['size'] for r in sel))}")
top = collections.defaultdict(lambda: [0, 0])
names = collections.defaultdict(lambda: [0, 0])
for r in recs:
    c = collections.Counter(r["string_classes"]) + collections.Counter(r["dwarf_paths"])
    key = c.most_common(1)[0][0] if c else "(no path evidence)"
    kind = key.split(":")[0] if c else key
    top[kind][0] += 1
    top[kind][1] += r["size"]
    if c:
        names[key][0] += 1
        names[key][1] += r["size"]
print("\n### Each file's most frequent path class, by kind\n")
for k, (n, b) in sorted(top.items(), key=lambda x: -x[1][1]):
    print(f"- `{k}`: {n} files, {gb(b)}")
print("\n### Top classes by bytes\n")
for k, (n, b) in sorted(names.items(), key=lambda x: -x[1][1])[:40]:
    print(f"- `{k}`: {n} files, {gb(b)}")
third = collections.Counter()
for r in recs:
    for k in set(r["string_classes"]) | set(r["dwarf_paths"]):
        if k.startswith(("vendored:", "fetchcontent:")):
            third[k] += 1
print("\n### Vendored and FetchContent trees named anywhere (files naming each)\n")
for k, n in third.most_common(40):
    print(f"- `{k}`: {n}")
com = collections.Counter(c for r in recs for c in r["comment"])
print("\n### `.comment` strings (files carrying each)\n")
for k, n in com.most_common(10):
    print(f"- `{k[:120]}`: {n}")
