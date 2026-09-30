#!/usr/bin/env bash
# T-R2 — does closure composition hold outside Haskell?
#
# The role split and the patch density in the spec were measured on two
# Haskell projects. If they are a property of the Haskell toolchain rather
# than of nix, the figures in the spec do not generalise and the
# documentation should say so. This probe answers that by taking closures
# for packages from several language ecosystems and reporting the same
# numbers for each.
#
# Usage:  closure-composition.sh [flakeref ...]
# With no arguments it uses nixpkgs packages standing in for each ecosystem,
# which is enough to answer the composition question: the closure of a
# nixpkgs-built Rust program has the same shape as the closure of a Rust
# project's own flake, since both are `buildRustPackage` derivations.
set -uo pipefail

REFS=("$@")
if [ ${#REFS[@]} -eq 0 ]; then
  REFS=(
    "nixpkgs#ripgrep"    # Rust
    "nixpkgs#hello"      # C / autotools
    "nixpkgs#jq"         # C, and known to carry CVE-named patches
    "nixpkgs#gopls"      # Go
    "nixpkgs#python3Packages.requests"  # Python
  )
fi

printf '%-34s %8s %8s %8s %8s %8s %8s %8s\n' \
  flakeref drvs artifact tooling both unref patches noCVE

for ref in "${REFS[@]}"; do
  json=$(nix derivation show -r --option allow-import-from-derivation false "$ref" 2>/dev/null)
  if [ -z "$json" ]; then
    printf '%-34s %8s\n' "$ref" "FAILED"
    continue
  fi
  echo "$json" | python3 -c '
import json,sys
ref=sys.argv[1]
doc=json.load(sys.stdin)
# `nix derivation show` emits a flat map on older versions and
# {"derivations": {...}, "version": 3} on newer ones. The Rust parser
# accepts both; so must this, or the probe silently measures nothing.
drvs=doc.get("derivations", doc) if isinstance(doc, dict) else doc
# output-path basename -> drv key, mirroring the Rust output_index
idx={}
for k,v in drvs.items():
    for o in (v.get("outputs") or {}).values():
        if o.get("path"): idx[o["path"].rsplit("/",1)[-1]]=k
art,tool=set(),set()
for k,v in drvs.items():
    env=v.get("env",{}) or {}
    for field,dest in (("buildInputs",art),("propagatedBuildInputs",art),
                       ("nativeBuildInputs",tool)):
        for p in (env.get(field,"") or "").split():
            t=idx.get(p.rsplit("/",1)[-1])
            if t: dest.add(t)
both=art&tool
patches=nocve=0
import re
CVE=re.compile(r"CVE-\d{4}-\d+")
for k,v in drvs.items():
    for p in ((v.get("env",{}) or {}).get("patches","") or "").split():
        patches+=1
        name=idx.get(p.rsplit("/",1)[-1])
        label=(drvs.get(name,{}).get("env",{}) or {}).get("pname") if name else None
        label=label or (drvs.get(name,{}) or {}).get("name") or p.rsplit("/",1)[-1]
        if not CVE.search(label): nocve+=1
print("%-34s %8d %8d %8d %8d %8d %8d %8d" % (
    ref,len(drvs),len(art-both),len(tool-both),len(both),
    len(drvs)-len(art|tool),patches,nocve))
' "$ref"
done
