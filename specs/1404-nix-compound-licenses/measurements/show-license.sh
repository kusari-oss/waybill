#!/usr/bin/env bash
set -euo pipefail
REV=a799d3e3886da994fa307f817a6bc705ae538eeb
SYS=${SYS:-aarch64-darwin}
nix eval --json --expr "let p = (builtins.getFlake \"github:NixOS/nixpkgs/$REV\").legacyPackages.$SYS; in p.${1:-llvm}.meta.license"
