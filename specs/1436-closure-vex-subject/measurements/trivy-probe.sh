#!/usr/bin/env bash
# #1436: which VEX shapes does Trivy 0.71.1 honour on a waybill document?
#
# Trivy does no CPE matching, so it reports nothing on the `pkg:generic`
# closure components nixpkgs patches (shape-probe.sh: 0 of the subject pair on
# every target). To learn its matching rules anyway, this probe uses a finding
# Trivy DOES report on curl's document -- CVE-2026-41066 on pkg:pypi/lxml@6.0.2
# -- as the subject. The statements are synthetic: nothing claims lxml is
# patched; the probe measures matching mechanics only.
#
# Variants:
#   none        no --vex
#   a_pair      `affected` {lxml}, then `not_affected` {root, subcomponent lxml}
#               (m1035's OpenVEX shape)
#   a_only      the `not_affected` half alone
#   b_only      `not_affected` {product: lxml}
#   b_pair      `affected` {lxml}, then `not_affected` {lxml}
#   b_pair_rev  the same pair, `not_affected` first
#   a_parent    `not_affected` {product: pkg:generic/gtk-doc (lxml's only
#               parent in the document's dependency graph), subcomponent lxml}:
#               is the subcomponent shape honoured for any ancestor?
#   c_embed     vulnerabilities[] embedded in the scanned SBOM: not_affected,
#               affects[].ref = lxml's bom-ref (no --vex)
#   c_link_comp separate CycloneDX VEX, not_affected, affects BOM-Link -> lxml
#   c_link_pair separate CycloneDX VEX in waybill's m1068 CDX shape: exploitable
#               -> lxml, then not_affected -> root
#
# Usage: SRC=<curl's waybill.cdx.json> OUT_DIR=<scratch> trivy-probe.sh
set -uo pipefail
SRC="${SRC:?set SRC}"
W="${OUT_DIR:?set OUT_DIR}"
mkdir -p "$W"
P='pkg:pypi/lxml@6.0.2'
CVE='CVE-2026-41066'
base="$W/base.cdx.json"; cp "$SRC" "$base"
root=$(jq -r '.metadata.component.purl' "$base")
rootref=$(jq -r '.metadata.component["bom-ref"]' "$base")
ref=$(jq -r --arg p "$P" '[.. | objects | select(.purl? == $p) | .["bom-ref"]][0]' "$base")
serial=$(jq -r '.serialNumber | sub("^urn:uuid:"; "")' "$base")
ver=$(jq -r '.version' "$base")

openvex() { jq -n --argjson s "$1" '{"@context":"https://openvex.dev/ns/v0.2.0",
  "@id":"https://waybill.kusari.dev/openvex/probe-1436-trivy","author":"probe-1436",
  "timestamp":"2026-11-10T00:00:00Z","version":1,"statements":$s}'; }
st() { jq -n --arg v "$CVE" --arg s "$1" --arg p "$2" --arg c "$3" '
  {vulnerability:{name:$v}, status:$s,
   products:[{"@id":$p, identifiers:{purl:$p}}
     + (if $c == "" then {} else {subcomponents:[{"@id":$c, identifiers:{purl:$c}}]} end)]}
  + (if $s == "not_affected" then {justification:"vulnerable_code_not_present"} else {} end)'; }
cdxvex() { jq -n --argjson v "$1" '{bomFormat:"CycloneDX", specVersion:"1.6", version:1,
  serialNumber:"urn:uuid:00000000-0000-4000-8000-000000001436", vulnerabilities:$v}'; }
link() { echo "urn:cdx:$serial/$ver#$1"; }

openvex "$( (st affected "$P" ""; st not_affected "$root" "$P") | jq -s .)" > "$W/a_pair.vex.json"
openvex "$(st not_affected "$root" "$P" | jq -s .)" > "$W/a_only.vex.json"
openvex "$(st not_affected "$P" "" | jq -s .)" > "$W/b_only.vex.json"
openvex "$( (st affected "$P" ""; st not_affected "$P" "") | jq -s .)" > "$W/b_pair.vex.json"
openvex "$( (st not_affected "$P" ""; st affected "$P" "") | jq -s .)" > "$W/b_pair_rev.vex.json"
openvex "$(st not_affected 'pkg:generic/gtk-doc@1.35.1' "$P" | jq -s .)" > "$W/a_parent.vex.json"
jq --arg r "$ref" --arg v "$CVE" \
  '.vulnerabilities = [{id:$v, analysis:{state:"not_affected", justification:"code_not_present"}, affects:[{ref:$r}]}]' \
  "$base" > "$W/c_embed.cdx.json"
cdxvex "$(jq -n --arg v "$CVE" --arg r "$(link "$ref")" \
  '[{id:$v, analysis:{state:"not_affected", justification:"code_not_present"}, affects:[{ref:$r}]}]')" \
  > "$W/c_link_comp.vex.json"
cdxvex "$(jq -n --arg v "$CVE" --arg c "$(link "$ref")" --arg r "$(link "$rootref")" \
  '[{id:$v, analysis:{state:"exploitable"}, affects:[{ref:$c}]},
    {id:$v, analysis:{state:"not_affected", justification:"code_not_present"}, affects:[{ref:$r}]}]')" \
  > "$W/c_link_pair.vex.json"

tsv() { jq -r '.Results[]?.Vulnerabilities[]? | [.PkgIdentifier.PURL, .VulnerabilityID] | @tsv' "$1" | LC_ALL=C sort -u; }
printf 'variant\texit\tfindings\tsubject_reported\tothers_lost\n'
run() { # $1 variant, $2 sbom, $3 vex or ""
  local a=(); [ -n "$3" ] && a=(--vex "$3")
  trivy sbom --quiet --skip-db-update --format json "${a[@]}" --output "$W/$1.trivy.json" "$2" 2>"$W/$1.trivy.err"
  local e=$?
  tsv "$W/$1.trivy.json" > "$W/$1.tsv"
  local key; key=$(printf '%s\t%s' "$P" "$CVE")
  printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$e" "$(wc -l < "$W/$1.tsv" | tr -d ' ')" \
    "$(grep -c -F "$key" "$W/$1.tsv")" \
    "$(LC_ALL=C comm -23 "$W/none.tsv" "$W/$1.tsv" | grep -v -c -F "$key")"
}
run none "$base" ""
for v in a_pair a_only a_parent b_only b_pair b_pair_rev c_link_comp c_link_pair; do run "$v" "$base" "$W/$v.vex.json"; done
run c_embed "$W/c_embed.cdx.json" ""
