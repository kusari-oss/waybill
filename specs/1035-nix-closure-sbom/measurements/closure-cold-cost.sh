#!/usr/bin/env bash
# T-R3 — what does `--nix-closure` cost on a store that has never seen the
# inputs?
#
# The spec quotes warm-store figures. A first scan on a clean machine pays
# for fetching the closure's inputs, and that cost lands inside the same
# wall-clock budget, so an operator whose CI runner starts clean needs the
# cold number, not the warm one.
#
# This probe reports both, and says which it measured. It does NOT fake a
# cold store by deleting paths: garbage-collecting a shared store to take a
# measurement is destructive to everything else on the machine. To get the
# cold figure, run it on a runner whose store is genuinely empty --
# `WAYBILL_COLD=1` records that claim in the output so a warm run can never
# be mistaken for a cold one later.
#
# Usage: closure-cold-cost.sh [flakeref]   (default: the current directory)
set -uo pipefail

REF="${1:-path:$PWD#default}"
COLD="${WAYBILL_COLD:-0}"

store_bytes() {
  # `nix path-info --all --json` is the only portable size source; du on
  # /nix/store counts hardlinks more than once.
  nix path-info --all --json 2>/dev/null \
    | python3 -c 'import json,sys
d=json.load(sys.stdin)
vals = d.values() if isinstance(d, dict) else d
print(sum((v or {}).get("narSize") or 0 for v in vals))' 2>/dev/null || echo 0
}

before=$(store_bytes)
start=$(python3 -c 'import time;print(time.time())')
nix derivation show -r --option allow-import-from-derivation false "$REF" > /dev/null 2>&1
rc=$?
end=$(python3 -c 'import time;print(time.time())')
after=$(store_bytes)

python3 - "$REF" "$COLD" "$rc" "$start" "$end" "$before" "$after" <<'PY'
import sys
ref, cold, rc, start, end, before, after = sys.argv[1:]
grew = int(after) - int(before)
print(f"flakeref        : {ref}")
print(f"store state     : {'declared COLD by the operator' if cold == '1' else 'WARM (lower bound only)'}")
print(f"exit            : {rc}")
print(f"wall seconds    : {float(end) - float(start):.1f}")
print(f"store growth    : {grew / 1e6:.0f} MB")
if cold != "1":
    print()
    print("This is a warm-store figure. It is a lower bound on what a first")
    print("scan costs and must not be quoted as the cold number. Re-run with")
    print("WAYBILL_COLD=1 on a runner whose Nix store is genuinely empty.")
PY
