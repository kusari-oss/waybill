#!/bin/bash
# #1455: collect what the documentation-output rule needs for one built output,
# from the local store only (nothing is built or substituted).
#
#   collect.sh <outdir> <installed output store path>...
#
# Writes:
#   pathinfo.json   nix path-info -r --json: each runtime path's `references`
#   plain.txt       nix path-info -r: the listing waybill queried before #1455
#   drvs.json       nix derivation show -r of each output's deriver: the closure's
#                   `outputs.<name>.path`, which names the output a path is
#   timing.txt      wall seconds for the plain and the --json query, 3 runs each
set -eu
out=$1; shift
mkdir -p "$out"
nix --version > "$out/nix-version.txt"
nix path-info -r "$@" > "$out/plain.txt"
nix path-info -r --json "$@" > "$out/pathinfo.json"
: > "$out/timing.txt"
for i in 1 2 3; do
  s=$(python3 -I -c 'import time; print(time.time())')
  nix path-info -r "$@" > /dev/null
  m=$(python3 -I -c 'import time; print(time.time())')
  nix path-info -r --json "$@" > /dev/null
  e=$(python3 -I -c 'import time; print(time.time())')
  python3 -I -c "print('plain %.3f  json %.3f' % ($m - $s, $e - $m))" >> "$out/timing.txt"
done
drvs=()
for p in "$@"; do drvs+=("$(nix path-info --json "$p" | python3 -I -c 'import json,sys; v=json.load(sys.stdin); v=v if isinstance(v,dict) else {x["path"]:x for x in v}; print(next(iter(v.values()))["deriver"])')"); done
nix derivation show -r "${drvs[@]}" > "$out/drvs.json"
