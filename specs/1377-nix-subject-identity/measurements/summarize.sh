#!/usr/bin/env bash
# Reduce one probe run (gen-sboms.sh + scan.sh output) to what is committed:
# per-target, per-scanner finding sets, the document subject, and C184.
# Raw scanner JSON stays where scan.sh wrote it (outside the repository).
# Usage: OUT_DIR=<scan.sh's OUT_DIR> DEST=<directory to write> summarize.sh
set -euo pipefail
W="${OUT_DIR:?set OUT_DIR}"
D="${DEST:?set DEST}"
mkdir -p "$D"
for t in curl openssh httpie gh ripgrep moat; do
  s="$W/$t/waybill.cdx.json"
  [ -s "$s" ] || { echo "missing $s" >&2; exit 1; }
  # Finding sets: one `id<TAB>package<TAB>version` row per finding, sorted and
  # de-duplicated, so two runs compare with `diff`.
  jq -r '.matches[] | [.vulnerability.id, .artifact.name, .artifact.version] | @tsv' \
    "$s.grype.json" | LC_ALL=C sort -u > "$D/$t.grype.tsv"
  jq -r '.Results[]?.Vulnerabilities[]? | [.VulnerabilityID, .PkgName, .InstalledVersion] | @tsv' \
    "$s.trivy.json" | LC_ALL=C sort -u > "$D/$t.trivy.tsv"
  jq -r '.results[]?.packages[]? | . as $p | .vulnerabilities[]? | [.id, $p.package.name, $p.package.version] | @tsv' \
    "$s.osv.json" | LC_ALL=C sort -u > "$D/$t.osv.tsv"
  # The subject, as each consumer reads it from the CycloneDX document.
  jq '{name: .metadata.component.name, version: .metadata.component.version,
       purl: .metadata.component.purl, cpe: .metadata.component.cpe,
       heuristic: ([.metadata.properties[]? | select(.name == "waybill:root-selection-heuristic") | .value] | first)}' \
    "$s" > "$D/$t.subject.json"
  # C184, parsed out of its string property.
  jq '[.metadata.properties[]? | select(.name == "waybill:nix-closure") | .value | fromjson] | first' \
    "$s" > "$D/$t.c184.json"
done
( cd "$D" && wc -l ./*.tsv ) > "$D/counts.txt"
