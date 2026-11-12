#!/usr/bin/env bash
# #1430, after the fix: (1) does grype see the same findings from waybill's
# SPDX 2.3 document as from its CycloneDX one, and (2) does waybill write the
# patch-derived OpenVEX sidecar (milestones 1035 / 1050) for these targets,
# which `grype --vex` could use to suppress CVEs nixpkgs patches?
#
# Scans each target once in CycloneDX and SPDX 2.3 (with an explicit OpenVEX
# path), with the same flags as 1377's gen-sboms.sh. When the sidecar exists,
# grype is also run with `--vex`.
# Usage: TARGETS_DIR=... OUT_DIR=<scratch> WAYBILL=<binary> GRYPE=<grype> vex-spdx-probe.sh
set -uo pipefail
TARGETS_DIR="${TARGETS_DIR:?set TARGETS_DIR}"
W="${OUT_DIR:?set OUT_DIR}"
WAYBILL="${WAYBILL:?set WAYBILL}"
GRYPE="${GRYPE:?set GRYPE}"
export GRYPE_DB_AUTO_UPDATE=false
tsv() {
  jq -r '.matches[] | [.vulnerability.id, .artifact.name, .artifact.version] | @tsv' "$1" | LC_ALL=C sort -u
}
printf 'target\twaybill_exit\tcdx_grype\tspdx23_grype\tcdx_vs_spdx23_diff\topenvex_written\tcdx_vex_grype\n'
for t in curl openssh httpie gh ripgrep moat; do
  o="$W/$t"; mkdir -p "$o"; rm -f "$o/waybill.openvex.json"
  ( cd "$TARGETS_DIR/$t" && "$WAYBILL" sbom scan --no-deep-hash --nix-closure --no-clearly-defined \
      --path . --format cyclonedx-json,spdx-2.3-json \
      --output "cyclonedx-json=$o/waybill.cdx.json" \
      --output "spdx-2.3-json=$o/waybill.spdx.json" \
      --output "openvex=$o/waybill.openvex.json" >"$o/waybill.log" 2>&1 )
  we=$?
  "$GRYPE" "sbom:$o/waybill.cdx.json" -o json --file "$o/cdx.grype.json" >/dev/null 2>"$o/cdx.grype.err" \
    || echo "$t: grype on CycloneDX failed" >&2
  "$GRYPE" "sbom:$o/waybill.spdx.json" -o json --file "$o/spdx.grype.json" >/dev/null 2>"$o/spdx.grype.err" \
    || echo "$t: grype on SPDX 2.3 failed" >&2
  tsv "$o/cdx.grype.json" > "$o/cdx.grype.tsv"
  tsv "$o/spdx.grype.json" > "$o/spdx.grype.tsv"
  d=$(LC_ALL=C comm -3 "$o/cdx.grype.tsv" "$o/spdx.grype.tsv" | wc -l | tr -d ' ')
  vex=no; vg=-
  if [ -s "$o/waybill.openvex.json" ]; then
    vex=yes
    "$GRYPE" "sbom:$o/waybill.cdx.json" --vex "$o/waybill.openvex.json" -o json \
      --file "$o/cdx-vex.grype.json" >/dev/null 2>"$o/cdx-vex.grype.err"
    tsv "$o/cdx-vex.grype.json" > "$o/cdx-vex.grype.tsv"
    vg=$(wc -l < "$o/cdx-vex.grype.tsv" | tr -d ' ')
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$t" "$we" "$(wc -l < "$o/cdx.grype.tsv" | tr -d ' ')" \
    "$(wc -l < "$o/spdx.grype.tsv" | tr -d ' ')" "$d" "$vex" "$vg"
done
