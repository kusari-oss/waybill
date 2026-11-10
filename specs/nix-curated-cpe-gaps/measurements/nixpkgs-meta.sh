#!/usr/bin/env bash
# What nixpkgs (at the wrapper flakes' pin) says about each candidate:
# name, pname, version, src URL, homepage, and meta.identifiers (cpe, cpeParts).
# Usage: nixpkgs-meta.sh <attr>...
set -uo pipefail
PIN=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
for a in "$@"; do
  nix eval --json "$PIN#$a" --apply 'p: {
    name = p.name; pname = p.pname or null; version = p.version or null;
    src = (p.src.urls or [ (p.src.url or null) ]);
    homepage = p.meta.homepage or null;
    description = p.meta.description or null;
    cpe = p.meta.identifiers.cpe or null;
    cpeParts = p.meta.identifiers.cpeParts or null;
  }' 2>&1 | sed "s|^|$a\t|"
done
