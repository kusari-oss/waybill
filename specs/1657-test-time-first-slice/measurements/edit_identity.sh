#!/bin/bash
# usage: edit_identity.sh <before-bin> <after-bin> <public_corpus dir>
# Edits every corpus golden, in all three formats, with three recipes, with
# both binaries, and cmp's each pair (spec 1657 FR-002). RFC 3339 timestamps
# are masked in both outputs first: the derivation record's `created` is the
# edit's wall-clock time, so a binary disagrees with itself between runs
# (measured: before vs before differs only there). sbom edit has no
# SOURCE_DATE_EPOCH. The edited document's `-edit-<16 hex>` IRI suffix is
# masked too: it varies between runs of one binary (measured). One line per pair,
# then a summary; exits 1 on any output difference or any exit-status difference.
B=$1; A=$2; C=$3; T=$(mktemp -d); bad=0; n=0
recipes=("--drop scope=excluded" "--drop scope=development,test" "--retier design=build")
for d in "$C"/*/; do t=$(basename "$d")
  for f in cdx.json spdx-2.3.json spdx-3.json; do
    for i in 0 1 2; do
      n=$((n+1))
      "$B" sbom edit "$d$f" ${recipes[$i]} -o "$T/b" >/dev/null 2>&1; rb=$?
      "$A" sbom edit "$d$f" ${recipes[$i]} -o "$T/a" >/dev/null 2>&1; ra=$?
      for x in a b; do [ -f "$T/$x" ] && sed -E -i '' -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z/TS/g' -e 's/-edit-[0-9a-f]{16}/-edit-H/g' "$T/$x"; done
      if [ $rb -ne $ra ]; then echo "EXIT-DIFF $t/$f r$i before=$rb after=$ra"; bad=$((bad+1))
      elif [ $rb -eq 0 ] && ! cmp -s "$T/b" "$T/a"; then echo "OUTPUT-DIFF $t/$f r$i"; bad=$((bad+1))
      else echo "same $t/$f r$i exit=$rb"; fi
      rm -f "$T/a" "$T/b"
    done
  done
done
echo "pairs=$n differing=$bad"; rm -rf "$T"; [ $bad -eq 0 ]
