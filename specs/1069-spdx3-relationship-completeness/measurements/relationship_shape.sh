#!/usr/bin/env bash
# #878 / m1069: the shape of SPDX 3 dependsOn relationships in committed
# public-corpus goldens. `completeness` describes whether a relationship's
# `to` list is exhaustive, so it only means something per (from, type) if
# that set is in one relationship.
# Usage: relationship_shape.sh [corpus-goldens-dir]
D=${1:-waybill-cli/tests/fixtures/public_corpus}
for f in "$D"/*/spdx-3.json; do
  t=$(basename "$(dirname "$f")")
  jq -r --arg t "$t" '
    [.["@graph"][] | select((.type=="Relationship" or .type=="LifecycleScopedRelationship")
                            and .relationshipType=="dependsOn")] as $r
    | ($r | group_by(.from) | map(length)) as $g
    | "\($t): dependsOn=\($r|length) froms=\($g|length) froms-with-several=\([$g[]|select(.>1)]|length) max-to-len=\(([$r[].to|length]|max) // 0) with-completeness=\([$r[]|select(has("completeness"))]|length)"' "$f"
done
