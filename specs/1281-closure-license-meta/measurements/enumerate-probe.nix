let
  f = builtins.getFlake "github:NixOS/nixpkgs?rev=a799d3e3886da994fa307f817a6bc705ae538eeb";
  pkgs = f.legacyPackages.x86_64-linux;
  roots = [ { id="pkgs";   set=pkgs; }
            { id="hsPkgs"; set=pkgs.haskellPackages; }
            { id="ghc910"; set=pkgs.haskell.packages.ghc910; } ];
  lic = l:
    if builtins.isList l then map (x: if builtins.isAttrs x then (x.spdxId or x.shortName or "?") else "?") l
    else if builtins.isAttrs l then [ (l.spdxId or l.shortName or "?") ]
    else if builtins.isString l then [ l ] else [];
  rowIn = root: name:
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p  = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = { inherit name; rootId = root.id; drv = baseNameOf p.drvPath;
              licenses = lic (p.meta.license or null); };
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
  scan = root:
    let nr = builtins.tryEval (builtins.attrNames root.set);
    in if ! nr.success then [ ] else builtins.concatMap (rowIn root) nr.value;
in builtins.concatMap scan roots
