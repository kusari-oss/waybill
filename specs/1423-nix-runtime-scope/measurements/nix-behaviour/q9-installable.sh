#!/usr/bin/env bash
# Q9 (T001, implementation time): does `nix path-info -r` accept the INSTALLABLE
# (<flakeref>#<attr>) the plan intends to pass, rather than the outPath Q2 used?
#
# Two questions Q2 did not ask, because Q2 always passed an outPath:
#   (a) with `--store <binary-cache-url>`, where does evaluation write the .drv?
#   (b) which outputs does an installable select: `outputName`, or
#       `meta.outputsToInstall`?
# Each case writes <case>.out / .err / .exit / .secs under q9-out/.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q9-out; mkdir -p "$o"
B=(--option connect-timeout 2 --option download-attempts 1)
run() { local name=$1; shift; echo "$*" > "$o/$name.cmd"; python3 -I -B timed.py "$o/$name" "$@"; }

for a in shellcheck curl; do
  nix eval --raw "$R#$a.outPath" > "$o/$a.outpath"
  nix eval --raw "$R#$a.outputName" > "$o/$a.outputname"
  nix eval --json "$R#$a.meta.outputsToInstall" > "$o/$a.outputs-to-install" 2> "$o/$a.outputs-to-install.err"
  nix eval --json "$R#$a.outputs" > "$o/$a.outputs"

  # (a) remote store, installable, default eval store (= the --store URL)
  run "a-$a-remote-installable"            nix path-info -r --store https://cache.nixos.org/ "${B[@]}" "$R#$a"
  # (a) the same with the evaluation kept on the local (daemon) store
  run "a-$a-remote-installable-evalauto"   nix path-info -r --eval-store auto --store https://cache.nixos.org/ "${B[@]}" "$R#$a"
  # (b) local store, installable
  run "b-$a-local-installable"             nix path-info -r "$R#$a"
  # (b) remote, outPath (the Q2 shape), for comparison of the selected closure
  run "b-$a-remote-outpath"                nix path-info -r --store https://cache.nixos.org/ "${B[@]}" "$(cat "$o/$a.outpath")"
  # (c) store verify, installable vs outPath
  run "c-$a-verify-installable-evalauto"   nix store verify --no-contents -r --eval-store auto --store https://cache.nixos.org/ "${B[@]}" "$R#$a"
  run "c-$a-verify-outpath"                nix store verify --no-contents -r --store https://cache.nixos.org/ "${B[@]}" "$(cat "$o/$a.outpath")"
done
