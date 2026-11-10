# Transitive Haskell *library* dependency closure of an executable package, from the
# generic builder's `getCabalDeps` passthru (cabal2nix's flattened build-depends).
# Start: the package's libraryHaskellDepends ++ executableHaskellDepends (test/bench excluded).
# Step : each library's libraryHaskellDepends (its exe/test/bench deps are not linked into a consumer).
# Boot packages shipped with GHC appear as `null` in haskellPackages and are dropped here.
# Usage: nix eval --json --impure --expr 'import ./cabal-closure.nix { rev = "..."; attr = [ "haskellPackages" "ShellCheck" ]; }'
{ rev, attr, system ? "aarch64-darwin" }:
let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/${rev}").legacyPackages.${system};
  pkg = pkgs.lib.attrByPath attr null pkgs;
  nn = builtins.filter (x: x != null);
  cd = p: p.getCabalDeps or {};
  libDeps = p: nn ((cd p).libraryHaskellDepends or [] ++ (cd p).buildDepends or []);
  start = nn ((cd pkg).libraryHaskellDepends or [] ++ (cd pkg).executableHaskellDepends or [] ++ (cd pkg).buildDepends or []);
  testOnly = let s = map (p: p.outPath) start; in
    builtins.filter (p: !(builtins.elem p.outPath s)) (nn ((cd pkg).testHaskellDepends or []));
  node = p: { key = p.outPath; item = p; };
  closure = builtins.genericClosure { startSet = map node start; operator = e: map node (libDeps e.item); };
  desc = p: { name = p.pname or p.name; version = p.version or null; out = p.outPath; drv = p.drvPath;  # not "outPath": nix serialises any attrset with outPath as that string
              hasCabalDeps = p ? getCabalDeps; };
in {
  root = desc pkg;
  direct = map desc start;
  testOnlyDirect = map desc testOnly;
  closure = map (e: desc e.item) closure;
}
