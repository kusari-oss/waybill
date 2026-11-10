#!/usr/bin/env bash
# Compare before/after finding sets per target per scanner.
set -euo pipefail
B="${BEFORE:?set BEFORE (summarize.sh DEST, pre-fix)}"; A="${AFTER:?set AFTER (summarize.sh DEST, post-fix)}"
for t in curl openssh httpie gh ripgrep moat; do
  for s in grype trivy osv; do
    lost=$(comm -23 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    gained=$(comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | wc -l | tr -d ' ')
    other=$(comm -13 "$B/$t.$s.tsv" "$A/$t.$s.tsv" | awk -F'\t' '$2!="python3"' | wc -l | tr -d ' ')
    echo "$t $s before=$(wc -l < "$B/$t.$s.tsv" | tr -d ' ') after=$(wc -l < "$A/$t.$s.tsv" | tr -d ' ') lost=$lost gained=$gained gained_not_on_python3=$other"
  done
done
echo "--- distinct CVE ids gained on python3, union over targets"
cat "$A"/*.grype.tsv | awk -F'\t' '$2=="python3"{print $1"\t"$3}' | LC_ALL=C sort -u | wc -l
