#!/usr/bin/env bash
# Spec 1667 T001: every C200 path in every corpus golden.
# usage: measure.sh <golden-root> <label>
set -euo pipefail
root=$1; out=$2; mkdir -p "$out"
python3 -I -B - "$root" "$out" <<'PY'
import json, sys, glob, os
root, out = sys.argv[1], sys.argv[2]
NAME = "waybill:hpack-read-outcome"
def c200(fmt, d):
    if fmt == "cdx":
        for p in d.get("metadata", {}).get("properties", []):
            if p["name"] == NAME: return json.loads(p["value"])
    else:
        anns = d.get("annotations", []) + [e for e in d.get("@graph", []) if e.get("type") == "Annotation"]
        for a in anns:
            t = a.get("comment") or a.get("statement") or ""
            if NAME in t:
                env = json.loads(t); v = env["value"]
                return json.loads(v) if isinstance(v, str) else v
    return None
def paths(v):
    for k, xs in sorted(v.items()):
        for e in xs:
            if isinstance(e, str): yield k, e
            else:
                for q in ("file", "manifest"):
                    if q in e: yield f"{k}.{q}", e[q]
rows, summary = [], []
for f in sorted(glob.glob(os.path.join(root, "*", "*.json"))):
    tgt, fmt = f.split(os.sep)[-2], os.path.basename(f)[:-5]
    text = open(f).read()
    v = c200("cdx" if fmt == "cdx" else fmt, json.loads(text))
    host = text.count("/home/runner")
    if v is None and not host: continue
    ps = list(paths(v or {}))
    absn = sum(1 for _, p in ps if p.startswith("/"))
    up = sum(1 for _, p in ps if p.startswith(".."))
    summary.append(f"{tgt}\t{fmt}\tpaths={len(ps)}\tabsolute={absn}\tabove-root={up}\thome-runner-occurrences={host}\tc244={'waybill:project-discovery-dropped' in text}")
    rows += [f"{tgt}\t{fmt}\t{k}\t{p}" for k, p in ps]
open(os.path.join(out, "c200.tsv"), "w").write("\n".join(rows) + "\n")
open(os.path.join(out, "summary.tsv"), "w").write("\n".join(summary) + "\n")
print(f"files={len(summary)}"); print("\n".join(summary))
PY
