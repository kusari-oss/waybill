#!/usr/bin/env bash
# Compare before/after finding sets per target per scanner (summarize.sh output).
# "gained_elsewhere" counts gained findings on any component other than the
# seven new rows; it must be 0 for the gain to be attributable to them.
set -euo pipefail
B="${BEFORE:?set BEFORE (summarize.sh DEST, pre-fix)}"; A="${AFTER:?set AFTER (summarize.sh DEST, post-fix)}"
ROWS='^(cjson|fontforge|giflib|libass|libsndfile|libssh|unbound)$'
for t in curl openssh httpie gh ripgrep moat; do
  for s in grype trivy osv; do
    lost=$(comm -23 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    gained=$(comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    other=$(comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | awk -F'\t' -v r="$ROWS" '$2 !~ r' | wc -l | tr -d ' ')
    echo "$t $s before=$(wc -l < "$B/$t.$s.tsv" | tr -d ' ') after=$(wc -l < "$A/$t.$s.tsv" | tr -d ' ') lost=$lost gained=$gained gained_elsewhere=$other"
  done
done
echo "--- grype findings gained, by component (union of targets)"
for t in curl openssh httpie gh ripgrep moat; do
  comm -13 "$B/$t.grype.tsv" "$A/$t.grype.tsv" | awk -F'\t' -v t="$t" '{print t"\t"$2"\t"$3}'
done | LC_ALL=C sort | uniq -c
