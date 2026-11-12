#!/usr/bin/env bash
# alias-census.sh <revision>... — research R4's ambiguity, measured (#1282, 2026-11-12).
#
# R4 deferred versioned aliases because "when two aliases carry the same
# version, something has to choose". For each pinned revision this reports:
#   - how many attributes hackage-packages.nix binds, and whether they are unique;
#   - how many are versioned aliases (attribute != pname);
#   - how many are NEITHER `<pname>` NOR `<pname>_<version with . -> _>` —
#     if zero, the alias for an exact pair is a function of the pair;
#   - how many (pname, version) pairs more than one attribute holds — R4's
#     ambiguity; if zero, no lookup ever has to choose.
#
# Reads the per-revision cache waybill already keeps
# (`$WAYBILL_NIXPKGS_CACHE`, default `~/.cache/waybill/nixpkgs`). The attribute
# head mirrors `attribute_re` in `haskell_packages/package_set.rs`, including a
# head split across two lines (`  name =` then `callPackage`), which that
# regex's `\s*` spans. Cross-check: at a799d3e3 this counts 19,429 attributes,
# the figure `package_set::parse`'s documentation records for that revision.
set -euo pipefail
cache="${WAYBILL_NIXPKGS_CACHE:-$HOME/.cache/waybill/nixpkgs}"
[ "$#" -gt 0 ] || { echo "usage: $0 <revision>..." >&2; exit 64; }
export LC_ALL=C
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
for rev in "$@"; do
  f="$cache/$rev/hackage-packages.nix"
  [ -s "$f" ] || { echo "$rev: no cached hackage-packages.nix at $f" >&2; exit 66; }
  awk '
    function flush() { if (attr != "") print attr "\t" pname "\t" version }
    function start(a) { flush(); gsub(/"/, "", a); attr = a; pname = ""; version = "" }
    pending != "" { if ($0 ~ /^[ \t]*callPackage/) { start(pending); pending = ""; next } pending = "" }
    /^  ("[^"]+"|[A-Za-z0-9][A-Za-z0-9_.'"'"'-]*)[ \t]*=[ \t]*callPackage/ { start($1); next }
    /^  ("[^"]+"|[A-Za-z0-9][A-Za-z0-9_.'"'"'-]*)[ \t]*=[ \t]*$/ { pending = $1; next }
    attr != "" && pname == "" && /pname = "/ { match($0, /"[^"]*"/); pname = substr($0, RSTART+1, RLENGTH-2); next }
    attr != "" && version == "" && /version = "/ { match($0, /"[^"]*"/); version = substr($0, RSTART+1, RLENGTH-2); next }
    END { flush() }
  ' "$f" > "$tmp/attrs.tsv"
  t="$tmp/attrs.tsv"
  echo "== $rev"
  echo "attributes:                                   $(wc -l < "$t" | tr -d ' ')"
  echo "distinct attribute names:                     $(cut -f1 "$t" | sort -u | wc -l | tr -d ' ')"
  echo "without a pname / without a version:          $(awk -F'\t' '$2==""' "$t" | wc -l | tr -d ' ') / $(awk -F'\t' '$3==""' "$t" | wc -l | tr -d ' ')"
  echo "versioned aliases (attribute != pname):       $(awk -F'\t' '$1!=$2' "$t" | wc -l | tr -d ' ')"
  odd=$(awk -F'\t' '{v=$3; gsub(/\./,"_",v); if ($1!=$2 && $1!=($2"_"v)) print}' "$t")
  echo "neither <pname> nor <pname>_<version>:        $(printf '%s' "$odd" | grep -c . || true)"
  [ -z "$odd" ] || printf '%s\n' "$odd" | sed 's/^/    /'
  dup=$(cut -f2,3 "$t" | sort | uniq -d)
  echo "(pname, version) held by more than one:       $(printf '%s' "$dup" | grep -c . || true)"
  [ -z "$dup" ] || printf '%s\n' "$dup" | sed 's/^/    /'
done
