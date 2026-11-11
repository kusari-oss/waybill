#!/usr/bin/env bash
# Q7 follow-up: is the test-framework over-inclusion darwin-specific (lib/links symlinks)?
# Query cache.nixos.org for the x86_64-linux and aarch64-darwin builds of the same libraries
# and of hlint, and compare direct references and closure sizes. Nothing is built.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q7-out
for sys in x86_64-linux aarch64-darwin; do
  for a in yaml unordered-containers case-insensitive hlint; do
    out=$(nix eval --raw "$R#legacyPackages.$sys.haskellPackages.$a.outPath")
    nix path-info --json --json-format 1 --store https://cache.nixos.org "$out" > "$o/xsys.$sys.$a.json" 2> "$o/xsys.$sys.$a.err"
    nix path-info -r --store https://cache.nixos.org "$out" > "$o/xsys.$sys.$a.closure" 2>> "$o/xsys.$sys.$a.err"
  done
done
python3 -I -B - "$o" > "$o/xsys.summary.txt" <<'PY'
import json, os, re, sys
o = sys.argv[1]
nm = lambda p: re.sub(r"^[a-z0-9]{32}-", "", os.path.basename(p))
TEST = re.compile(r"^(hspec|tasty|test-framework|QuickCheck|quickcheck|HUnit|ChasingBottoms|doctest|smallcheck|nanospec|mockery|silently)")
for a in ["yaml", "unordered-containers", "case-insensitive", "hlint"]:
    for sys_ in ["x86_64-linux", "aarch64-darwin"]:
        d = json.load(open(f"{o}/xsys.{sys_}.{a}.json"))
        (p, v), = d.items()
        refs = sorted(nm(r) for r in v["references"] if r != p)
        cl = [nm(l.strip()) for l in open(f"{o}/xsys.{sys_}.{a}.closure") if l.strip()]
        t = sorted(r for r in refs if TEST.match(r))
        tc = sorted(c for c in cl if TEST.match(c))
        print(f"{a:22} {sys_:15} direct refs={len(refs):3} test-framework direct refs={len(t)} {t}\n{'':38} closure={len(cl):3} test-framework paths in closure={len(tc)}")
PY
