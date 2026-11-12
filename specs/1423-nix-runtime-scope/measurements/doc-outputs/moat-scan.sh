#!/bin/bash
# #1455: scan moat with one waybill binary the way the corpus harness does,
# apply the runtime-only recipe, and count the test-framework packages each
# runtime view keeps.
#
#   moat-scan.sh <waybill binary> <moat checkout> <outdir>
#
# The output must already be in the local store (nix path-info succeeds):
# nothing is built. Test frameworks are matched by package name against
# FRAMEWORKS below.
set -eu
bin=$1; repo=$2; out=$3
mkdir -p "$out"
FRAMEWORKS='^(hspec|hspec-core|hspec-discover|hspec-expectations|hspec-golden|hspec-meta|tasty|tasty-[a-z-]+|QuickCheck|quickcheck-[a-z-]+|test-framework|test-framework-[a-z0-9-]+|HUnit|doctest|ChasingBottoms|smallcheck)$'
"$bin" sbom scan --nix-closure --nix-closure-attr default --no-deps-dev --no-clearly-defined \
  --path "$repo" --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output "cyclonedx-json=$out/moat.cdx.json" \
  --output "spdx-2.3-json=$out/moat.spdx.json" \
  --output "spdx-3-json=$out/moat.spdx3.json" \
  --root-name nix-closure-moat --root-version d069055 2> "$out/scan.log"
for f in cdx spdx spdx3; do
  "$bin" sbom edit "$out/moat.$f.json" --drop 'scope=excluded' -o "$out/runtime.$f.json" 2>> "$out/edit.log"
done
jq -r '.metadata.properties[] | select(.name == "waybill:closure-scope-method") | .value' "$out/moat.cdx.json" > "$out/c216.json"
names_cdx() { jq -r '[.. | objects | select(has("purl") and has("name")) | .name] | .[]' "$1" | sort -u; }
names_spdx() { jq -r '.packages[].name' "$1" | sort -u; }
names_spdx3() { jq -r '."@graph"[] | select(.type == "software_Package") | .name' "$1" | sort -u; }
{
  echo "test frameworks, full document / runtime view:"
  echo "cdx    $(names_cdx "$out/moat.cdx.json" | grep -cE "$FRAMEWORKS") / $(names_cdx "$out/runtime.cdx.json" | grep -cE "$FRAMEWORKS")"
  echo "spdx   $(names_spdx "$out/moat.spdx.json" | grep -cE "$FRAMEWORKS") / $(names_spdx "$out/runtime.spdx.json" | grep -cE "$FRAMEWORKS")"
  echo "spdx3  $(names_spdx3 "$out/moat.spdx3.json" | grep -cE "$FRAMEWORKS") / $(names_spdx3 "$out/runtime.spdx3.json" | grep -cE "$FRAMEWORKS")"
  echo "kept in the SPDX 2.3 runtime view:"
  names_spdx "$out/runtime.spdx.json" | grep -E "$FRAMEWORKS" | sed 's/^/  /'
  echo "packages, full / runtime view (spdx): $(names_spdx "$out/moat.spdx.json" | wc -l | tr -d ' ') / $(names_spdx "$out/runtime.spdx.json" | wc -l | tr -d ' ')"
  echo "C216: $(cat "$out/c216.json")"
} > "$out/summary.txt"
cat "$out/summary.txt"
