#!/usr/bin/env bash
# Q2 follow-up: nix orders substituters by each cache's advertised Priority (lower first),
# not by their position in `substituters`. Record what each configured cache advertises.
set -u
cd "$(dirname "$0")"
o=q2-out
for s in $(nix config show substituters); do
  u=${s%/}; f=$(echo "$u" | sed 's#https://##; s#[/:]#_#g')
  curl -sS -m 10 "$u/nix-cache-info" > "$o/cache-info.$f.txt" 2>&1; echo "$s exit=$?" >> "$o/cache-info.exits"
  nix store info --json --store "$s" > "$o/store-info.$f.json" 2>&1
done
