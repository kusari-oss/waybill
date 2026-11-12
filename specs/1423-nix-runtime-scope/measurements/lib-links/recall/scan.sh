#!/bin/bash
# #1455 lib/links rule — scanner findings on a runtime view, before and after.
#
#   scan.sh <waybill binary> <cdx document> <outdir>
#
# Derives the runtime-only view with the waybill binary itself
# (`sbom edit --drop 'scope=excluded'`), then runs grype 0.120.0 against the
# pinned DB (no update) and osv-scanner 2.7.0 on it, the invocations
# specs/1283-nix-allow-ifd/measurements/recall/scan.sh uses.
set -u
bin=$1; doc=$2; out=$3
GRYPE=${GRYPE:-/nix/store/7r24kzxg8in0g0vydh63z6ybrm5p7cmy-grype-0.120.0/bin/grype}
export GRYPE_DB_AUTO_UPDATE=false
mkdir -p "$out"
"$bin" sbom edit "$doc" --drop 'scope=excluded' -o "$out/runtime.cdx.json" 2> "$out/edit.err"
"$GRYPE" version > "$out/grype-version.txt" 2>&1
"$GRYPE" db status > "$out/grype-db.txt" 2>&1
"$GRYPE" "sbom:$out/runtime.cdx.json" -o json --file "$out/grype.json" > /dev/null 2> "$out/grype.err"
echo "grype exit=$?"
osv-scanner --version > "$out/osv-version.txt" 2>&1
# osv-scanner exits 1 when it finds vulnerabilities; >1 is an error.
osv-scanner scan source -L "$out/runtime.cdx.json" --format json > "$out/osv.json" 2> "$out/osv.err"
echo "osv exit=$?"
