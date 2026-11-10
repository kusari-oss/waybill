#!/usr/bin/env bash
# #1430 step 1: which carrier makes grype read a component's SECOND CPE?
#
# Takes a real waybill CycloneDX document per target (a `--nix-closure` scan
# that contains `pkg:generic/unzip@6.0`), writes hand-edited copies that put
# the second curated pair (`unzip_project:unzip`) in one candidate carrier
# each, and scans every copy with grype, Trivy and osv-scanner.
#
# Variants (each edits ONLY the unzip component unless stated):
#   base         unchanged document (component.cpe = info-zip:unzip)
#   swap         positive control: component.cpe = unzip_project:unzip
#   evidence     + evidence.identity[] {field: cpe, concludedValue: unzip_project}
#   syft1        + property syft:cpe23 = unzip_project
#   syft2        + property syft:cpe23 = waybill-fixture:unzip (matches nothing),
#                  then syft:cpe23 = unzip_project — is a later value read?
#   allsyft      every component carrying waybill:cpe-candidates gets
#                syft:cpe23 for candidates[1..] (preview of the fix's reach)
#
# Usage: SRC_DIR=<dir of <target>/waybill.cdx.json> OUT_DIR=<scratch> \
#        GRYPE=<grype 0.120.0> carrier-probe.sh [targets...]
set -uo pipefail
SRC="${SRC_DIR:?set SRC_DIR}"
W="${OUT_DIR:?set OUT_DIR}"
GRYPE="${GRYPE:?set GRYPE}"
export GRYPE_DB_AUTO_UPDATE=false
TARGETS="${*:-curl openssh httpie gh ripgrep moat}"
P='pkg:generic/unzip@6.0'
B='cpe:2.3:a:unzip_project:unzip:6.0:*:*:*:*:*:*:*'
F='cpe:2.3:a:waybill-fixture:unzip:6.0:*:*:*:*:*:*:*'
SEL='(.. | objects | select(.purl? == $p))'

variant_filter() {
  case "$1" in
    base) echo '.' ;;
    swap) echo "$SEL |= (.cpe = \$b)" ;;
    evidence) echo "$SEL |= (.evidence.identity += [{field: \"cpe\", concludedValue: \$b, confidence: 0.7, methods: [{technique: \"other\", confidence: 0.7, value: \"waybill curated CPE table\"}]}])" ;;
    syft1) echo "$SEL |= (.properties += [{name: \"syft:cpe23\", value: \$b}])" ;;
    syft2) echo "$SEL |= (.properties += [{name: \"syft:cpe23\", value: \$f}, {name: \"syft:cpe23\", value: \$b}])" ;;
    allsyft) echo '(.. | objects | select(has("bom-ref") and ((.properties // []) | any(.name == "waybill:cpe-candidates")))) |= (.properties += ([.properties[] | select(.name == "waybill:cpe-candidates") | .value | fromjson | .[1:][]] | map({name: "syft:cpe23", value: .})))' ;;
  esac
}

for t in $TARGETS; do
  src="$SRC/$t/waybill.cdx.json"
  [ -s "$src" ] || { echo "missing $src" >&2; exit 1; }
  mkdir -p "$W/$t"
  for v in base swap evidence syft1 syft2 allsyft; do
    d="$W/$t/$v.cdx.json"
    jq --arg p "$P" --arg b "$B" --arg f "$F" "$(variant_filter "$v")" "$src" > "$d" \
      || { echo "jq failed: $t $v" >&2; exit 1; }
    "$GRYPE" "sbom:$d" -o json --file "$d.grype.json" >/dev/null 2>"$d.grype.err"; ge=$?
    trivy sbom --quiet --format json --output "$d.trivy.json" "$d" 2>"$d.trivy.err"; te=$?
    # osv-scanner exits 1 when it FINDS vulns; >1 is an error.
    osv-scanner scan source -L "$d" --format json >"$d.osv.json" 2>"$d.osv.err"; oe=$?
    jq -r '.matches[] | [.vulnerability.id, .artifact.name, .artifact.version] | @tsv' \
      "$d.grype.json" | LC_ALL=C sort -u > "$d.grype.tsv"
    jq -r '.Results[]?.Vulnerabilities[]? | [.VulnerabilityID, .PkgName, .InstalledVersion] | @tsv' \
      "$d.trivy.json" | LC_ALL=C sort -u > "$d.trivy.tsv"
    jq -r '.results[]?.packages[]? | . as $p | .vulnerabilities[]? | [.id, $p.package.name, $p.package.version] | @tsv' \
      "$d.osv.json" | LC_ALL=C sort -u > "$d.osv.tsv"
    echo "$t $v grype_exit=$ge trivy_exit=$te osv_exit=$oe"
  done
done
