#!/usr/bin/env bash
# Q5 follow-up: does nix's own --offline stop the default-store query from asking
# substituters about a missing path (the "these N paths will be fetched" report)?
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q5-out
SC=$(nix eval --raw "$R#shellcheck.outPath")
python3 -I -B timed.py "$o/b-default-store-absent-offline" nix path-info -r --offline "$SC"
python3 -I -B timed.py "$o/b-default-store-absent-nosubst" nix path-info -r --option substituters '' "$SC"
