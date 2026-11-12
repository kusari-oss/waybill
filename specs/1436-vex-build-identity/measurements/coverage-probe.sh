#!/usr/bin/env bash
# #1436: per-target before/after counts for each candidate VEX shape, over
# EVERY CVE nixpkgs patches by filename in the build -- not one subject.
#
# The patched set is read from the waybill document itself: every
# (component PURL, CVE) in `pedigree.patches[].resolves[]`. A finding is
# "patched" when grype reports that exact (artifact PURL, vulnerability id).
#
# Shapes (one OpenVEX document per target per shape, statements generated
# from the patched set):
#   today      m1035 as emitted: `affected` {product: component}, then
#              `not_affected` {product: metadata.component PURL, subcomponent:
#              component}. On the five wrapper flakes waybill emits nothing
#              today (#1436 part 1); this is what it WOULD emit with the
#              subject taken from metadata.component.
#   lone_na    only `not_affected` {product: component}. Drops m1035's
#              `affected` and names the version as the product.
#   pair_grype `affected` {component}, then `not_affected` {component}, in
#              that document order (grype resolves a tie to the LAST).
#   pair_trivy the same pair in the opposite order (Trivy resolves a tie to
#              the FIRST, see README).
#
# Bases: `waybill` is the document as scanned; `unzip_cpe` adds the
# `syft:cpe23 = unzip_project:unzip` property #1435 surfaces (#1430's syft1
# variant), without which grype matches nothing on unzip.
#
# Output TSV: target, base, shape, findings, patched_reported,
# patched_suppressed, others_lost (must be 0).
#
# Usage: SRC_DIR=<dir of <target>/waybill.cdx.json> OUT_DIR=<scratch> \
#        GRYPE=<grype 0.120.0> coverage-probe.sh [targets...]
set -uo pipefail
SRC="${SRC_DIR:?set SRC_DIR}"
W="${OUT_DIR:?set OUT_DIR}"
GRYPE="${GRYPE:?set GRYPE}"
export GRYPE_DB_AUTO_UPDATE=false
TARGETS="${*:-curl openssh httpie gh ripgrep moat}"
UNZIP='pkg:generic/unzip@6.0'
UNZIP_CPE='cpe:2.3:a:unzip_project:unzip:6.0:*:*:*:*:*:*:*'

doc() { # stdin: JSON array of statements
  jq '{"@context":"https://openvex.dev/ns/v0.2.0",
    "@id":"https://waybill.kusari.dev/openvex/probe-1436-coverage","author":"probe-1436",
    "timestamp":"2026-11-10T00:00:00Z","version":1,"statements":.}'
}

gen() { # $1 shape, $2 patched.tsv, $3 root purl
  jq -R -s --arg shape "$1" --arg root "$3" '
    def p($x): {"@id":$x, identifiers:{purl:$x}};
    def st($s; $cve; $prod): {vulnerability:{name:$cve}, status:$s, products:[$prod]}
      + (if $s == "not_affected" then {justification:"vulnerable_code_not_present"} else {} end);
    [ split("\n")[] | select(length > 0) | split("\t") | {c:.[0], v:.[1]} ]
    | map(
        if   $shape == "today"      then st("affected"; .v; p(.c)), st("not_affected"; .v; p($root) + {subcomponents:[p(.c)]})
        elif $shape == "lone_na"    then st("not_affected"; .v; p(.c))
        elif $shape == "pair_grype" then st("affected"; .v; p(.c)), st("not_affected"; .v; p(.c))
        elif $shape == "pair_trivy" then st("not_affected"; .v; p(.c)), st("affected"; .v; p(.c))
        else error("unknown shape") end)' "$2" | doc
}

tsv() { jq -r '.matches[] | [.artifact.purl, .vulnerability.id] | @tsv' "$1" | LC_ALL=C sort -u; }

printf 'target\tbase\tshape\tfindings\tpatched_reported\tpatched_suppressed\tothers_lost\n'
for t in $TARGETS; do
  src="$SRC/$t/waybill.cdx.json"
  [ -s "$src" ] || { echo "missing $src" >&2; exit 1; }
  o="$W/$t"; mkdir -p "$o"
  jq -r '[.. | objects | select(.pedigree?.patches?) | .purl as $p
          | .pedigree.patches[] | .resolves[]?.id | "\($p)\t\(.)"] | unique | .[]' "$src" \
    | LC_ALL=C sort -u > "$o/patched.tsv"
  root=$(jq -r '.metadata.component.purl' "$src")
  cp "$src" "$o/waybill.cdx.json"
  jq --arg p "$UNZIP" --arg b "$UNZIP_CPE" \
    '(.. | objects | select(.purl? == $p)) |= (.properties += [{name:"syft:cpe23", value:$b}])' \
    "$src" > "$o/unzip_cpe.cdx.json"
  for shape in today lone_na pair_grype pair_trivy; do
    gen "$shape" "$o/patched.tsv" "$root" > "$o/$shape.vex.json"
  done
  for base in waybill unzip_cpe; do
    "$GRYPE" "sbom:$o/$base.cdx.json" -o json --file "$o/$base.none.grype.json" >/dev/null 2>&1 \
      || { echo "grype failed: $t $base none" >&2; exit 1; }
    tsv "$o/$base.none.grype.json" > "$o/$base.none.tsv"
    LC_ALL=C comm -12 "$o/patched.tsv" "$o/$base.none.tsv" > "$o/$base.patched_reported.tsv"
    LC_ALL=C comm -23 "$o/$base.none.tsv" "$o/patched.tsv" > "$o/$base.others.tsv"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$base" none \
      "$(wc -l < "$o/$base.none.tsv" | tr -d ' ')" \
      "$(wc -l < "$o/$base.patched_reported.tsv" | tr -d ' ')" 0 0
    for shape in today lone_na pair_grype pair_trivy; do
      f="$o/$base.$shape.grype.json"
      "$GRYPE" "sbom:$o/$base.cdx.json" --vex "$o/$shape.vex.json" -o json --file "$f" >/dev/null 2>&1 \
        || { echo "grype failed: $t $base $shape" >&2; exit 1; }
      tsv "$f" > "$o/$base.$shape.tsv"
      supp=$(LC_ALL=C comm -23 "$o/$base.patched_reported.tsv" "$o/$base.$shape.tsv" | wc -l | tr -d ' ')
      lost=$(LC_ALL=C comm -23 "$o/$base.others.tsv" "$o/$base.$shape.tsv" | wc -l | tr -d ' ')
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$base" "$shape" \
        "$(wc -l < "$o/$base.$shape.tsv" | tr -d ' ')" \
        "$(wc -l < "$o/$base.patched_reported.tsv" | tr -d ' ')" "$supp" "$lost"
    done
  done
done
