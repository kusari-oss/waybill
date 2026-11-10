# Each looked-up name's `meta.license` exactly as nixpkgs states it, unprojected,
# keyed by `.drv` basename — the m1281 join key. Same two roots and the same
# three guards as `eval/licenses.rs::licenses_expr`; only the projection is
# absent, so what the reader's `one`/`lic` would make of each value can be
# judged against the value itself.
{ rev, system, names }:
let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/${rev}").legacyPackages.${system};
  roots = [ { id = "pkgs"; set = pkgs; } { id = "haskellPackages"; set = pkgs.haskellPackages; } ];
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = { attr = name; root = root.id; drv = baseNameOf p.drvPath;
              license = p.meta.license or null; };
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) names
