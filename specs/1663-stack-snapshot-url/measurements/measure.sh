#!/usr/bin/env bash
# Spec 1663 (FR-006): Stack snapshot components, Hackage sets and lock-file
# parse warnings per Haskell corpus target, for one waybill binary.
#
# Flags are the corpus harness's (`--offline`, `--root-name`,
# `--root-version`). HOME and WAYBILL_NIXPKGS_CACHE point at empty
# directories so a developer's caches cannot change the result (spec 1662
# found a local nixpkgs cache adding 13 components). The nix-closure targets
# are scanned `--offline` too, and only when they contain a Stack file: the
# snapshot reader is source-side, and the closure tier needs network.
#
# usage: measure.sh <waybill-binary> <label> <work-dir>
set -euo pipefail
bin=$1 label=$2 work=$3
here=$(cd "$(dirname "$0")" && pwd)
out="$here/$label"
mkdir -p "$out" "$work/empty-home" "$work/empty-nixpkgs"

targets=(
  "haskell-aeson https://github.com/haskell/aeson 682162c66d26a770fbfb6271c797e646bc5c4f2e no"
  "haskell-security-advisories https://github.com/haskell/security-advisories 4dc0b9b921bd71688388b1cd8486c26fde568b76 no"
  "haskell-language-server https://github.com/haskell/haskell-language-server 1b4b3c6bdd2bf8d1e1182e2e770f5dea9198db80 no"
  "hpack-hs-opentelemetry https://github.com/iand675/hs-opentelemetry 25f5b2d65744af384e06f61040e5c037d7cb8db9 yes"
  "hpack-lootbox https://github.com/serokell/lootbox 2cd7b1e0fde3888593404d1e1842d45e1b62bb6c yes"
  "hpack-hs-temporal-sdk https://github.com/mercury/hs-temporal-sdk d24313e37a009cc4121012e509e16a3e749811d3 no"
  "hpack-ogmios https://github.com/CardanoSolutions/ogmios 435189aea2d9a047dc395a6f1cdef737d52f1cbe yes"
  "nix-closure-moat https://github.com/MercuryTechnologies/moat d06905558ba68b49b17f36c875e97f7234a9c19b no"
  "nix-closure-slack-web https://github.com/MercuryTechnologies/slack-web 7b46273d3723ba37da26e4c4f7a3c5575ae9e837 no"
  "nix-closure-openai https://github.com/MercuryTechnologies/openai 8a357137b9d3f1727b1dbb6ea3574e7b1b0fb4b7 no"
  "nix-closure-slacklinker https://github.com/MercuryTechnologies/slacklinker 72af26a71622bcf1eb4b84d017677be711c6fc7b no"
)

remove_generated() {
  find "$1" -name .git -prune -o -type f -name '*.cabal' -print | while IFS= read -r f; do
    if head -n 4 "$f" | grep -q hpack; then rm "$f"; fi
  done
}

scan() { # name dir sha
  local name=$1 dir=$2 sha=$3 cdx="$work/$label-$1.cdx.json" log="$work/$label-$1.log"
  HOME="$work/empty-home" WAYBILL_NIXPKGS_CACHE="$work/empty-nixpkgs" \
    "$bin" --offline sbom scan --path "$dir" --format cyclonedx-json \
    --output "cyclonedx-json=$cdx" --root-name "$name" --root-version "${sha:0:7}" >/dev/null 2>"$log"
  jq -r '[.. | objects | .purl? // empty | select(startswith("pkg:hackage/"))] | unique | .[]' "$cdx" > "$out/$name.hackage.txt"
  # One line per snapshot component: purl, source files, reachable, orphan reason.
  jq -r --arg w "$work" '
    ([(.dependencies // [])[] | .dependsOn[]?]) as $targets
    | [.. | objects | select((.properties // []) | map(select(.name == "waybill:source-type" and .value == "hackage-snapshot")) | length > 0)]
    | map(. as $c | [
        .purl,
        ((.properties | map(select(.name == "waybill:source-files")) | .[0].value) // "-" | gsub($w; "<work>")),
        (if ($targets | index($c["bom-ref"])) then "reachable" else "unreachable" end),
        ((.properties | map(select(.name == "waybill:orphan-reason")) | .[0].value) // "-")
      ] | join("\t"))
    | unique | .[]' "$cdx" > "$out/$name.snapshots.tsv"
  local warn
  warn=$(grep -c 'failed to parse stack.yaml.lock' "$log" || true)
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$name" "${sha:0:7}" \
    "$(wc -l < "$out/$name.hackage.txt" | tr -d ' ')" \
    "$(wc -l < "$out/$name.snapshots.tsv" | tr -d ' ')" "$warn"
}

printf 'label\ttarget\tpin\thackage\tsnapshots\tlock_parse_warnings\n'
for t in "${targets[@]}"; do
  read -r name url sha variant <<<"$t"
  src="$work/src/$name"
  if [ ! -d "$src/.git" ]; then
    git init -q "$src"
    git -C "$src" fetch -q --depth 1 "$url" "$sha"
    git -C "$src" checkout -q FETCH_HEAD
  fi
  test "$(git -C "$src" rev-parse HEAD)" = "$sha"
  case $name in
    nix-closure-*)
      if [ -z "$(find "$src" -name .git -prune -o \( -name stack.yaml -o -name stack.yaml.lock \) -print)" ]; then
        printf '%s\t%s\t%s\tno-stack-files\t-\t-\n' "$label" "$name" "${sha:0:7}"; continue
      fi ;;
  esac
  scan "$name" "$src" "$sha"
  if [ "$variant" = yes ]; then
    v="$work/src/$name-no-generated-cabal"
    rm -rf "$v"; cp -R "$src" "$v"; rm -rf "$v/.git"
    remove_generated "$v"
    scan "$name-no-generated-cabal" "$v" "$sha"
  fi
done
