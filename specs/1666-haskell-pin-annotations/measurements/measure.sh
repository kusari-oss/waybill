#!/usr/bin/env bash
# Spec 1666 T001: C172/C221/orphan-reason census over corpus goldens.
# usage: measure.sh <golden-root> <out-dir>
set -euo pipefail
root=$1; out=$2; mkdir -p "$out"
for d in "$root"/*/; do
  t=$(basename "$d")
  case $t in hpack-*|haskell-*|nix-closure-*) ;; *) continue;; esac
  f=$d/cdx.json; [ -f "$f" ] || continue
  python3 -I -B - "$f" > "$out/$t.tsv" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
comps = []
def walk(cs):
    for c in cs or []:
        comps.append(c); walk(c.get("components"))
walk(doc.get("components"))
def props(o):
    return {p["name"]: p.get("value") for p in o.get("properties", []) or []}
print("# C172")
print("purl\tversion\tsource-type\temitted\tnixpkgs\tnote60\tC221")
for c in comps:
    p = props(c)
    v = p.get("waybill:nixpkgs-version-disagreement")
    if v is None: continue
    j = json.loads(v) if isinstance(v, str) else v
    print("\t".join(str(x) for x in [c.get("purl"), c.get("version"), p.get("waybill:source-type"),
        j.get("emitted"), j.get("nixpkgs"), (j.get("note") or "")[:60],
        "yes" if "waybill:requirement-unsatisfied" in p else "no"]))
# reachability from the metadata component
adj = {}
for d in doc.get("dependencies", []) or []:
    adj.setdefault(d["ref"], []).extend(d.get("dependsOn", []) or [])
root = (doc.get("metadata", {}).get("component") or {}).get("bom-ref")
seen, stack = set(), [root]
while stack:
    r = stack.pop()
    if r in seen: continue
    seen.add(r); stack.extend(adj.get(r, []))
print("# unreachable")
print("purl\torphan-reason")
for c in comps:
    if c.get("bom-ref") in seen or not c.get("purl"): continue
    print(f'{c.get("purl")}\t{props(c).get("waybill:orphan-reason", "<none>")}')
m = props(doc.get("metadata", {}))
r = m.get("waybill:nixpkgs-haskell-resolution")
if r:
    j = json.loads(r)
    print(f'# C174 disagreements\t{j.get("disagreements")}')
PY
done
