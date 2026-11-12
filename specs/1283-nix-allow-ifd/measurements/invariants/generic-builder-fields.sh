#!/usr/bin/env bash
# Where nixpkgs' Haskell generic builder puts a package's dependencies, read
# from the derivation itself. Evaluation only: `nix derivation show` and
# `nix-instantiate` build nothing and fetch no outputs.
#
# Question (#1283): at a Haskell LIBRARY derivation built with doCheck, are
# any of the library's own link dependencies (cabal2nix's
# libraryHaskellDepends + executableHaskellDepends, from the getCabalDeps
# passthru) found only in `buildInputs`, or are they all in
# `propagatedBuildInputs`? And at an EXECUTABLE-only derivation
# (`justStaticExecutables`, isLibrary = false), where are they? Does the
# install phase's package registration (the builder's isLibrary branch) tell
# the two apart where propagation cannot -- a library whose dependencies are
# all boot packages, such as hashable, propagates nothing?
#
# Usage: generic-builder-fields.sh [system]   (default x86_64-linux)
set -euo pipefail
system="${1:-x86_64-linux}"
rev=a799d3e3886da994fa307f817a6bc705ae538eeb
pkgs="(import (builtins.fetchTarball \"https://github.com/NixOS/nixpkgs/archive/${rev}.tar.gz\") { system = \"${system}\"; })"

report() {
  local label="$1" expr="$2" deps_expr="$3"
  local drv deps
  drv=$(nix-instantiate --expr "$expr" 2>/dev/null)
  deps=$(nix-instantiate --eval --strict --json --expr "$deps_expr" 2>/dev/null)
  echo "== ${label} (${system}) ${drv##*/}"
  nix derivation show "$drv" 2>/dev/null | python3 -I -c '
import json, sys
doc = json.load(sys.stdin)
# nix >= 2.33 wraps the map as {"derivations": {...}, "version": 4}.
drvs = doc.get("derivations", doc)
env = next(iter(drvs.values()))["env"]
def names(k):
    return sorted({p.split("-", 1)[1].rsplit("-", 1)[0] for p in env.get(k, "").split()})
links = sorted(set(json.loads(sys.argv[1])))
prop, bi = names("propagatedBuildInputs"), names("buildInputs")
print("doCheck:", repr(env.get("doCheck", "")))
print("installPhase registers a package:", "register --gen-pkg-config" in env.get("installPhase", ""))
print(f"link deps (lib+exe, getCabalDeps) ({len(links)}):", " ".join(links))
print(f"propagatedBuildInputs ({len(prop)}):", " ".join(prop))
print(f"buildInputs ({len(bi)}):", " ".join(bi))
print("link deps in neither field:", " ".join(sorted(set(links) - set(prop) - set(bi))) or "-")
print("link deps only in buildInputs:", " ".join(sorted(set(links) & set(bi) - set(prop))) or "-")
print("buildInputs that are not link deps:", " ".join(sorted(set(bi) - set(links))) or "-")
' "$deps"
}

cabal_deps() {
  # Names of the lib+exe Haskell deps cabal2nix recorded; boot packages are null.
  echo "let d = ($1).getCabalDeps; in map (p: p.pname) (builtins.filter (p: p != null) (d.libraryHaskellDepends ++ d.executableHaskellDepends))"
}

for p in aeson warp servant-server hspec-core tasty-hunit hashable; do
  report "haskellPackages.${p}" "${pkgs}.haskellPackages.${p}" "$(cabal_deps "${pkgs}.haskellPackages.${p}")"
done
static="${pkgs}.haskell.lib.compose.justStaticExecutables ${pkgs}.haskellPackages.ShellCheck"
report "justStaticExecutables ShellCheck" "$static" "$(cabal_deps "(${static})")"
