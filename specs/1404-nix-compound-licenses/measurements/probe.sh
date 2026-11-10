#!/usr/bin/env bash
# #1404 probe: which closure members declare a compound `meta.license`, and
# what waybill emits for them.
#
# Usage:
#   TARGETS_DIR=<copied wrapper flakes> OUT_DIR=<dir> WAYBILL=<binary> \
#     LABEL=<before|after> bash probe.sh
#
# TARGETS_DIR holds copies of specs/1377-nix-subject-identity/measurements/
# wrapper-flakes/*, each `git init`ed and committed (see README.md). Never
# point it at the in-repo copies: evaluating a flake in place can re-lock it.
#
# LABEL=before also takes the shape census (census.json per target); the
# census is a property of nixpkgs and the closure, not of the binary.
# SKIP_SCAN=1 re-takes the census from an existing before run.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TARGETS_DIR="${TARGETS_DIR:?}"
OUT_DIR="${OUT_DIR:?}"
LABEL="${LABEL:?before|after}"
TARGETS="${TARGETS:-curl openssh httpie gh ripgrep}"
REV=a799d3e3886da994fa307f817a6bc705ae538eeb
SYSTEM=${SYSTEM:-aarch64-darwin}
NIX_REAL=$(command -v nix)

census() {
  local d=$1 o=$2
  "$NIX_REAL" derivation show -r "path:$d#default" >"$o/closure.json" 2>/dev/null
  # The names the reader asked the license evaluation for, from its own argv.
  python3 -I - "$o/nix-argv.log" >"$o/names.json" <<'PY'
import json, re, sys
for rec in open(sys.argv[1], "rb").read().split(b"\x1e"):
    args = [a.decode() for a in rec.split(b"\0") if a]
    expr = next((a for a in args if "rowIn" in a), None)
    if expr:
        print(json.dumps(re.findall(r'"([^"]+)"', expr.rsplit("roots) [", 1)[1])))
        break
else:
    print("[]")
PY
  # The names were admitted by waybill's is_safe_attribute_name, so they
  # carry no quote or interpolation and can be written as Nix literals.
  local nixlist
  nixlist=$(python3 -I -c 'import json,sys;print("[ "+" ".join("\""+n+"\"" for n in json.load(open(sys.argv[1])))+" ]")' "$o/names.json")
  "$NIX_REAL" eval --json --file "$HERE/raw-licenses.nix" \
      --apply "f: f { rev = \"$REV\"; system = \"$SYSTEM\"; names = $nixlist; }" >"$o/raw-rows.json"
  python3 -I "$HERE/classify.py" "$o/closure.json" "$o/raw-rows.json" >"$o/census.json"
}

shimdir=$(mktemp -d)
cp "$HERE/nix-logging-shim.sh" "$shimdir/nix"
chmod +x "$shimdir/nix"

for t in $TARGETS; do
  d="$TARGETS_DIR/$t"; o="$OUT_DIR/$LABEL/$t"; mkdir -p "$o"
  if [ -z "${SKIP_SCAN:-}" ]; then
    WAYBILL="${WAYBILL:?}"
    : >"$o/nix-argv.log"
    ( cd "$d" && PATH="$shimdir:$PATH" NIX_REAL="$NIX_REAL" NIX_SHIM_LOG="$o/nix-argv.log" \
        "$WAYBILL" sbom scan --no-deep-hash --nix-closure --no-clearly-defined --no-deps-dev \
          --path . \
          --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
          --output "cyclonedx-json=$o/waybill.cdx.json" \
          --output "spdx-2.3-json=$o/waybill.spdx23.json" \
          --output "spdx-3-json=$o/waybill.spdx3.json" >"$o/waybill.log" 2>&1 )
    echo "$t $LABEL waybill exit=$?"
  fi
  if [ "$LABEL" = before ]; then
    census "$d" "$o"
    echo "$t census: $(python3 -I -c 'import json,sys;d=json.load(open(sys.argv[1]));print(d["members_with_a_row"], "members with a row,", d["members_using_an_operator"], "use an operator,", d["lost_before_1404"], "lost before #1404")' "$o/census.json")"
  fi
done
rm -rf "$shimdir"
