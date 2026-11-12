#!/usr/bin/env bash
# Reproduce the 2026-11-12 waybill-vs-sbomnix comparison (docs/audits/2026-11-12-nix-vs-sbomnix.md).
# Evaluates/builds ONLY: nixpkgs (the wrapper flakes and the tool pins) and sbomnix's own flake at the
# pinned v1.9.0 commit (nixpkgs' sbomnix 1.9.0 fails to build: sqliteschema wants typepy<2).
#
# Usage: WAYBILL=<waybill binary> GRYPE_DB_TARBALL=<vulnerability-db_v6.1.10_2026-11-08T00:22:04Z_1794118797.tar.zst>
#        RECALL_RUN=<recall-truth.yml run id with sbomnix artifacts, e.g. 43107760056> OUT=$(mktemp -d) ./run.sh
set -euo pipefail
: "${WAYBILL:?}" "${GRYPE_DB_TARBALL:?}" "${RECALL_RUN:?}" "${OUT:?}"
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(cd "$HERE/../../.." && pwd)
CLASSIFY="$ROOT/specs/1283-nix-allow-ifd/measurements/recall/classify.py"
GRYPE=$(nix build --no-link --print-out-paths "github:NixOS/nixpkgs/0bb2eedc20e4739f29e8045034f2828ad7054f8f#grype")/bin/grype
SBOMNIX=$(nix build --no-link --print-out-paths --option accept-flake-config false "github:tiiuae/sbomnix/95480c2168f0156eea4a3aac34d2bbb964aa3b4b")/bin/sbomnix
export GRYPE_DB_CACHE_DIR="$OUT/grype-db" GRYPE_DB_AUTO_UPDATE=false GRYPE_DB_VALIDATE_AGE=false GRYPE_CHECK_FOR_APP_UPDATE=false
"$GRYPE" db import "$GRYPE_DB_TARBALL"
RUNTIME_VIEW='def strip: if type=="object" then (if has("components") then .components |= map(select(.scope != "excluded") | strip) else . end) else . end; strip'
# OpenVEX built from a document's own CycloneDX vulnerabilities[] (FR-006 of #1436 makes the two carriers equal).
VEX_FROM_CDX='{"@context":"https://openvex.dev/ns/v0.2.0","@id":"https://example.invalid/vs-sbomnix","author":"derived from cdx vulnerabilities[]","timestamp":"2026-11-12T00:00:00Z","version":1,"statements":[ . as $d | ([$d|..|objects|select(has("bom-ref") and has("purl"))]|map({key:.["bom-ref"],value:.purl})|from_entries) as $p | .vulnerabilities[]? | select(.analysis.state=="resolved") | .id as $v | .affects[] | {"vulnerability":{"name":$v},"products":[{"@id":$p[.ref]}],"status":"fixed"}]}'

# (1) Haskell closure corpus targets: sbomnix side from the CI recall-truth artifacts; waybill side = committed goldens.
for t in nix-closure-slack-web nix-closure-openai nix-closure-slacklinker; do
  gh run download "$RECALL_RUN" -n "recall-truth-$t" -D "$OUT/ci/$t"
  o="$OUT/w/$t"; mkdir -p "$o"; g="$ROOT/waybill-cli/tests/fixtures/public_corpus/$t/cdx.json"
  jq "$RUNTIME_VIEW" "$g" > "$o/wr.cdx.json"; jq "$VEX_FROM_CDX" "$g" > "$o/waybill.openvex.json"
  "$GRYPE" "sbom:$o/wr.cdx.json" -o json > "$o/wr.grype.json"
  "$GRYPE" "sbom:$o/wr.cdx.json" --vex "$o/waybill.openvex.json" -o json > "$o/wr.vex.grype.json"
  osv-scanner scan source -L "$o/wr.cdx.json" --format json > "$o/wr.osv.json" || true
  cp "$OUT/ci/$t/S.cdx.json" "$o/S.cdx.json"; osv-scanner scan source -L "$o/S.cdx.json" --format json > "$o/S.osv.json" || true
done
python3 -I -B "$HERE/haskell-compare.py" "$CLASSIFY" "$OUT" "$OUT/ci" "$ROOT/waybill-cli/tests/fixtures/public_corpus" \
  nix-closure-slack-web nix-closure-openai nix-closure-slacklinker

# (2) nixpkgs wrapper flakes (nixpkgs a799d3e3, aarch64-darwin as measured).
"$HERE/scan-wrappers.sh" "$WAYBILL" "$HERE/wrappers" "$OUT/wrap/waybill"
for t in curl openssh httpie gh ripgrep; do
  s="$OUT/wrap/sbomnix/$t"; w="$OUT/wrap/waybill/$t"; mkdir -p "$s"; ref="path:$HERE/wrappers/$t#default"
  nix build --no-link --option accept-flake-config false "$ref"
  (cd "$s" && "$SBOMNIX" --cdx runtime.cdx.json "$ref" && "$SBOMNIX" --buildtime --cdx buildtime.cdx.json "$ref")
  jq "$RUNTIME_VIEW" "$w/cdx.json" > "$w/wr.cdx.json"; cp "$w/cdx.json" "$w/full.cdx.json"
  for d in full wr; do
    "$GRYPE" "sbom:$w/$d.cdx.json" -o json > "$w/$d.grype.json"
    "$GRYPE" "sbom:$w/$d.cdx.json" --vex "$w/waybill.openvex.json" -o json > "$w/$d.vex.grype.json"
    osv-scanner scan source -L "$w/$d.cdx.json" --format json > "$w/$d.osv.json" || true
  done
  for d in runtime buildtime; do
    "$GRYPE" "sbom:$s/$d.cdx.json" -o json > "$s/$d.grype.json"
    osv-scanner scan source -L "$s/$d.cdx.json" --format json > "$s/$d.osv.json" || true
  done
done
python3 -I -B "$HERE/wrapper-causes.py" "$CLASSIFY" "$OUT/wrap" curl openssh httpie gh ripgrep
python3 -I -B "$HERE/wrapper-compare.py" "$CLASSIFY" "$OUT/wrap" curl openssh httpie gh ripgrep
