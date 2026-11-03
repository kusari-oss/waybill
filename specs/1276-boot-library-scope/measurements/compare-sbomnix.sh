#!/usr/bin/env bash
# Compare waybill's Nix closure SBOM against sbomnix's, on one derivation.
#
# Why this exists: milestone 1275's spec assumed "sbomnix emits 8,230 edges on
# the same project by treating store references as dependencies", inherited
# from #1282 rather than re-measured. The project's standard is that a number
# describing external behaviour must be traceable to an observation. This is
# that observation, and it also answers a question #1340 raised implicitly:
# what does the other tool do about boot libraries?
#
# Usage:
#   SBOMNIX=<path to sbomnix> PROJ=<flake dir> WAYBILL=<binary> ./compare-sbomnix.sh
#
# `--buildtime` and a FLAKEREF target are both load-bearing. Given a bare
# store path sbomnix logs "No nixpkgs metadata source is available for
# store-path targets" and emits zero licenses -- a limitation of the
# invocation, not of the tool, and reporting it as the latter would be a
# measurement error.
set -euo pipefail
SBOMNIX="${SBOMNIX:?set SBOMNIX to the sbomnix binary}"
PROJ="${PROJ:?set PROJ to a flake directory}"
WAYBILL="${WAYBILL:-./target/debug/waybill}"
# Absolutise before any `cd`, or a relative default resolves against the
# project directory and vanishes.
WAYBILL="$(cd "$(dirname "$WAYBILL")" && pwd)/$(basename "$WAYBILL")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "sbomnix: $("$SBOMNIX" --version)"
echo "project: $PROJ"

( cd "$work" && "$SBOMNIX" --buildtime --cdx sbomnix.cdx.json "$PROJ#default" >/dev/null 2>&1 )
( cd "$PROJ" && "$WAYBILL" \
    sbom scan --no-deep-hash --nix-closure --no-deps-dev --no-clearly-defined \
    --path . --format cyclonedx-json --output "cyclonedx-json=$work/waybill.cdx.json" >/dev/null 2>&1 )

python3 -I - "$work/sbomnix.cdx.json" "$work/waybill.cdx.json" <<'PY'
import json, sys, collections
def load(p): return json.load(open(p))
def stats(d):
    comps = d.get("components", [])
    return dict(
        components=len(comps),
        edges=sum(len(x.get("dependsOn", [])) for x in d.get("dependencies", [])),
        licensed=sum(1 for c in comps if c.get("licenses")),
        hashed=sum(1 for c in comps if c.get("hashes")),
        no_purl=sum(1 for c in comps if not c.get("purl")),
        purl_types=dict(collections.Counter(
            (c.get("purl") or "(none)").split("/")[0] for c in comps).most_common(5)),
    )
a, b = load(sys.argv[1]), load(sys.argv[2])
sa, sb = stats(a), stats(b)
print(f"{'':16}{'sbomnix':>12}{'waybill':>12}")
for k in ("components", "edges", "licensed", "hashed", "no_purl"):
    print(f"{k:16}{sa[k]:>12}{sb[k]:>12}")
print(f"\nsbomnix purl types: {sa['purl_types']}")
print(f"waybill purl types: {sb['purl_types']}")
names_a = {c.get("name") for c in a.get("components", [])}
names_b = {c.get("name") for c in b.get("components", [])}
print(f"\nnames: sbomnix {len(names_a)}, waybill {len(names_b)}, shared {len(names_a & names_b)}")
boot = ["pretty", "directory", "ghc-heap", "array", "binary", "deepseq"]
print("GHC boot libraries, by tool:")
for n in boot:
    print(f"  {n:12} sbomnix={'yes' if n in names_a else 'NO '}   waybill={'yes' if n in names_b else 'NO '}")
PY
