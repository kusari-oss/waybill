#!/usr/bin/env bash
# Runtime closures of Haskell executables at nixpkgs a799d3e3, from cache.nixos.org.
# Question: does `nix path-info -r` include the Haskell libraries statically linked
# into the executable? Run 2026-11-10, aarch64-darwin.
set -euo pipefail
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
for a in shellcheck haskellPackages.hlint pandoc; do
  out=$(nix eval --raw "$R#$a.outPath")
  nix path-info -r --store https://cache.nixos.org "$out" > "$a.rt"
  echo "$a $(wc -l < "$a.rt") paths"
done
