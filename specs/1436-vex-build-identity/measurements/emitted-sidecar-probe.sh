#!/usr/bin/env bash
# #1436 SC-001/SC-002/SC-004: grype with and without the EMITTED sidecar.
# Reads DOCS/<target>/{cdx.json,waybill.openvex.json}. "patched" = a finding whose
# (artifact PURL without qualifiers, vulnerability id) is in the document's
# pedigree.patches[].resolves. Columns: target findings_novex patched_reported
# patched_suppressed others_lost statements.
# Usage: GRYPE=<grype 0.120.0> GRYPE_DB_CACHE_DIR=<imported v6.1.10 DB> \
#          ./emitted-sidecar-probe.sh <DOCS> <OUT_DIR>
# Pins: see quickstart.md (nixpkgs 0bb2eedc… grype; DB sha256 dcca2472…).
set -euo pipefail
: "${GRYPE:?}" "${GRYPE_DB_CACHE_DIR:?}"
export GRYPE_DB_AUTO_UPDATE=false GRYPE_DB_VALIDATE_AGE=false
G="$GRYPE"
DOCS="$1"; OUT="$2"; TARGETS="${TARGETS:-curl openssh httpie gh ripgrep}"
printf 'target\tfindings_novex\tpatched_reported\tpatched_suppressed\tothers_lost\tstatements\n'
for t in $TARGETS; do
  o="$OUT/$t"; mkdir -p "$o"
  d="$DOCS/$t/cdx.json"; v="$DOCS/$t/waybill.openvex.json"
  jq -r '[.. | objects | select(has("purl") and has("bom-ref"))
          | . as $c | (.pedigree.patches // [])[] | (.resolves // [])[]
          | "\($c.purl | split("?")[0])\t\(.id)"] | unique | .[]' "$d" | sort -u > "$o/pairs"
  "$G" "sbom:$d" -o json > "$o/novex.json" 2> "$o/novex.err"
  jq -r '.matches[] | "\(.artifact.purl | split("?")[0])\t\(.vulnerability.id)"' "$o/novex.json" | sort -u > "$o/novex.pairs"
  if [ -f "$v" ]; then
    "$G" "sbom:$d" --vex "$v" -o json > "$o/vex.json" 2> "$o/vex.err"
    jq -r '.matches[] | "\(.artifact.purl | split("?")[0])\t\(.vulnerability.id)"' "$o/vex.json" | sort -u > "$o/vex.pairs"
    n=$(jq '.statements | length' "$v")
  else
    cp "$o/novex.pairs" "$o/vex.pairs"; n=0
  fi
  comm -12 "$o/novex.pairs" "$o/pairs" > "$o/patched_reported"
  comm -23 "$o/novex.pairs" "$o/pairs" > "$o/others"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$t" \
    "$(wc -l < "$o/novex.pairs" | tr -d ' ')" \
    "$(wc -l < "$o/patched_reported" | tr -d ' ')" \
    "$(comm -23 "$o/patched_reported" "$o/vex.pairs" | wc -l | tr -d ' ')" \
    "$(comm -23 "$o/others" "$o/vex.pairs" | wc -l | tr -d ' ')" "$n"
done
