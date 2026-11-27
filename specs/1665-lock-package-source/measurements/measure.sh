#!/usr/bin/env bash
# Spec 1665 (FR-001): every lock- or freeze-pinned Haskell component per corpus
# target, with its source files, reachability from the root and orphan reason,
# for one waybill binary. Read from the binary's output, not modelled.
#
# Flags are the corpus harness's (--offline, --root-name, --root-version).
# HOME and WAYBILL_NIXPKGS_CACHE point at empty directories. The hpack
# `-no-generated-cabal` variants are made as the harness makes them.
#
# usage: measure.sh <waybill-binary> <label> <src-root> <work-dir>
#   <src-root>/<target> holds each target checked out at its corpus pin.
set -euo pipefail
bin=$1 label=$2 srcs=$3 work=$4
here=$(cd "$(dirname "$0")" && pwd)
out="$here/$label"
mkdir -p "$out" "$work/home" "$work/nixpkgs"

remove_generated() {
  find "$1" -name .git -prune -o -type f -name '*.cabal' -print | while IFS= read -r f; do
    if head -n 4 "$f" | grep -q hpack; then rm "$f"; fi
  done
}

scan() { # name dir
  local name=$1 dir=$2 cdx="$work/$label-$1.cdx.json"
  HOME="$work/home" WAYBILL_NIXPKGS_CACHE="$work/nixpkgs" "$bin" --offline sbom scan --path "$dir" \
    --format cyclonedx-json --output "cyclonedx-json=$cdx" --root-name "$name" --root-version 0 >/dev/null 2>&1
  jq -r '([(.dependencies // [])[] | .dependsOn[]?]) as $r
    | [.. | objects | select((.properties // []) | map(select(.name == "waybill:source-type"
         and (.value == "hackage-stack-lock" or .value == "hackage-freeze"))) | length > 0)]
    | unique_by(.purl)
    | map(. as $c | [
        .purl,
        ((.properties | map(select(.name == "waybill:source-type")) | .[0].value)),
        ((.properties | map(select(.name == "waybill:source-files")) | .[0].value) // "-"),
        (if ($r | index($c["bom-ref"])) then "reachable" else "unreachable" end),
        ((.properties | map(select(.name == "waybill:orphan-reason")) | .[0].value) // "-")
      ] | join("\t")) | .[]' "$cdx" > "$out/$name.tsv"
  printf '%s\t%s\t%s\t%s\n' "$name" "$(wc -l < "$out/$name.tsv" | tr -d ' ')" \
    "$(awk -F'\t' '$3=="-" || $3=="[\"\"]"' "$out/$name.tsv" | wc -l | tr -d ' ')" \
    "$(awk -F'\t' '$4=="unreachable" && $5=="-"' "$out/$name.tsv" | wc -l | tr -d ' ')"
}

printf 'target\tlock_components\tno_source_file\tunreachable_unexplained\n'
for d in "$srcs"/*/; do
  t=$(basename "$d"); src=${d%/}
  scan "$t" "$src"
  case $t in hpack-hs-opentelemetry|hpack-lootbox|hpack-ogmios)
    v="$work/variants/$t-no-generated-cabal"; rm -rf "$v"; mkdir -p "$v"; cp -R "$src/." "$v/"; rm -rf "$v/.git"
    remove_generated "$v"; scan "$t-no-generated-cabal" "$v" ;;
  esac
done
