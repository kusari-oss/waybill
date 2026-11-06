let
  f = builtins.getFlake "github:NixOS/nixpkgs?rev=a799d3e3886da994fa307f817a6bc705ae538eeb";
  pkgs = f.legacyPackages.x86_64-linux;
  names = builtins.fromJSON (builtins.readFile ./names.json);

  roots = [ { id="pkgs";   set=pkgs; }
            { id="hsPkgs"; set=pkgs.haskellPackages; }
            { id="ghc94";  set=pkgs.haskell.packages.ghc94; }
            { id="ghc96";  set=pkgs.haskell.packages.ghc96; }
            { id="ghc910"; set=pkgs.haskell.packages.ghc910; } ];

  lic = l:
    if builtins.isList l then map (x: if builtins.isAttrs x then (x.spdxId or x.shortName or "?") else "?") l
    else if builtins.isAttrs l then [ (l.spdxId or l.shortName or "?") ]
    else if builtins.isString l then [ l ] else [];

  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p  = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = { inherit name; rootId = root.id; drv = baseNameOf p.drvPath;
              licenses = lic (p.meta.license or null); };
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) names
