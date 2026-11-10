#!/usr/bin/env bash
# What derivation names do nixpkgs' Python interpreters carry at the pinned rev?
set -uo pipefail
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
for a in python3 python313 python312 python314 python3Minimal python27 pypy3 python313FreeThreading; do
  n=$(nix eval --raw "$R#$a.name" 2>/dev/null || echo ERR)
  p=$(nix eval --raw "$R#$a.pname" 2>/dev/null || echo ERR)
  c=$(nix eval --json "$R#$a.meta.identifiers.cpeParts" 2>/dev/null || echo ERR)
  echo "$a	name=$n	pname=$p	cpeParts=$c"
done
