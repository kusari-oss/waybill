#!/usr/bin/env bash
# Spec 1664 (FR-001): Haskell package directories #1032's membership rule
# rejects, per corpus target, and every Nix line in the repository that names
# each one.
#
# "Rejected" is read from the binary, not modelled: a target is scanned twice,
# as committed and with every cabal.project removed (no governing file, so
# nothing is filtered). A manifest directory in the second scan's
# waybill:source-files but not the first's is one the rule rejected.
#
# usage: rejected.sh <waybill-binary> <src-root> <work-dir>
#   <src-root>/<target> holds each target checked out at its corpus pin.
set -euo pipefail
bin=$1 srcs=$2 work=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work/home" "$work/nixpkgs" "$here/rejected"

manifests() { # cdx -> manifest dirs relative to root
  jq -r '[.. | objects | .properties? // empty | .[] | select(.name=="waybill:source-files") | .value] | .[]' "$1" \
    | tr ',' '\n' | sed 's/^[[:space:]"[]*//; s/[]"[:space:]]*$//' \
    | { grep -E '(\.cabal|package\.yaml)$' || true; } | sed "s#^$2/##" \
    | while IFS= read -r f; do dirname "$f"; done | LC_ALL=C sort -u
}
scan() { # dir out
  HOME="$work/home" WAYBILL_NIXPKGS_CACHE="$work/nixpkgs" "$bin" --offline sbom scan --path "$1" \
    --format cyclonedx-json --output "cyclonedx-json=$2" --root-name t --root-version 0 >/dev/null 2>&1
}
printf 'target\tmanifest_dirs_ungoverned\trejected\trejected_with_nix_reference\n'
for d in "$srcs"/*/; do
  t=$(basename "$d"); src=${d%/}
  scan "$src" "$work/$t.gov.json"
  u="$work/ungov/$t"; rm -rf "$u"; mkdir -p "$u"; cp -R "$src/." "$u/"; rm -rf "$u/.git"
  find "$u" -name cabal.project -delete
  scan "$u" "$work/$t.ungov.json"
  manifests "$work/$t.gov.json" "$src" > "$work/$t.gov.txt"
  manifests "$work/$t.ungov.json" "$u" > "$work/$t.ungov.txt"
  LC_ALL=C comm -13 "$work/$t.gov.txt" "$work/$t.ungov.txt" > "$here/rejected/$t.txt"
  : > "$here/rejected/$t.nix-refs.txt"; n=0
  while IFS= read -r dir; do
    hits=$(cd "$src" && grep -rnF --include='*.nix' -e "$dir" . || true)
    if [ -n "$hits" ]; then n=$((n+1)); printf '## %s\n%s\n' "$dir" "$hits" >> "$here/rejected/$t.nix-refs.txt"; fi
  done < "$here/rejected/$t.txt"
  printf '%s\t%s\t%s\t%s\n' "$t" "$(wc -l < "$work/$t.ungov.txt" | tr -d ' ')" "$(wc -l < "$here/rejected/$t.txt" | tr -d ' ')" "$n"
done
