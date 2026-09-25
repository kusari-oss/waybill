#!/usr/bin/env bash
# Ground-truth probe: ask nix itself what version a pinned nixpkgs carries
# for each Haskell package name, using the project's own compiler set.
#
#   nixprobe.sh <nixpkgs-rev> <ghc-attr> <names-file> > out.json
#
# Values are tagged so the three cases stay distinguishable:
#   ABSENT   - the attribute does not exist in the package set
#   BOOT     - the attribute exists but is null (compiler-supplied)
#   <semver> - the version the package set carries
set -euo pipefail
REV="$1"; GHC="$2"; NAMES_FILE="$3"
NAMES=$(awk 'NF' "$NAMES_FILE" | sed 's/.*/"&"/' | tr '\n' ' ')
nix eval --impure --json --expr "
let
  pkgs = import (builtins.fetchTarball {
    url = \"https://github.com/NixOS/nixpkgs/archive/${REV}.tar.gz\";
  }) { config.allowBroken = true; config.allowUnfree = true; };
  hp = pkgs.haskell.packages.${GHC};
  names = [ ${NAMES} ];
in builtins.listToAttrs (map (n: {
     name = n;
     value =
       if !(builtins.hasAttr n hp) then \"ABSENT\"
       else let v = builtins.getAttr n hp; in
         if v == null then \"BOOT\"
         else if builtins.hasAttr \"version\" v then v.version
         else \"NOVERSION\";
   }) names)
"
