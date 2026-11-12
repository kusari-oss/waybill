#!/usr/bin/env bash
# #1436 research R2: does grype honour `fixed` on a build-qualified product?
#
# The 2026-11-10 probes (shape-probe.sh) measured `fixed` only on an
# UNqualified product (b_fixed) and the qualified product only with
# `not_affected` (q_lone). This probe measures the shape the spec chose.
#
# For each committed --nix-closure golden, over EVERY (component, CVE) pair in
# its pedigree.patches[].resolves:
#   none      grype, no --vex, on the document as committed
#   q_none    grype, no --vex, patched components' PURLs qualified (FR-007 control)
#   fixed_q   one `fixed` statement per pair, product = qualified PURL
#   na_q      one `not_affected` per pair (justification
#             inline_mitigations_already_exist), product = qualified PURL
#   leak      fixed_q's VEX on the UNqualified document: must suppress nothing
#
# Columns: target, variant, findings, patched_reported, patched_suppressed,
# others_lost. "patched" = a finding whose (artifact PURL without qualifiers,
# vulnerability id) is a resolves pair. others_lost counts non-patched
# findings that vanish relative to `none`.
#
# Usage: GRYPE=<grype 0.120.0> GOLDENS=<public_corpus dir> OUT_DIR=$(mktemp -d) \
#          QUAL='build=waybill-probe-1436' ./fixed-qualified-probe.sh
# The grype DB is whatever GRYPE_DB_CACHE_DIR holds; set GRYPE_DB_AUTO_UPDATE=false.
set -euo pipefail
: "${GRYPE:?}" "${GOLDENS:?}" "${OUT_DIR:?}"
QUAL="${QUAL:-build=waybill-probe-1436}"
TARGETS="${TARGETS:-nix-closure-moat nix-closure-openai nix-closure-shellcheck nix-closure-slack-web nix-closure-slacklinker}"

printf 'target\tvariant\tfindings\tpatched_reported\tpatched_suppressed\tothers_lost\n'
for t in $TARGETS; do
  o="$OUT_DIR/$t"; mkdir -p "$o"
  cp "$GOLDENS/$t/cdx.json" "$o/base.cdx.json"
  # (purl, cve) pairs the document's patches resolve.
  jq -r '[.. | objects | select(has("purl") and has("bom-ref"))
          | . as $c | (.pedigree.patches // [])[] | (.resolves // [])[]
          | "\($c.purl)\t\(.id)"] | unique | .[]' "$o/base.cdx.json" > "$o/pairs.tsv"
  # Qualify exactly the components that have a resolves entry (FR-001).
  jq --arg q "$QUAL" '
    ([.. | objects | select(has("purl") and has("bom-ref"))
      | select((.pedigree.patches // []) | any((.resolves // []) | length > 0)) | .purl]
     | unique) as $patched
    | (.. | objects | select(has("purl") and (.purl as $p | $patched | index($p))))
      |= (.purl = .purl + "?" + $q)' "$o/base.cdx.json" > "$o/qualified.cdx.json"
  for status in fixed not_affected; do
    jq -n --rawfile pairs "$o/pairs.tsv" --arg q "$QUAL" --arg s "$status" '
      {"@context":"https://openvex.dev/ns/v0.2.0","@id":"https://example.invalid/1436-probe",
       "author":"waybill #1436 probe","timestamp":"2026-11-12T00:00:00Z","version":1,
       "statements": [$pairs | split("\n")[] | select(length > 0) | split("\t") as [$p, $v]
         | {"vulnerability":{"name":$v},"products":[{"@id":($p + "?" + $q)}],"status":$s}
           + (if $s == "not_affected" then {"justification":"inline_mitigations_already_exist"} else {} end)]}' \
      > "$o/$status.openvex.json"
  done
  scan() { # name doc vex
    if [ -n "$3" ]; then "$GRYPE" "sbom:$2" --vex "$3" -o json > "$o/$1.json" 2> "$o/$1.err"
    else "$GRYPE" "sbom:$2" -o json > "$o/$1.json" 2> "$o/$1.err"; fi
    jq -r '.matches[] | "\(.artifact.purl | split("?")[0])\t\(.vulnerability.id)"' "$o/$1.json" | sort -u > "$o/$1.pairs"
  }
  scan none "$o/base.cdx.json" ""
  scan q_none "$o/qualified.cdx.json" ""
  scan fixed_q "$o/qualified.cdx.json" "$o/fixed.openvex.json"
  scan na_q "$o/qualified.cdx.json" "$o/not_affected.openvex.json"
  scan leak "$o/base.cdx.json" "$o/fixed.openvex.json"
  sort -u "$o/pairs.tsv" > "$o/pairs.sorted"
  comm -12 "$o/none.pairs" "$o/pairs.sorted" > "$o/patched_reported"
  comm -23 "$o/none.pairs" "$o/pairs.sorted" > "$o/others"
  for v in none q_none fixed_q na_q leak; do
    f=$(wc -l < "$o/$v.pairs" | tr -d ' ')
    rep=$(wc -l < "$o/patched_reported" | tr -d ' ')
    sup=$(comm -23 "$o/patched_reported" "$o/$v.pairs" | wc -l | tr -d ' ')
    lost=$(comm -23 "$o/others" "$o/$v.pairs" | wc -l | tr -d ' ')
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$v" "$f" "$rep" "$sup" "$lost"
  done
done
