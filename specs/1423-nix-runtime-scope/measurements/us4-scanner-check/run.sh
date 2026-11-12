#!/usr/bin/env bash
# T035 (SC-001, SC-002): the documented runtime-only recipe, run live.
#
# For each of the five milestone-1377 wrapper flakes:
#   1. scan with --nix-closure in all three formats, with gen-sboms.sh's flags
#      (specs/1377-nix-subject-identity/measurements/gen-sboms.sh), deps.dev on;
#   2. apply the recipe, `sbom edit <doc> --drop 'scope=excluded'`, to each
#      format;
#   3. run grype on the full and the runtime-only CycloneDX document.
#
# The real nix answers every query: all five outputs are in the local store,
# so the scans take the runtime closure (C216 `store: local`).
#
# usage: run.sh   (env: WAYBILL, GRYPE, TARGETS_DIR, US4_DIR)
set -uo pipefail
WAYBILL="${WAYBILL:?set WAYBILL to the waybill binary under test}"
GRYPE="${GRYPE:?set GRYPE to grype 0.120.0}"
TARGETS_DIR="${TARGETS_DIR:?set TARGETS_DIR to the wrapper flakes}"
US4_DIR="${US4_DIR:?set US4_DIR for the documents and scanner output}"
export GRYPE_DB_AUTO_UPDATE=false

for t in curl openssh httpie gh ripgrep; do
  o="$US4_DIR/$t"; mkdir -p "$o"
  s=$(date +%s)
  ( cd "$TARGETS_DIR/$t" && "$WAYBILL" sbom scan --no-deep-hash \
      --nix-closure --no-clearly-defined --path . \
      --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
      --output "cyclonedx-json=$o/waybill.cdx.json" \
      --output "spdx-2.3-json=$o/waybill.spdx.json" \
      --output "spdx-3-json=$o/waybill.spdx3.json" >"$o/waybill.log" 2>&1 )
  echo "$t scan exit=$? $(( $(date +%s)-s ))s"
  for f in cdx spdx spdx3; do
    "$WAYBILL" sbom edit "$o/waybill.$f.json" --drop 'scope=excluded' \
      -o "$o/runtime.$f.json" >"$o/edit.$f.log" 2>&1
    echo "$t edit $f exit=$?"
  done
  for d in waybill runtime; do
    "$GRYPE" "sbom:$o/$d.cdx.json" -o json --file "$o/$d.cdx.json.grype.json" \
      >/dev/null 2>"$o/$d.cdx.json.grype.err"
    echo "$t grype $d exit=$?"
  done
done
