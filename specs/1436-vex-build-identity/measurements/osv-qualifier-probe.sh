#!/usr/bin/env bash
# #1436 research R3: does a PURL qualifier cost osv-scanner findings?
# Qualifies EVERY component PURL in a document (all ecosystems, harsher than
# FR-001, which qualifies only components with a resolved CVE) and compares
# osv-scanner's (package, advisory) pairs with the unqualified document.
# Usage: OSV=<osv-scanner 2.7.0> DOC=<cdx.json> OUT_DIR=$(mktemp -d) QUAL='build=x' ./osv-qualifier-probe.sh
set -uo pipefail
: "${OSV:?}" "${DOC:?}" "${OUT_DIR:?}"
QUAL="${QUAL:-build=waybill-probe-1436}"
cp "$DOC" "$OUT_DIR/base.cdx.json"
jq --arg q "$QUAL" '(.. | objects | select(has("purl") and has("bom-ref")) | select(.purl | test("\\?") | not)) |= (.purl = .purl + "?" + $q)' "$DOC" > "$OUT_DIR/qualified.cdx.json"
for v in base qualified; do
  "$OSV" scan source -L "$OUT_DIR/$v.cdx.json" --format json > "$OUT_DIR/$v.json" 2> "$OUT_DIR/$v.err"
  jq -r '.results[]?.packages[]? | . as $p | .vulnerabilities[]? | "\($p.package.ecosystem)\t\($p.package.name)\t\($p.package.version)\t\(.id)"' "$OUT_DIR/$v.json" | sort -u > "$OUT_DIR/$v.pairs"
done
printf 'variant\tpurls_qualified\tecosystems\tpairs\tlost_vs_base\tgained_vs_base\n'
q=$(jq '[.. | objects | select(has("purl") and has("bom-ref")) | select(.purl | test("\\?"))] | length' "$OUT_DIR/qualified.cdx.json")
eco=$(cut -f1 "$OUT_DIR/base.pairs" | sort -u | paste -sd, -)
for v in base qualified; do
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$v" "$([ $v = base ] && echo 0 || echo $q)" "$eco" "$(wc -l < "$OUT_DIR/$v.pairs" | tr -d ' ')" \
    "$(comm -23 "$OUT_DIR/base.pairs" "$OUT_DIR/$v.pairs" | wc -l | tr -d ' ')" "$(comm -13 "$OUT_DIR/base.pairs" "$OUT_DIR/$v.pairs" | wc -l | tr -d ' ')"
done
