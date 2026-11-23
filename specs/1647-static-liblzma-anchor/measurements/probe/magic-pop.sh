set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends binutils file libarchive-tools busybox-static kmod systemd python3 7zip zstd rpm squashfs-tools xz-utils libarchive13t64 >/dev/null 2>&1
echo "packages: $(dpkg-query -W -f '${Package}=${Version} ' libarchive13t64 busybox-static kmod systemd 7zip rpm squashfs-tools xz-utils liblzma5)"
total=0; hits=0
printf '\xfd7zXZ\x00' > /tmp/m
for f in $(find / -xdev -type f \( -perm -u+x -o -name '*.so*' \) 2>/dev/null); do
  head -c 4 "$f" 2>/dev/null | grep -q $'\x7fELF' || continue
  total=$((total+1))
  grep -q -a -F -f /tmp/m "$f" 2>/dev/null || continue
  hits=$((hits+1))
  def=$(readelf -W --dyn-syms "$f" 2>/dev/null | awk '$8 ~ /^lzma_version_(string|number)/ && $7 != "UND"' | wc -l)
  und=$(readelf -W --dyn-syms "$f" 2>/dev/null | awk '$8 ~ /^lzma_/ && $7 == "UND"' | wc -l)
  echo "MAGIC $f defines_anchor=$def imports_lzma=$und"
done
echo "elf_files=$total with_magic=$hits"
