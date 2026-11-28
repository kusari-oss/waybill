#!/usr/bin/env bash
# Spec 1668 T002: over <golden-root>/<target>/cdx.json list C45 edgeless-lock-pin
# components and versionless hackage components; write <label>/corpus.tsv.
set -euo pipefail
root=$1; label=$2; mkdir -p "$label"
python3 -I -B - "$root" > "$label/corpus.tsv" <<'PY'
import json, sys, pathlib
print("target\tkind\tpurl")
for f in sorted(pathlib.Path(sys.argv[1]).glob("*/cdx.json")):
    d = json.loads(f.read_text()); cs = []
    def walk(x):
        for c in x: cs.append(c); walk(c.get("components", []))
    walk(d.get("components", []))
    for c in cs:
        p = c.get("purl", "")
        if any(q["name"] == "waybill:orphan-reason" and "edgeless-lock-pin" in q["value"] for q in c.get("properties", [])):
            print(f"{f.parent.name}\tedgeless\t{p}")
        if p.startswith("pkg:hackage/") and "@" not in p:
            print(f"{f.parent.name}\tversionless\t{p}")
PY
