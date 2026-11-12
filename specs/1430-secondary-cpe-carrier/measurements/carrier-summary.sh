#!/usr/bin/env bash
# Reduce carrier-probe.sh output to one row per (target, variant): total
# findings per scanner, unzip findings, and the set difference against
# `base` (lost = in base and not in the variant; gained = the reverse).
# Gained findings on any component other than unzip are listed in full.
# Usage: OUT_DIR=<carrier-probe.sh's OUT_DIR> carrier-summary.sh
set -euo pipefail
W="${OUT_DIR:?set OUT_DIR}"
printf 'target\tvariant\tgrype\tgrype_unzip\tgrype_lost\tgrype_gained\tgained_not_unzip\ttrivy\ttrivy_diff\tosv\tosv_diff\n'
extra=""
for t in curl openssh httpie gh ripgrep moat; do
  b="$W/$t/base.cdx.json"
  for v in base swap evidence syft1 syft2 allsyft; do
    d="$W/$t/$v.cdx.json"
    g=$(wc -l < "$d.grype.tsv" | tr -d ' ')
    gu=$(awk -F'\t' '$2 == "unzip"' "$d.grype.tsv" | wc -l | tr -d ' ')
    lost=$(LC_ALL=C comm -23 "$b.grype.tsv" "$d.grype.tsv" | wc -l | tr -d ' ')
    gained=$(LC_ALL=C comm -13 "$b.grype.tsv" "$d.grype.tsv" | wc -l | tr -d ' ')
    other=$(LC_ALL=C comm -13 "$b.grype.tsv" "$d.grype.tsv" | awk -F'\t' '$2 != "unzip"')
    on=$(printf '%s' "$other" | grep -c . || true)
    tr_=$(wc -l < "$d.trivy.tsv" | tr -d ' ')
    td=$(LC_ALL=C comm -3 "$b.trivy.tsv" "$d.trivy.tsv" | wc -l | tr -d ' ')
    o=$(wc -l < "$d.osv.tsv" | tr -d ' ')
    od=$(LC_ALL=C comm -3 "$b.osv.tsv" "$d.osv.tsv" | wc -l | tr -d ' ')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$v" "$g" "$gu" "$lost" "$gained" "$on" "$tr_" "$td" "$o" "$od"
    [ -n "$other" ] && extra="$extra$(printf '%s' "$other" | sed "s/^/$t\t$v\t/")"$'\n'
  done
done
printf '\n# gained findings on components other than unzip\n%s' "$extra"
