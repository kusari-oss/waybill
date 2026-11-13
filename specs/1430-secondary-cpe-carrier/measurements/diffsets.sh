#!/usr/bin/env bash
# Compare before/after finding sets per target per scanner (1377's
# summarize.sh output), as specs/nix-curated-cpe-gaps/measurements/diffsets.sh
# does. "gained_elsewhere" counts gained findings on any component that did NOT
# gain a `syft:cpe23` property; it must be 0 for the gain to be attributable
# to the new carrier.
set -euo pipefail
B="${BEFORE:?set BEFORE (summarize.sh DEST, pre-fix)}"; A="${AFTER:?set AFTER (summarize.sh DEST, post-fix)}"
ROWS='^(c-ares|file|flex|jq|less|unzip)$'
for t in curl openssh httpie gh ripgrep moat; do
  for s in grype trivy osv; do
    lost=$(LC_ALL=C comm -23 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    gained=$(LC_ALL=C comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    other=$(LC_ALL=C comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | awk -F'\t' -v r="$ROWS" '$2 !~ r' | wc -l | tr -d ' ')
    echo "$t $s before=$(wc -l < "$B/$t.$s.tsv" | tr -d ' ') after=$(wc -l < "$A/$t.$s.tsv" | tr -d ' ') lost=$lost gained=$gained gained_elsewhere=$other"
  done
done
echo "--- grype findings gained, by component (count of targets)"
for t in curl openssh httpie gh ripgrep moat; do
  LC_ALL=C comm -13 "$B/$t.grype.tsv" "$A/$t.grype.tsv"
done | LC_ALL=C sort | uniq -c
