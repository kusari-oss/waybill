#!/usr/bin/env bash
# Q5 follow-up (T041 follow-up): what does nix do with a connect-timeout that
# is not a whole number of seconds, or is zero? waybill shrinks the timeout to
# the remaining remote budget, so it must know which values nix honours.
# No network: `nix config show` only reports the effective setting.
set -u
cd "$(dirname "$0")"
o=q5-out
mkdir -p "$o"
for v in 2 1 1.5 0.5 0; do
  nix config show --option connect-timeout "$v" connect-timeout \
    >"$o/c-connect-timeout-$v.out" 2>"$o/c-connect-timeout-$v.err"
  echo $? >"$o/c-connect-timeout-$v.exit"
done
nix config show --json | python3 -I -B -c \
  'import json,sys; d=json.load(sys.stdin)["connect-timeout"]; print(json.dumps({k: d[k] for k in ("defaultValue", "description")}, indent=1))' \
  >"$o/c-connect-timeout-description.json"
for v in 2 1 1.5 0.5 0; do
  printf '%-4s exit=%s effective=%s stderr=%s\n' "$v" \
    "$(cat "$o/c-connect-timeout-$v.exit")" \
    "$(tr -d '\n' <"$o/c-connect-timeout-$v.out")" \
    "$(tr -d '\n' <"$o/c-connect-timeout-$v.err")"
done | tee "$o/c-connect-timeout-summary.txt"
