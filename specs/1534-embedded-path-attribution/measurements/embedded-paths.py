"""Census of build and source paths embedded in installed binaries (waybill spec 1534, #1532).

Usage: python3 -I -B embedded-paths.py <extracted-root> <binary-list> <therock_manifest.json> <out.json>

For each listed file (root-relative), records what it carries that could name
the source it was built from: a GNU build-id, the .comment section, DWARF
compilation-unit names and directories (top level only), and absolute source
paths in its strings. Each path is classified: a TheRock submodule (longest
`submodule_path` prefix under the checkout), a CMake FetchContent `_deps/<name>-src`
tree, a vendored `third_party/<name>` / `external/<name>` / `3rdparty/<name>`
tree, the TheRock build tree, or other. Uses binutils only; reads nothing
outside the extracted root. A measuring script, not a model of a reader.
"""

import collections
import json
import os
import re
import subprocess
import sys

root, listfile, manifest_path, out_path = sys.argv[1:5]
manifest = json.load(open(manifest_path))
subs = sorted(((s["submodule_path"], s["submodule_name"]) for s in manifest["submodules"]), key=lambda x: -len(x[0]))

SRC = re.compile(r"(/[A-Za-z0-9._+@~-]+(?:/[A-Za-z0-9._+@~-]+)+\.(?:c|cc|cpp|cxx|c\+\+|h|hh|hpp|hxx|hip|cu|cuh|inc|def|cl|s|S))(?::\d+)?$")
VENDOR = re.compile(r"/(?:third_party|third-party|thirdparty|3rdparty|external|extern|vendor)/([A-Za-z0-9._+-]+)/")
FETCH = re.compile(r"/_deps/([A-Za-z0-9._+-]+?)-(?:src|build|subbuild)/")
THEROCK = re.compile(r"/TheRock/TheRock/(.*)$")


def run(args):
    try:
        return subprocess.run(args, capture_output=True, text=True, errors="replace", timeout=900).stdout
    except subprocess.TimeoutExpired:
        return ""


def classify(p):
    m = FETCH.search(p)
    if m:
        return f"fetchcontent:{m.group(1)}"
    m = VENDOR.search(p)
    if m:
        return f"vendored:{m.group(1)}"
    m = THEROCK.search(p)
    if m:
        rest = m.group(1)
        if rest.startswith("output/build/"):
            parts = rest.split("/")
            return "build:" + "/".join(parts[2:4])
        for sp, name in subs:
            if rest == sp or rest.startswith(sp + "/"):
                sub = rest[len(sp) + 1:].split("/")
                proj = "/".join(sub[:2]) if sub and sub[0] == "projects" else ""
                return f"submodule:{name}" + (f":{proj}" if proj else "")
        return "therock:" + rest.split("/")[0]
    return "other:" + "/".join(p.split("/")[1:4])


files = [l.strip() for l in open(listfile) if l.strip()]
records = []
for i, rel in enumerate(files):
    path = os.path.join(root, rel)
    if not os.path.isfile(path):
        records.append({"path": rel, "missing": True})
        continue
    sections = run(["readelf", "-S", "-W", path])
    notes = run(["readelf", "-n", "-W", path])
    comment = run(["readelf", "-p", ".comment", "-W", path])
    rec = {
        "path": rel,
        "size": os.path.getsize(path),
        "build_id": "Build ID:" in notes,
        "debug_info": ".debug_info" in sections,
        "comment": sorted({l.split("]", 1)[1].strip() for l in comment.splitlines() if "]" in l and l.strip().startswith("[")}),
    }
    dwarf = collections.Counter()
    cus = 0
    if rec["debug_info"]:
        for line in run(["readelf", "--debug-dump=info", "--dwarf-depth=1", "-W", path]).splitlines():
            if "DW_TAG_compile_unit" in line:
                cus += 1
            if "DW_AT_comp_dir" in line or ("DW_AT_name" in line and "/" in line):
                v = line.rsplit(": ", 1)[-1].strip()
                if v.startswith("/"):
                    dwarf[classify(v + "/")] += 1
    rec["compile_units"] = cus
    rec["dwarf_paths"] = dict(dwarf.most_common(8))
    strs = collections.Counter()
    nstr = 0
    for line in run(["strings", "-a", "-n", "8", path]).splitlines():
        m = SRC.search(line.strip())
        if m:
            nstr += 1
            strs[classify(m.group(1))] += 1
    rec["string_paths"] = nstr
    rec["string_classes"] = dict(strs.most_common(8))
    records.append(rec)
    if i % 50 == 0:
        print(f"{i}/{len(files)}", flush=True)
json.dump(records, open(out_path, "w"), indent=1, sort_keys=True)
