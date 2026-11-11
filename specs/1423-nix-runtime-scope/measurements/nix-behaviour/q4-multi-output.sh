#!/usr/bin/env bash
# Q4: multi-output package (curl: bin, dev, out, man, devdoc).
# Which output does the evaluated attribute's outPath name, and does path-info on one
# output return only that output's closure?
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q4-out; mkdir -p "$o"
{ echo "curl.outputName = $(nix eval --raw "$R#curl.outputName")"
  echo "curl.outputs = $(nix eval --json "$R#curl.outputs")"
  echo "curl.outPath = $(nix eval --raw "$R#curl.outPath")"
  echo "curl.meta.outputsToInstall = $(nix eval --json "$R#curl.meta.outputsToInstall")"
  echo "curl.drvPath = $(nix eval --raw "$R#curl.drvPath")"
} > "$o/attrs.txt"
for out in bin dev out man devdoc; do
  p=$(nix eval --raw "$R#curl.$out.outPath"); echo "$out $p" >> "$o/output-paths.txt"
  nix path-info -r --store https://cache.nixos.org "$p" > "$o/closure.$out.paths" 2> "$o/closure.$out.err"; echo $? > "$o/closure.$out.exit"
  nix path-info -r "$p" > "$o/local.$out.paths" 2> "$o/local.$out.err"; echo $? > "$o/local.$out.exit"
done
# The evaluated .drv's own idea of its outputs (path per output name)
nix derivation show "$(nix eval --raw "$R#curl.drvPath")" > "$o/curl.drv.json"
# Multiple paths in one query: closure of the union
nix path-info -r --store https://cache.nixos.org $(awk '{print $2}' "$o/output-paths.txt") > "$o/closure.all-outputs.paths" 2>&1
python3 -I -B - "$o" > q4-summary.txt <<'PY'
import sys, json, os
o = sys.argv[1]
print(open(f"{o}/attrs.txt").read())
outs = dict(l.split() for l in open(f"{o}/output-paths.txt"))
d = json.load(open(f"{o}/curl.drv.json"))
(drv, body), = d["derivations"].items() if "derivations" in d else d.items()
print("drv outputs (nix derivation show):", {k: v.get("path") for k, v in body["outputs"].items()})
cl = {k: set(l.strip() for l in open(f"{o}/closure.{k}.paths") if l.strip()) for k in outs}
for k, p in outs.items():
    others = [j for j, q in outs.items() if j != k and (q in cl[k] or os.path.basename(q) in {os.path.basename(x) for x in cl[k]})]
    print(f"{k:7} exit={open(f'{o}/closure.{k}.exit').read().strip()} local_exit={open(f'{o}/local.{k}.exit').read().strip()} closure={len(cl[k]):3}  other curl outputs in its closure: {others}")
union = set(l.strip() for l in open(f"{o}/closure.all-outputs.paths") if l.strip().startswith("/nix/store"))
print("union of all outputs:", len(union))
PY
