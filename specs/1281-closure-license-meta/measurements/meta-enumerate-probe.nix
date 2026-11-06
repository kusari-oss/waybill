let
  flake = builtins.getFlake "github:NixOS/nixpkgs?rev=a799d3e3886da994fa307f817a6bc705ae538eeb";
  pkgs = flake.legacyPackages.aarch64-darwin;
  lic = l: if builtins.isList l then map (x: x.spdxId or x.shortName or "?") l
           else if builtins.isAttrs l then [ (l.spdxId or l.shortName or "?") ]
           else [];
  safe = name:
    let r = builtins.tryEval (
      let p = pkgs.${name}; in
      if builtins.isAttrs p && (p.type or null) == "derivation"
      then [ { drv = p.drvPath; licenses = lic (p.meta.license or null); } ]
      else [ ]
    );
    in if r.success then r.value else [ ];
in builtins.concatLists (map safe (builtins.attrNames pkgs))
