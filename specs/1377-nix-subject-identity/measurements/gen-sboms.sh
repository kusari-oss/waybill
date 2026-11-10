#!/usr/bin/env bash
# Spike: generate sbomnix and waybill SBOMs for each wrapper-flake target.
# Usage: gen-sboms.sh sbomnix|waybill
set -uo pipefail
TARGETS_DIR="${TARGETS_DIR:?set TARGETS_DIR (see quickstart.md)}"
OUT_DIR="${OUT_DIR:?set OUT_DIR (see quickstart.md)}"
TARGETS="curl openssh httpie gh ripgrep moat"
mode="${1:?sbomnix|waybill}"
case "$mode" in
  sbomnix) SBOMNIX="${SBOMNIX:?set SBOMNIX}";;
  waybill) WAYBILL="${WAYBILL:?set WAYBILL}";;
esac

for t in $TARGETS; do
  d="$TARGETS_DIR/$t"; o="$OUT_DIR/$t"; mkdir -p "$o"
  case "$mode" in
    sbomnix)
      s=$(date +%s)
      ( cd "$o" && "$SBOMNIX" --buildtime --cdx sbomnix-bt.cdx.json "$d#default" >sbomnix-bt.log 2>&1 )
      echo "$t sbomnix-bt exit=$? $(( $(date +%s)-s ))s"
      if [ "$t" != moat ]; then
        s=$(date +%s)
        ( cd "$o" && "$SBOMNIX" --cdx sbomnix-rt.cdx.json "$d#default" >sbomnix-rt.log 2>&1 )
        echo "$t sbomnix-rt exit=$? $(( $(date +%s)-s ))s"
      fi
      ;;
    waybill)
      s=$(date +%s)
      # deps.dev confirmation left ON: it is what produces ecosystem PURLs
      # (m1366). compare-sbomnix.sh runs --no-deps-dev and so hides them.
      ( cd "$d" && "$WAYBILL" sbom scan --no-deep-hash --nix-closure --no-clearly-defined \
          --path . --format cyclonedx-json \
          --output "cyclonedx-json=$o/waybill.cdx.json" >"$o/waybill.log" 2>&1 )
      echo "$t waybill exit=$? $(( $(date +%s)-s ))s"
      ;;
  esac
done
