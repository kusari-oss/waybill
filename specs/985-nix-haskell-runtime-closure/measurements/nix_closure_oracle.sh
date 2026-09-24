#!/usr/bin/env bash
# Ask nix for the RUNTIME closure of a set of Haskell packages, by walking
# propagatedBuildInputs transitively. This is the denominator waybill's
# declared-only component set should be measured against (#962).
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
  roots = builtins.filter (p: p != null)
            (map (n: if builtins.hasAttr n hp then builtins.getAttr n hp else null) names);
  isHs = d: d != null && builtins.isAttrs d && builtins.hasAttr \"pname\" d;
  mk = p: { key = p.pname; val = p; };
  closure = builtins.genericClosure {
    startSet = map mk (builtins.filter isHs roots);
    operator = item:
      map mk (builtins.filter isHs (item.val.propagatedBuildInputs or []));
  };
in builtins.listToAttrs (map (i: {
     name = i.val.pname;
     value = i.val.version or \"NOVERSION\";
   }) closure)
"
