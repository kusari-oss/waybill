#!/usr/bin/env bash
# #1436 step 1: which VEX statement shape does a scanner honour for ONE
# patched CVE on ONE component of a real waybill `--nix-closure` document?
#
# Subject: CVE-2014-8139 on pkg:generic/unzip@6.0, which nixpkgs patches by
# filename (`pedigree.patches[].resolves` in every target's document).
#
# Base document: the target's waybill CycloneDX document plus one
# `syft:cpe23 = unzip_project:unzip` property on unzip -- #1430's `syft1`
# variant (PR #1435), without which grype matches nothing on unzip at all.
# Every variant below shares that base, so the VEX shape is the only variable.
#
# OpenVEX variants (passed with --vex):
#   a_pair         today's m1035 shape: `affected` {product: unzip}, then
#                  `not_affected` {product: root PURL, subcomponent: unzip}
#   a_only         the `not_affected` half of a_pair alone
#   a_generic      `not_affected` {product: pkg:generic/<root name>@<root
#                  version>, subcomponent: unzip} -- the product grype
#                  synthesises from the SBOM (openvex/implementation.go)
#   a_generic_pair `affected` {unzip} + a_generic
#   b_only         `not_affected` {product: unzip}
#   b_pair         `affected` {unzip}, then `not_affected` {unzip}
#   b_pair_rev     `not_affected` {unzip}, then `affected` {unzip}
#   b_fixed        `fixed` {product: unzip}
#   b_pair_ts      b_pair_rev's order, but `affected` stamped 2026-01-01 and
#                  `not_affected` 2026-11-10: does a timestamp beat order?
# Build-qualified PURL variants. The SBOM's unzip PURL gains a qualifier
# (`?build=waybill-probe-1436`) standing for "this build's unzip":
#   q_none         no --vex, on the qualified document (control)
#   q_affected     `affected` {product: unzip, unqualified} only
#   q_pair         `affected` {unqualified}, then `not_affected` {qualified}
#   q_pair_rev     `not_affected` {qualified}, then `affected` {unqualified}
#   q_lone         `not_affected` {qualified} alone
#   q_lone_other   q_lone's VEX on the UNqualified base document: a statement
#                  scoped to this build must not suppress another unzip 6.0
# CycloneDX variants:
#   c_embed_na     vulnerabilities[] in the scanned SBOM, analysis.state
#                  not_affected, affects[].ref = unzip's bom-ref (no --vex)
#   c_embed_res    same, analysis.state resolved
#   c_vexflag      a separate CycloneDX VEX BOM passed with --vex, affects[].ref
#                  a BOM-Link to unzip (urn:cdx:<serial>/<version>#<bom-ref>)
#
# Output: one TSV row per (target, variant, scanner): total findings, whether
# the subject pair is still reported, and how many OTHER findings vanished
# (must be 0 -- a shape that suppresses more than its subject overclaims).
#
# Usage: SRC_DIR=<dir of <target>/waybill.cdx.json> OUT_DIR=<scratch> \
#        GRYPE=<grype 0.120.0> shape-probe.sh [targets...]
set -uo pipefail
SRC="${SRC_DIR:?set SRC_DIR}"
W="${OUT_DIR:?set OUT_DIR}"
GRYPE="${GRYPE:?set GRYPE}"
export GRYPE_DB_AUTO_UPDATE=false
TARGETS="${*:-curl openssh httpie gh ripgrep moat}"
P='pkg:generic/unzip@6.0'
CVE='CVE-2014-8139'
B='cpe:2.3:a:unzip_project:unzip:6.0:*:*:*:*:*:*:*'

openvex() { # $1 = JSON array of statements
  jq -n --argjson s "$1" '{"@context":"https://openvex.dev/ns/v0.2.0",
    "@id":"https://waybill.kusari.dev/openvex/probe-1436","author":"probe-1436",
    "timestamp":"2026-11-10T00:00:00Z","version":1,"statements":$s}'
}
st() { # $1 status, $2 product id, $3 subcomponent id or "", $4 timestamp or ""
  jq -n --arg v "$CVE" --arg s "$1" --arg p "$2" --arg c "$3" --arg ts "${4:-}" '
    {vulnerability:{name:$v}, status:$s,
     products:[{"@id":$p, identifiers:{purl:$p}}
       + (if $c == "" then {} else {subcomponents:[{"@id":$c, identifiers:{purl:$c}}]} end)]}
    + (if $s == "not_affected" then {justification:"vulnerable_code_not_present"} else {} end)
    + (if $ts == "" then {} else {timestamp:$ts} end)'
}
arr() { jq -s '.' ; }

# PURL qualifiers are dropped so the q_* variants compare against `none`.
grype_tsv() { jq -r '.matches[] | [(.artifact.purl | sub("[?].*$"; "")), .vulnerability.id] | @tsv' "$1" | LC_ALL=C sort -u; }
trivy_tsv() { jq -r '.Results[]?.Vulnerabilities[]? | [(.PkgIdentifier.PURL | sub("[?].*$"; "")), .VulnerabilityID] | @tsv' "$1" | LC_ALL=C sort -u; }

printf 'target\tvariant\tscanner\texit\tfindings\tsubject_reported\tothers_lost\n'
for t in $TARGETS; do
  src="$SRC/$t/waybill.cdx.json"
  [ -s "$src" ] || { echo "missing $src" >&2; exit 1; }
  o="$W/$t"; mkdir -p "$o"
  base="$o/base.cdx.json"
  jq --arg p "$P" --arg b "$B" \
    '(.. | objects | select(.purl? == $p)) |= (.properties += [{name:"syft:cpe23", value:$b}])' \
    "$src" > "$base"
  root=$(jq -r '.metadata.component.purl' "$base")
  gen="pkg:generic/$(jq -r '.metadata.component.name | ascii_downcase' "$base")@$(jq -r '.metadata.component.version' "$base")"
  ref=$(jq -r --arg p "$P" '[.. | objects | select(.purl? == $p) | .["bom-ref"]][0]' "$base")
  serial=$(jq -r '.serialNumber | sub("^urn:uuid:"; "")' "$base")
  ver=$(jq -r '.version' "$base")

  openvex "$( (st affected "$P" ""; st not_affected "$root" "$P") | arr)" > "$o/a_pair.vex.json"
  openvex "$(st not_affected "$root" "$P" | arr)" > "$o/a_only.vex.json"
  openvex "$(st not_affected "$gen" "$P" | arr)" > "$o/a_generic.vex.json"
  openvex "$( (st affected "$P" ""; st not_affected "$gen" "$P") | arr)" > "$o/a_generic_pair.vex.json"
  openvex "$(st not_affected "$P" "" | arr)" > "$o/b_only.vex.json"
  openvex "$( (st affected "$P" ""; st not_affected "$P" "") | arr)" > "$o/b_pair.vex.json"
  openvex "$( (st not_affected "$P" ""; st affected "$P" "") | arr)" > "$o/b_pair_rev.vex.json"
  openvex "$(st fixed "$P" "" | arr)" > "$o/b_fixed.vex.json"
  openvex "$( (st not_affected "$P" "" 2026-11-10T00:00:00Z; st affected "$P" "" 2026-01-01T00:00:00Z) | arr)" \
    > "$o/b_pair_ts.vex.json"
  Q="$P?build=waybill-probe-1436"
  jq --arg p "$P" --arg q "$Q" '(.. | objects | select(.purl? == $p)) |= (.purl = $q)' "$base" > "$o/qualified.cdx.json"
  openvex "$(st affected "$P" "" | arr)" > "$o/q_affected.vex.json"
  openvex "$( (st affected "$P" ""; st not_affected "$Q" "") | arr)" > "$o/q_pair.vex.json"
  openvex "$( (st not_affected "$Q" ""; st affected "$P" "") | arr)" > "$o/q_pair_rev.vex.json"
  openvex "$(st not_affected "$Q" "" | arr)" > "$o/q_lone.vex.json"
  for s in not_affected resolved; do
    jq --arg r "$ref" --arg v "$CVE" --arg s "$s" \
      '.vulnerabilities = [{id:$v, analysis:({state:$s} + (if $s == "not_affected" then {justification:"code_not_present"} else {} end)), affects:[{ref:$r}]}]' \
      "$base" > "$o/c_embed_$s.cdx.json"
  done
  jq -n --arg r "urn:cdx:$serial/$ver#$ref" --arg v "$CVE" \
    '{bomFormat:"CycloneDX", specVersion:"1.6", version:1,
      serialNumber:"urn:uuid:00000000-0000-4000-8000-000000001436",
      vulnerabilities:[{id:$v, analysis:{state:"not_affected", justification:"code_not_present"},
                        affects:[{ref:$r}]}]}' > "$o/c_vexflag.vex.json"

  run() { # $1 variant, $2 sbom, $3 vex file or ""
    local v="$1" sbom="$2" vex="$3" ge te
    local gargs=() targs=()
    [ -n "$vex" ] && { gargs=(--vex "$vex"); targs=(--vex "$vex"); }
    "$GRYPE" "sbom:$sbom" "${gargs[@]}" -o json --file "$o/$v.grype.json" >/dev/null 2>"$o/$v.grype.err"; ge=$?
    trivy sbom --quiet --skip-db-update --format json "${targs[@]}" --output "$o/$v.trivy.json" "$sbom" \
      2>"$o/$v.trivy.err"; te=$?
    if [ "$ge" = 0 ]; then grype_tsv "$o/$v.grype.json" > "$o/$v.grype.tsv"; else : > "$o/$v.grype.tsv"; fi
    if [ "$te" = 0 ]; then trivy_tsv "$o/$v.trivy.json" > "$o/$v.trivy.tsv"; else : > "$o/$v.trivy.tsv"; fi
    for sc in grype trivy; do
      local e; [ "$sc" = grype ] && e=$ge || e=$te
      local n subj lost
      n=$(wc -l < "$o/$v.$sc.tsv" | tr -d ' ')
      subj=$(grep -c -F "$(printf '%s\t%s' "$P" "$CVE")" "$o/$v.$sc.tsv")
      lost=$(LC_ALL=C comm -23 "$o/none.$sc.tsv" "$o/$v.$sc.tsv" \
        | grep -v -c -F "$(printf '%s\t%s' "$P" "$CVE")")
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$v" "$sc" "$e" "$n" "$subj" "$lost"
    done
  }
  run none "$base" ""
  for v in a_pair a_only a_generic a_generic_pair b_only b_pair b_pair_rev b_fixed b_pair_ts c_vexflag; do
    run "$v" "$base" "$o/$v.vex.json"
  done
  run q_none "$o/qualified.cdx.json" ""
  for v in q_affected q_pair q_pair_rev q_lone; do
    run "$v" "$o/qualified.cdx.json" "$o/$v.vex.json"
  done
  run q_lone_other "$base" "$o/q_lone.vex.json"
  run c_embed_na "$o/c_embed_not_affected.cdx.json" ""
  run c_embed_res "$o/c_embed_resolved.cdx.json" ""
done
