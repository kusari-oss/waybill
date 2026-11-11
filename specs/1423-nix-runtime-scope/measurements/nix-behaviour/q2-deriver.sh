#!/usr/bin/env bash
# Q2/FR-003: is the `deriver` path-info reports the same .drv the evaluation names?
# Compare local-store deriver vs cache.nixos.org deriver for curl's runtime closure,
# and each against the drvPath nix eval gives for the evaluated attribute.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q2-out
CURL=$(nix eval --raw "$R#curl.outPath")
nix eval --raw "$R#curl.drvPath" > "$o/deriver-curl.evaldrv"
nix path-info --json --json-format 1 -r "$CURL" > "$o/deriver-curl.local.json"
nix path-info --json --json-format 1 -r --store https://cache.nixos.org "$CURL" > "$o/deriver-curl.cache.json"
SC=$(nix eval --raw "$R#shellcheck.outPath")
nix eval --raw "$R#shellcheck.drvPath" > "$o/deriver-shellcheck.evaldrv"
nix path-info --json --json-format 1 -r --store https://cache.nixos.org "$SC" > "$o/deriver-shellcheck.cache.json"
python3 -I -B - "$o" > "$o/deriver-compare.txt" <<'PY'
import json, sys, os
o = sys.argv[1]
for t in ["curl", "shellcheck"]:
    ev = open(f"{o}/deriver-{t}.evaldrv").read().strip()
    c = json.load(open(f"{o}/deriver-{t}.cache.json"))
    l = json.load(open(f"{o}/deriver-{t}.local.json")) if os.path.exists(f"{o}/deriver-{t}.local.json") else {}
    print(f"== {t}: eval drvPath {ev}")
    same = diff = 0
    for p in sorted(c):
        cd = c[p].get("deriver"); ld = l.get(p, {}).get("deriver") if l else None
        if l:
            if cd == ld: same += 1
            else: diff += 1
        print(f"  {os.path.basename(p)}\n    cache deriver: {cd}\n    local deriver: {ld}")
    root_cache = [c[p]['deriver'] for p in c if p.endswith(t+'-8.20.0-bin') or p.endswith('-0.11.0-bin')]
    print(f"  root output's cache deriver == eval drvPath: {root_cache and root_cache[0] == ev}")
    if l: print(f"  local vs cache deriver: same={same} differ={diff} of {len(c)}")
PY
