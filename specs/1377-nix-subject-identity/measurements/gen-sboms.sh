#!/usr/bin/env bash
# Spike: generate sbomnix and waybill SBOMs for each wrapper-flake target.
# Usage: gen-sboms.sh sbomnix|waybill
set -uo pipefail
W=/Users/mlieberman/Projects/mikebom/target/nix-vuln-spike
SBOMNIX=/nix/store/cbh4k59d3sa3w9axcwdbk6jzkg99ybvk-python3.14-sbomnix-1.9.0+g95480c2/bin/sbomnix
WAYBILL=/Users/mlieberman/Projects/mikebom/target/debug/waybill
TARGETS="curl openssh httpie gh ripgrep moat"
mode="${1:?sbomnix|waybill}"

for t in $TARGETS; do
  d="$W/targets/$t"; o="$W/out/$t"; mkdir -p "$o"
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
