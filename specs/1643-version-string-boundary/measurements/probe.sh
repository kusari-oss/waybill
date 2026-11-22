#!/usr/bin/env bash
# Spec 1643 R1: re-verify #1568 and #1641 at a given waybill binary.
# Usage: probe.sh <waybill-binary> <libz.1.3.2.dylib> <libncursesw.6.dylib> <workdir>
# Copies each library, changes only the byte before the version literal,
# scans each copy alone three times, and prints the zlib/ncurses PURLs.
set -euo pipefail
WB=$1; LIBZ=$2; LIBNC=$3; WORK=$4
mkdir -p "$WORK"
mutate() { # lib literal-regex byte out
  python3 -I -B - "$1" "$2" "$3" "$4" <<'PY'
import re, sys
src, pat, byte, out = sys.argv[1], sys.argv[2].encode(), sys.argv[3], sys.argv[4]
d = bytearray(open(src, "rb").read())
ms = list(re.finditer(pat, bytes(d)))
assert len(ms) == 1, f"{len(ms)} matches of {pat!r}"
i = ms[0].start()
before = d[i - 1]
if byte != "keep":
    d[i - 1] = int(byte, 16)
open(out, "wb").write(d)
print(f"{out}: byte at {i-1:#x} {before:#04x} -> {d[i-1]:#04x}")
PY
}
for spec in "zlib|${LIBZ}|libz.1.3.2.dylib| deflate 1\\.3\\.2 Copyright" \
            "ncurses|${LIBNC}|libncursesw.6.dylib|ncurses \\d+\\.\\d+\\.\\d{8}\\x00"; do
  IFS='|' read -r name lib file pat <<<"$spec"
  for b in keep 2d ff 78; do
    d="$WORK/$name-$b"; mkdir -p "$d"
    mutate "$lib" "$pat" "$b" "$d/$file"
    for i in 1 2 3; do
      HOME="$WORK/home" "$WB" --offline sbom scan --path "$d" --no-deps-dev \
        --format cyclonedx-json --output "$d/out$i.cdx.json" 2>/dev/null
      r=$(jq -r --arg n "$name" '[.components[]? | recurse(.components[]?) | select(.name==$n) | .purl] | sort | join(",")' "$d/out$i.cdx.json")
      echo "$name byte=$b run$i: ${r:-<none>}"
    done
  done
done
