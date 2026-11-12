#!/usr/bin/env bash
# #1412 — confirm that the members whose row carries no license really declare
# none at the pinned revision (rather than declaring one the projection drops),
# and show expand-response-params' declaration and store path.
set -euo pipefail
REV="${NIX_REV:-a799d3e3886da994fa307f817a6bc705ae538eeb}"
SYS="${NIX_SYSTEM:-aarch64-darwin}"
nix eval --json --option allow-import-from-derivation false --expr "
let pkgs = (builtins.getFlake \"github:NixOS/nixpkgs/$REV\").legacyPackages.$SYS;
    f = p: { hasLicense = p.meta ? license;
             license = p.meta.license or null;
             drv = baseNameOf p.drvPath; };
in { getopt = f pkgs.getopt; apple-sdk = f pkgs.apple-sdk; libunwind = f pkgs.libunwind;
     libredirect = f pkgs.libredirect; expand-response-params = f pkgs.expand-response-params; }"
