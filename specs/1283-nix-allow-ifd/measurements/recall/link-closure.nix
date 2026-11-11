# The Haskell packages of one flake output, per the cabal metadata nixpkgs'
# Haskell builder keeps on every package it builds (`getCabalDeps`).
#
#   nix eval --json --option allow-import-from-derivation true \
#     --file link-closure.nix \
#     --argstr flake 'github:<owner>/<repo>/<full sha>' \
#     --argstr system x86_64-linux --argstr attr default \
#     result
#
# The trailing `result` attribute path is what makes nix apply the --argstr
# values; with no attribute path it prints the function instead.
#
# Two sets, for two different questions:
#
#   packages  the LINK closure: library and executable Haskell dependencies,
#             transitively. What the shipped code contains, so this is what
#             osv-scanner reads for HSEC truth.
#   universe  every Haskell package reachable through ANY *HaskellDepends or
#             *ToolDepends field -- test, benchmark, setup and build tools
#             included -- keeping only members that are themselves Haskell
#             builds (they carry getCabalDeps). Only used to answer "is this
#             grype match a Haskell package?". A test-only dependency such as
#             hedgehog can sit in a runtime store closure, and if it were
#             missing here its unrelated-product CVEs would be counted as
#             real ones.
#
# Independent of waybill: it asks nix, not the SBOM under test.
#
# Limits, stated rather than hidden: GHC boot libraries are `null` in
# getCabalDeps, so they are absent from both sets. `root.has-cabal-deps =
# false` means the output is a wrapper without the metadata and both sets are
# empty -- the caller must treat that as a failure, not as "no Haskell
# packages" (classify.py does).
{ flake, system, attr ? "default" }:
let
  lib = builtins;
  root = (lib.getFlake flake).packages.${system}.${attr};
  usable = d: d != null && lib.isAttrs d && d ? pname && d ? version;
  linkDeps = p:
    let c = p.getCabalDeps or { }; in
    lib.filter usable ((c.libraryHaskellDepends or [ ]) ++ (c.executableHaskellDepends or [ ]));
  allDeps = p:
    let
      c = p.getCabalDeps or { };
      fields = lib.filter
        (n: builtins.match ".*(HaskellDepends|ToolDepends)" n != null)
        (lib.attrNames c);
    in
    lib.filter (d: usable d && d ? getCabalDeps)
      (lib.concatMap (n: let v = c.${n}; in if lib.isList v then v else [ ]) fields);
  node = p: { key = "${p.pname}-${p.version}"; inherit p; };
  closureOf = deps: lib.genericClosure {
    startSet = map node (deps root);
    operator = e: map node (deps e.p);
  };
  names = cl: map (e: { inherit (e.p) pname version; }) cl;
in
{
  result.root = {
    pname = root.pname or null;
    version = root.version or null;
    has-cabal-deps = root ? getCabalDeps;
  };
  result.packages = names (closureOf linkDeps);
  result.universe = names (closureOf allDeps);
}
