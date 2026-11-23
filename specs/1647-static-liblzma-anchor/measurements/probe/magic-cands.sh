set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends binutils libarchive-tools busybox-static kmod systemd python3 7zip rpm squashfs-tools xz-utils libarchive13t64 libxml2 libmagic1t64 libdw1t64 tar >/dev/null 2>&1
for f in /usr/bin/tar /usr/bin/kmod /usr/bin/busybox /usr/bin/xz /usr/lib/x86_64-linux-gnu/liblzma.so.5.4.5 /usr/lib/x86_64-linux-gnu/libarchive.so.13.7.2 /usr/lib/x86_64-linux-gnu/libkmod.so.2.4.1 /usr/lib/x86_64-linux-gnu/libxml2.so.2.9.14 /usr/lib/x86_64-linux-gnu/libmagic.so.1.0.0 /usr/lib/x86_64-linux-gnu/libdw-0.190.so /usr/lib/7zip/7zr /usr/lib/7zip/7z.so /usr/lib/7zip/7za; do
  c=$(strings -a -n 5 "$f" | grep -E -x '[0-9]+\.[0-9]+\.[0-9]+(alpha|beta)?' | sort -u | tr '\n' ' ')
  echo "$(basename $f): [${c}]"
done
