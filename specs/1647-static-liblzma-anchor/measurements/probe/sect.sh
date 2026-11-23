set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null; apt-get install -y -qq --no-install-recommends binutils 7zip libarchive13t64 >/dev/null 2>&1
for spec in "/usr/lib/7zip/7zr 13.2.0" "/usr/lib/x86_64-linux-gnu/libarchive.so.13.7.2 5.4.5" "/usr/lib/x86_64-linux-gnu/libarchive.so.13.7.2 1.5.5"; do
  set -- $spec; f=$1; n=$2
  for s in .rodata .data.rel.ro .comment; do
    c=$(readelf -W -p $s "$f" 2>/dev/null | grep -c -E "\]  ${n//./\\.}$" || true)
    [ "$c" = 0 ] || echo "$(basename $f) $n in $s: $c"
  done
  readelf -W -p .rodata "$f" 2>/dev/null | grep -B1 -A1 -E "\]  ${n//./\\.}$" | cut -c1-100 || true
done
