let
  flake = builtins.getFlake "github:NixOS/nixpkgs?rev=a799d3e3886da994fa307f817a6bc705ae538eeb";
  pkgs = flake.legacyPackages.aarch64-darwin;
  lic = l: if builtins.isList l then map (x: x.spdxId or x.shortName or "?") l
           else if builtins.isAttrs l then [ (l.spdxId or l.shortName or "?") ]
           else [];
  pick = name: let p = pkgs.${name}; in {
    inherit name;
    drv = p.drvPath;
    out = "${p}";
    licenses = lic (p.meta.license or null);
  };
in map pick [ "bash" "autoconf" "bison" "brotli" ]
