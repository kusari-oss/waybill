#!/usr/bin/env bash
# Milestone 1273 Phase 0 — Probe D.
#
# Re-measure SC-003's second figure. SC-003 reads:
#
#   "Measured today: the two disagree on 2 of 2 directly-marked components,
#    and on 8 components in total on the public library #955 was filed
#    against."
#
# The "2 of 2" comes from `reproduce-1273.sh`, which is run here. The "8"
# does not: it is the one number in spec.md inherited from an issue
# (#1281's sbomnix 1.9.0 cross-check) rather than re-run. The requirements
# checklist flagged it as needing re-measurement in Phase 0 before any task
# depends on it. This is that re-measurement.
#
# The library is `slack-web`, the project #955 was filed against, whose
# `cli` executable is gated behind a flag defaulting to `False`. It is a
# published Hackage package; its coordinates are used deliberately, as the
# project's measurement-probe convention allows, because the subject of the
# measurement is this specific dependency graph.
#
# Its `flake.lock` pins nixpkgs `5ae3b07d8d6527c42f17c876e404993199144b6a`,
# which is in the local per-revision cache, so the scan runs `--offline`.
#
# Usage:  WAYBILL=./target/debug/waybill ./sc003-public-library.sh
set -euo pipefail

WAYBILL="${WAYBILL:-./target/debug/waybill}"
[ -x "$WAYBILL" ] || { echo "no waybill at '$WAYBILL'" >&2; exit 1; }
WAYBILL="$(cd "$(dirname "$WAYBILL")" && pwd)/$(basename "$WAYBILL")"
REPO="${REPO:-https://github.com/MercuryTechnologies/slack-web.git}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
git clone --depth 1 "$REPO" "$work/lib" 2>/dev/null
echo "subject:  $REPO @ $(git -C "$work/lib" rev-parse --short HEAD)"
echo "waybill:  $("$WAYBILL" --version)"
echo "nixpkgs:  $(python3 -c "
import json,sys
d=json.load(open('$work/lib/flake.lock'))
print(next(v['locked']['rev'] for v in d['nodes'].values()
           if v.get('locked',{}).get('repo')=='nixpkgs'))")"
echo

scan() { # <dir> <out>
  "$WAYBILL" --offline sbom scan --path "$1" --no-deep-hash \
    --format cyclonedx-json --output "cyclonedx-json=$2" 2>&1
}

# Case A — the flake.lock removed. The optional marks must hold.
cp -R "$work/lib" "$work/nolock"
rm -f "$work/nolock/flake.lock"
echo "=== case A: no flake.lock ==="
scan "$work/nolock" "$work/a.json" > "$work/a.log" || true

# Case B — as the repository ships, flake.lock pinning nixpkgs.
echo "=== case B: flake.lock pinning nixpkgs ==="
scan "$work/lib" "$work/b.json" > "$work/b.log" || true
sed 's/\x1b\[[0-9;]*m//g' "$work/b.log" \
  | grep -oE "runtime closure resolved declared=[0-9]+ transitive=[0-9]+|restored=[0-9]+" \
  | sed 's/^/  /'

echo
echo "=== the disagreement ==="
python3 - "$work/a.json" "$work/b.json" <<'PY'
import json, sys

def scopes(path):
    d = json.load(open(path))
    out = {}
    for c in d.get("components", []):
        if not (c.get("purl") or "").startswith("pkg:hackage/"):
            continue
        p = {q["name"]: q["value"] for q in c.get("properties", [])}
        out[c["name"]] = (
            p.get("waybill:lifecycle-scope"),
            p.get("waybill:optional-derivation"),
            c.get("version") or "",
        )
    return out

a, b = scopes(sys.argv[1]), scopes(sys.argv[2])
print(f"  hackage components:  A={len(a)}  B={len(b)}")

opt_a = {n for n, v in a.items() if v[0] == "optional"}
opt_b = {n for n, v in b.items() if v[0] == "optional"}
print(f"  classified optional: A={len(opt_a)}  B={len(opt_b)}")

lost = sorted(opt_a - opt_b)
print(f"  optional in A, NOT optional in B: {len(lost)}")
for n in lost:
    print(f"    {n:28} A={a[n][0]}/{a[n][1]} -> B={b[n][0] or 'runtime'}/{b[n][1] or '-'} @ {b[n][2] or '-'}")

gained = sorted(opt_b - opt_a)
if gained:
    print(f"  optional in B but not A ({len(gained)}): {gained}")

marked_a = sorted(n for n, v in a.items()
                  if v[1] == "cabal-flag-disabled-stanza")
print(f"  DIRECTLY marked in A (cabal-flag-disabled-stanza): "
      f"{len(marked_a)} {marked_a}")
print(f"  of those, still marked in B: "
      f"{sorted(n for n in marked_a if b.get(n, (None,None,''))[1] == 'cabal-flag-disabled-stanza')}")
PY

echo
echo "=== the fan-out below the marked pair ==="
echo "    (re-measures the figure #1281 quoted as '8 components in total')"
python3 "$(dirname "$0")/fanout.py" "$work/b.json" "$work/lib/flake.lock" \
  butcher monad-loops
