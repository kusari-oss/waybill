#!/usr/bin/env bash
# Evaluate outPath / drvPath / outputs for the probe attributes at the pinned nixpkgs,
# and record whether each output is valid in the local store.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
: > outpaths.tsv
# `pandoc` at this revision IS pandoc-cli (there is no top-level `pandoc-cli` attribute).
for a in shellcheck curl haskellPackages.hlint pandoc haskellPackages.ShellCheck; do
  out=$(nix eval --raw "$R#$a.outPath")
  drv=$(nix eval --raw "$R#$a.drvPath")
  outs=$(nix eval --json "$R#$a.outputs")
  outname=$(nix eval --raw "$R#$a.outputName")
  # `--store local` needs root (big-lock permission denied); the default store is the daemon.
  if nix path-info "$out" >/dev/null 2>&1; then loc=local-valid; else loc=local-absent; fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$a" "$out" "$drv" "$outs" "$outname" "$loc" >> outpaths.tsv
done
