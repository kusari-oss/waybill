#!/usr/bin/env bash
# Q1: how a program reads the user's configured substituters and trusted keys.
# Compares `nix config show <name>`, `nix show-config`, `nix config show --json`.
set -u
cd "$(dirname "$0")"
o=q1-out; mkdir -p "$o"
for s in substituters trusted-public-keys trusted-substituters extra-substituters require-sigs connect-timeout download-attempts stalled-download-timeout narinfo-cache-negative-ttl narinfo-cache-positive-ttl; do
  nix config show "$s" > "$o/config-show.$s.out" 2> "$o/config-show.$s.err"; echo "$?" > "$o/config-show.$s.exit"
done
nix show-config > "$o/show-config.out" 2> "$o/show-config.err"; echo "$?" > "$o/show-config.exit"
nix config show > "$o/config-show-all.out" 2> "$o/config-show-all.err"; echo "$?" > "$o/config-show-all.exit"
nix config show --json > "$o/config-show.json" 2> "$o/config-show-json.err"; echo "$?" > "$o/config-show-json.exit"
python3 -I -B -c '
import json,sys
d=json.load(open(sys.argv[1]))
for k in ["substituters","trusted-public-keys","trusted-substituters","require-sigs","connect-timeout","download-attempts","stalled-download-timeout","narinfo-cache-negative-ttl"]:
    print(k, json.dumps(d.get(k)))
print("extra-substituters present as key:", "extra-substituters" in d)
' "$o/config-show.json" > "$o/config-show-json.selected"
# Where the values come from, and whether this user is trusted by the daemon.
{ for f in /etc/nix/nix.conf /etc/nix/nix.custom.conf "$HOME/.config/nix/nix.conf"; do echo "== $f"; [ -f "$f" ] && grep -nE 'substituter|public-key|trusted-users|include' "$f" || echo "(absent)"; done; echo "== NIX_CONFIG=${NIX_CONFIG-<unset>}"; } > "$o/conf-sources.txt" 2>&1
nix config show trusted-users > "$o/config-show.trusted-users.out" 2>&1
nix store info --json > "$o/store-info.json" 2> "$o/store-info.err"; echo "$?" > "$o/store-info.exit"
nix store info --store daemon --json > "$o/store-info-daemon.json" 2> "$o/store-info-daemon.err"; echo "$?" > "$o/store-info-daemon.exit"
