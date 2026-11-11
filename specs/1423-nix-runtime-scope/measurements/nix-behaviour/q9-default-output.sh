#!/usr/bin/env bash
# Q9 (d): can the evaluated attribute's default output be read from the closure
# itself, so the runtime query needs no second evaluation of the scanned flake?
#
# `outPath` is the first entry of the derivation's `outputs` attribute.
# `nix derivation show` keys its `outputs` object alphabetically, so the order
# survives only in that attribute (`env.outputs`, or `structuredAttrs.outputs`).
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q9-out; mkdir -p "$o"
for a in shellcheck curl haskellPackages.hlint pandoc; do
  # By drvPath: `nix derivation show "$R#curl"` itself fails at this revision
  # with "'…curl.meta.outputsToInstall' is not a list of strings" (q9 case b).
  nix derivation show "$(nix eval --raw "$R#$a.drvPath")" > "$o/d-$a.drv.json"
  nix eval --raw "$R#$a.outPath" > "$o/d-$a.outpath"
  python3 -I -B q9-default-output.py "$o/d-$a.drv.json" "$o/d-$a.outpath" > "$o/d-$a.default-output.txt"
done
cat "$o"/d-*.default-output.txt
