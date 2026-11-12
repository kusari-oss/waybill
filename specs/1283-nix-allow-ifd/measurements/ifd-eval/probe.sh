#!/usr/bin/env bash
# What does `nix derivation show -r` cost with IFD allowed, on flakes already built locally?
# usage: probe.sh <flake-dir> <label>; 3 runs each, refused vs allowed.
set -u
dir=$1; label=$2
for setting in false true; do
  for i in 1 2 3; do
    s=$(python3 -I -B -c 'import time;print(time.time())')
    nix derivation show -r --option allow-import-from-derivation $setting "$dir#default" > "$label.$setting.$i.json" 2> "$label.$setting.$i.err"; rc=$?
    e=$(python3 -I -B -c 'import time;print(time.time())')
    n=$(python3 -I -B -c "import json,sys
try: print(len(json.load(open('$label.$setting.$i.json')).get('derivations', json.load(open('$label.$setting.$i.json')))))
except Exception: print(0)")
    printf '%s ifd=%s run=%s exit=%s secs=%.2f derivations=%s\n' "$label" "$setting" "$i" "$rc" "$(python3 -I -B -c "print($e-$s)")" "$n"
  done
done
