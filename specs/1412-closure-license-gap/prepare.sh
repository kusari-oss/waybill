#!/usr/bin/env bash
# #1412 — produce the inputs the accounting probe reads, for each wrapper flake.
#
# Usage: WAYBILL=<binary> WORK=<dir outside the repo> bash prepare.sh [target...]
#
# The flakes are copied from specs/1377-nix-subject-identity/measurements/
# wrapper-flakes/ into $WORK/flakes/ first (evaluating a flake in place can
# re-lock it), then for each target:
#   closure.json  `nix derivation show -r path:<flake>#default`
#   dump.json     `waybill --offline debug closure-dump` over it (population)
#   sbom.cdx.json `waybill sbom scan --nix-closure` (what is emitted)
set -euo pipefail
: "${WAYBILL:?set WAYBILL to the waybill binary}"
: "${WORK:?set WORK to a directory outside the repository}"
here="$(cd "$(dirname "$0")" && pwd)"
src="$here/../1377-nix-subject-identity/measurements/wrapper-flakes"
targets=("$@")
[ ${#targets[@]} -gt 0 ] || targets=(curl gh httpie openssh ripgrep)

mkdir -p "$WORK/flakes"
for t in "${targets[@]}"; do
  [ -d "$WORK/flakes/$t" ] || cp -R "$src/$t" "$WORK/flakes/$t"
  out="$WORK/out/$t"
  mkdir -p "$out"
  nix derivation show -r "path:$WORK/flakes/$t#default" > "$out/closure.json"
  "$WAYBILL" --offline debug closure-dump --closure-json "$out/closure.json" > "$out/dump.json"
  (cd "$WORK/flakes/$t" && "$WAYBILL" sbom scan --nix-closure --no-deep-hash --path . \
      --format cyclonedx-json --output "$out/sbom.cdx.json" 2> "$out/scan.log")
  echo "$t: closure $(wc -c < "$out/closure.json") bytes, sbom $(wc -c < "$out/sbom.cdx.json") bytes"
done
