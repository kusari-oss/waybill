#!/usr/bin/env bash
# #1374 — census of closure members carrying C206 `package`, read from the
# committed corpus goldens (no flake is evaluated). Per target: the C206
# counts, then each `package` member's name-shape group with the dependency
# edges into and out of it in CycloneDX `dependencies[]`.
#
# The groups are a reading aid for the spec, not a proposed classifier.
set -euo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
for t in moat openai shellcheck slack-web slacklinker; do
  f="$root/waybill-cli/tests/fixtures/public_corpus/nix-closure-$t/cdx.json"
  echo "== nix-closure-$t: $(jq -r '[..|objects|select(has("bom-ref") and has("type"))|.properties[]?|select(.name=="waybill:closure-member-role")|.value]|group_by(.)|map("\(.[0])=\(length)")|join(" ")' "$f")"
  jq -r '
   ([..|objects|select(has("bom-ref") and has("type"))|select(any(.properties[]?; .name=="waybill:closure-member-role" and .value=="package"))|{key:.["bom-ref"],value:.name}]|from_entries) as $p
   | [.dependencies[]? | .ref as $r | (.dependsOn//[])[] as $d | [$r,$d]] as $e
   | $p | to_entries[] | .key as $k | .value as $n
   | ($n|if test("-get-version-") then "get-version" elif test("-simple-program-") then "simple-program" elif test("(-source|-src-[0-9.]+|-vendor|\\.patch)$") then "source-tree" elif test("-env$") then "env" else "other" end) as $g
   | "\($g)\t\([$e[]|select(.[1]==$k)]|length)\t\([$e[]|select(.[0]==$k)]|length)\t\($n)"' "$f" \
  | LC_ALL=C sort \
  | awk -F'\t' '{n[$1]++; i[$1]+=$2; o[$1]+=$3; m[$1]=m[$1] " " $4} END {for (g in n) printf "  %-14s members=%-3d in-edges=%-3d out-edges=%-3d %s\n", g, n[g], i[g], o[g], m[g]}' | LC_ALL=C sort
done
