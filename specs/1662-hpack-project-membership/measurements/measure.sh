#!/usr/bin/env bash
# Spec 1662 (FR-006): each target's Haskell component set and C200 value,
# for one waybill binary. Same flags as the corpus harness
# (waybill-cli/tests/corpus_harness_195/harness.rs): --offline, --root-name,
# --root-version; the `-no-generated-cabal` variants delete every *.cabal
# whose first four lines contain `hpack`, as `remove_hpack_generated_cabal` does.
#
# usage: measure.sh <waybill-binary> <label> <work-dir>
#   writes <this dir>/<label>/<target>.txt  (sorted unique pkg:hackage purls)
#          <this dir>/<label>/<target>.c200.json
set -euo pipefail
bin=$1 label=$2 work=$3
here=$(cd "$(dirname "$0")" && pwd)
out="$here/$label"
mkdir -p "$out" "$work"

targets=(
  "haskell-language-server https://github.com/haskell/haskell-language-server 1b4b3c6bdd2bf8d1e1182e2e770f5dea9198db80 no"
  "hpack-hs-opentelemetry https://github.com/iand675/hs-opentelemetry 25f5b2d65744af384e06f61040e5c037d7cb8db9 yes"
  "hpack-lootbox https://github.com/serokell/lootbox 2cd7b1e0fde3888593404d1e1842d45e1b62bb6c yes"
  "hpack-hs-temporal-sdk https://github.com/mercury/hs-temporal-sdk d24313e37a009cc4121012e509e16a3e749811d3 no"
  "hpack-ogmios https://github.com/CardanoSolutions/ogmios 435189aea2d9a047dc395a6f1cdef737d52f1cbe yes"
)

remove_generated() {
  find "$1" -name .git -prune -o -type f -name '*.cabal' -print | while IFS= read -r f; do
    if head -n 4 "$f" | grep -q hpack; then rm "$f"; fi
  done
}

scan() { # name dir sha
  local name=$1 dir=$2 sha=$3
  "$bin" --offline sbom scan --path "$dir" --format cyclonedx-json \
    --output "cyclonedx-json=$work/$label-$name.cdx.json" \
    --root-name "$name" --root-version "${sha:0:7}" >/dev/null 2>"$work/$label-$name.log"
  jq -r '[.. | objects | .purl? // empty | select(startswith("pkg:hackage/"))] | unique | .[]' \
    "$work/$label-$name.cdx.json" > "$out/$name.txt"
  # Paths are rewritten under `<work>` so no local path is committed.
  jq --arg w "$work" '[.. | objects | select(.name? == "waybill:hpack-read-outcome") | .value | gsub($w; "<work>")] | unique' \
    "$work/$label-$name.cdx.json" > "$out/$name.c200.json"
  printf '%s\t%s\t%s\t%s\n' "$label" "$name" "$sha" "$(wc -l < "$out/$name.txt" | tr -d ' ')"
}

for t in "${targets[@]}"; do
  read -r name url sha variant <<<"$t"
  src="$work/src/$name"
  if [ ! -d "$src/.git" ]; then
    git init -q "$src"
    git -C "$src" fetch -q --depth 1 "$url" "$sha"
    git -C "$src" checkout -q FETCH_HEAD
  fi
  test "$(git -C "$src" rev-parse HEAD)" = "$sha"
  scan "$name" "$src" "$sha"
  if [ "$variant" = yes ]; then
    v="$work/src/$name-no-generated-cabal"
    rm -rf "$v"; cp -R "$src" "$v"; rm -rf "$v/.git"
    remove_generated "$v"
    scan "$name-no-generated-cabal" "$v" "$sha"
  fi
done
