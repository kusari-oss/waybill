set -eu
apt-get update -qq >/dev/null && apt-get install -y -qq gcc liblzma-dev binutils >/dev/null 2>&1
V=$(dpkg-query -W -f '${Version}' liblzma-dev); echo "liblzma-dev $V"
cd /w
for p in a b; do
  gcc -O2 -o $p.static $p.c -static -llzma
  cp $p.static $p.stripped && strip $p.stripped
  gcc -O2 -o $p.shared $p.c -llzma
done
ver=5.4.5
for f in a.static a.stripped a.shared b.static b.stripped b.shared; do
  dyn=$(readelf -W --dyn-syms $f 2>/dev/null | awk '$8 ~ /^lzma_version_(string|number)$/ && $7 != "UND"' | wc -l)
  sym=$(readelf -W --syms $f 2>/dev/null | awk '$8 ~ /^lzma_version_(string|number)$/ && $7 != "UND"' | wc -l)
  vs=$(grep -c -a -F -x "$ver" <(tr '\0' '\n' < $f) || true)
  lz=$(strings -a $f | grep -c 'lzma_' || true)
  echo "$f dynsym_def=$dyn symtab_def=$sym whole_string_$ver=$vs lzma_text_lines=$lz"
done
