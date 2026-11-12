#!/usr/bin/env bash
# #1430 step 1, SPDX 2.3 half: what do grype and Trivy do with SEVERAL
# `SECURITY / cpe23Type` externalRefs on one package?
#
# A minimal SPDX 2.3 document with one package, `unzip 6.0`, shaped like
# waybill's (a PACKAGE-MANAGER purl ref, then CPE refs). Variants:
#   primary   info-zip only              (what waybill emits today)
#   both      info-zip, then unzip_project
#   reversed  unzip_project, then info-zip
#   second    unzip_project only         (positive control)
#
# Usage: OUT_DIR=<scratch> GRYPE=<grype 0.120.0> spdx-probe.sh
set -uo pipefail
W="${OUT_DIR:?set OUT_DIR}"
GRYPE="${GRYPE:?set GRYPE}"
export GRYPE_DB_AUTO_UPDATE=false
A='cpe:2.3:a:info-zip:unzip:6.0:*:*:*:*:*:*:*'
B='cpe:2.3:a:unzip_project:unzip:6.0:*:*:*:*:*:*:*'
mkdir -p "$W/spdx"

doc() {
  jq -n --argjson cpes "$1" '{
    spdxVersion: "SPDX-2.3", dataLicense: "CC0-1.0", SPDXID: "SPDXRef-DOCUMENT",
    name: "waybill-fixture-1430", documentNamespace: "https://example.invalid/waybill-fixture-1430",
    creationInfo: {created: "2026-11-10T00:00:00Z", creators: ["Tool: waybill-fixture"]},
    packages: [{
      SPDXID: "SPDXRef-unzip", name: "unzip", versionInfo: "6.0",
      downloadLocation: "NOASSERTION", filesAnalyzed: false,
      externalRefs: ([{referenceCategory: "PACKAGE-MANAGER", referenceType: "purl",
                       referenceLocator: "pkg:generic/unzip@6.0"}]
                     + ($cpes | map({referenceCategory: "SECURITY", referenceType: "cpe23Type",
                                     referenceLocator: .})))
    }],
    relationships: [{spdxElementId: "SPDXRef-DOCUMENT", relationshipType: "DESCRIBES",
                     relatedSpdxElement: "SPDXRef-unzip"}]
  }'
}

printf 'variant\tgrype_exit\tgrype_unzip\ttrivy_exit\ttrivy_findings\n'
for v in primary both reversed second; do
  case "$v" in
    primary)  c="[\"$A\"]" ;;
    both)     c="[\"$A\",\"$B\"]" ;;
    reversed) c="[\"$B\",\"$A\"]" ;;
    second)   c="[\"$B\"]" ;;
  esac
  d="$W/spdx/$v.spdx.json"
  doc "$c" > "$d"
  "$GRYPE" "sbom:$d" -o json --file "$d.grype.json" >/dev/null 2>"$d.grype.err"; ge=$?
  trivy sbom --quiet --format json --output "$d.trivy.json" "$d" 2>"$d.trivy.err"; te=$?
  gu=$(jq '[.matches[] | select(.artifact.name == "unzip") | .vulnerability.id] | unique | length' "$d.grype.json")
  tf=$(jq '[.Results[]?.Vulnerabilities[]?] | length' "$d.trivy.json")
  printf '%s\t%s\t%s\t%s\t%s\n' "$v" "$ge" "$gu" "$te" "$tf"
done
