#!/usr/bin/env bash
# grype + osv-scanner over one package's arms: scan.sh <pkg> [arms...]   (default arms: W WR S)
# Invocations as nixhs-assess3/scripts/scan.sh (trivy omitted: not part of this task's truth).
set -uo pipefail
R="${RECALL_CORPUS_DIR:?set RECALL_CORPUS_DIR to the recall-corpus work directory (wrappers/, out/, realise/)}"
# grype 0.120.0 from nixpkgs 0bb2eedc20e4739f29e8045034f2828ad7054f8f; override with GRYPE=<path>.
GRYPE="${GRYPE:-grype}"
export GRYPE_DB_AUTO_UPDATE=false
p="$1"; shift; arms="${*:-W WR S}"
for f in $arms; do
  s="$R/out/$p/$f.cdx.json"; [ -s "$s" ] || { echo "$p $f MISSING"; continue; }
  st=$(date +%s); "$GRYPE" "sbom:$s" -o json --file "$s.grype.json" >/dev/null 2>"$s.grype.err"
  echo "$p $f grype exit=$? $(( $(date +%s)-st ))s" | tee -a "$R/out/scan-exits.txt"
  # osv-scanner exits 1 when it FINDS vulns; >1 is an error.
  st=$(date +%s); osv-scanner scan source -L "$s" --format json >"$s.osv.json" 2>"$s.osv.err"
  echo "$p $f osv exit=$? $(( $(date +%s)-st ))s" | tee -a "$R/out/scan-exits.txt"
done
