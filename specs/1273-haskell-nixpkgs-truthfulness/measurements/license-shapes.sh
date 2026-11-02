#!/usr/bin/env bash
# Milestone 1273 Phase 0 — Probe B.
#
# How does the cached nixpkgs Haskell package set declare a license?
#
# US3 / FR-010 says a component resolved from that set "MUST carry the
# license that set declares for it", and FR-011 says a declaration that
# cannot be canonicalised must be preserved rather than dropped. Both
# requirements depend on the SHAPE of the declaration, which is a property
# of nixpkgs, so it is measured here rather than assumed.
#
# Reads only `~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix`, already
# fetched by a previous scan, so this is offline and needs no `nix`.
#
# Usage:  NIXPKGS_REV=<rev> ./license-shapes.sh
set -euo pipefail

REV="${NIXPKGS_REV:-a799d3e3886da994fa307f817a6bc705ae538eeb}"
SET="${WAYBILL_NIXPKGS_CACHE:-$HOME/.cache/waybill/nixpkgs}/$REV/hackage-packages.nix"
[ -r "$SET" ] || { echo "not cached: $SET" >&2; exit 1; }

echo "revision: $REV"
echo "file:     $SET ($(wc -c < "$SET" | tr -d ' ') bytes)"
echo

echo "=== denominators ==="
printf '  %-42s %s\n' "'= callPackage' attribute blocks" \
  "$(grep -cE '^  ("[^"]+"|[A-Za-z0-9][A-Za-z0-9_.'"'"'-]*)[[:space:]]*=[[:space:]]*callPackage' "$SET")"
printf '  %-42s %s\n' "'license = ' declarations" \
  "$(grep -cE '^[[:space:]]*license = ' "$SET")"
printf '  %-42s %s\n' "of those, single-line (ending in ';')" \
  "$(grep -coE 'license = [^;]*;' "$SET")"
# A multi-line or list-valued form would need a different parser. Asserted
# rather than assumed: a zero here is what makes a line-oriented read sound.
printf '  %-42s %s\n' "multi-line forms (no ';' on the line)" \
  "$(grep -cE '^[[:space:]]*license = [^;]*$' "$SET" || true)"
printf '  %-42s %s\n' "'with lib.licenses; [ ... ]' list forms" \
  "$(grep -cE 'license = with lib' "$SET" || true)"
printf '  %-42s %s\n' "bare list forms 'license = [ ... ]'" \
  "$(grep -cE 'license = \[' "$SET" || true)"
echo

echo "=== the three families (must sum to the declaration count) ==="
printf '  %-42s %s\n' 'lib.licenses.<attr>' \
  "$(grep -cE 'license = lib\.licenses\.' "$SET")"
printf '  %-42s %s\n' 'lib.meta.getLicenseFromSpdxId "<id>"' \
  "$(grep -cE 'license = lib\.meta\.getLicenseFromSpdxId ' "$SET")"
printf '  %-42s %s\n' 'a bare string' \
  "$(grep -cE 'license = "' "$SET")"
echo

echo "=== family 1: the whole lib.licenses attribute vocabulary ==="
echo "    (the size of the mapping table FR-010 needs)"
grep -oE 'license = lib\.licenses\.[A-Za-z0-9_]+' "$SET" \
  | sed 's/.*licenses\.//' | sort | uniq -c | sort -rn | sed 's/^/  /'
echo

echo "=== family 2: distinct SPDX ids (pass-through, no mapping) ==="
grep -oE 'getLicenseFromSpdxId "[^"]*"' "$SET" \
  | sed 's/.*Id //' | sort -u | tr -d '"' | tr '\n' ' ' | fold -s -w 72 | sed 's/^/  /'
echo

echo "=== family 3: every bare-string value (FR-011's population) ==="
grep -oE 'license = "[^"]*"' "$SET" \
  | sed 's/license = //' | sort | uniq -c | sort -rn | sed 's/^/  /'
