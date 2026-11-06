let
  flake = builtins.getFlake "github:NixOS/nixpkgs?rev=a799d3e3886da994fa307f817a6bc705ae538eeb";
  pkgs  = flake.legacyPackages.aarch64-darwin;
  names = builtins.fromJSON (builtins.readFile ./names.json);

  roots = [ { id = "pkgs"; set = pkgs; }
            { id = "haskellPackages"; set = pkgs.haskellPackages; } ];

  lic = l:
    if builtins.isList l then map (x: if builtins.isAttrs x then (x.spdxId or x.shortName or "?") else "?") l
    else if builtins.isAttrs l then [ (l.spdxId or l.shortName or "?") ]
    else if builtins.isString l then [ l ]
    else [];

  # tryEval catches throw/assert ONLY. A missing attribute, and a type error
  # from a null alias, both escape it -- so they are guarded structurally first.
  # getAttr/hasAttr (not `.`/`?`) so a name containing a dot stays one key.
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let
      pr = builtins.tryEval (builtins.getAttr name root.set);
      p  = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let
      v = { inherit name; rootId = root.id; drv = p.drvPath; out = "${p}";
            licenses = lic (p.meta.license or null); };
      r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];

  rows = builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) names;
in rows
