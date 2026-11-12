#!/bin/bash
# #1455 (second rule): what a darwin Haskell output's `lib/links` folder holds,
# and what the Mach-O files in that output actually load.
#
#   probe.sh <outdir> <store path>...
#
# Writes, per store path:
#   <name>.tree         every file and symlink in the output (type, path, link target)
#   <name>.macho        each Mach-O file in the output (by `file`)
#   <name>.otool        `otool -L` of each Mach-O file (LC_LOAD_DYLIB and LC_ID_DYLIB)
#   <name>.links        each lib/links entry and the store path it points into
# Reads only the local store. Nothing is built or substituted.
set -eu
out=$1; shift
mkdir -p "$out"
for p in "$@"; do
  n=$(basename "$p")
  ( cd "$p" && find . -mindepth 1 \( -type f -printf 'f %p\n' -o -type l -printf 'l %p -> %l\n' -o -type d -printf 'd %p\n' \) 2>/dev/null \
      || find . -mindepth 1 | while IFS= read -r f; do
           if [ -L "$f" ]; then echo "l $f -> $(readlink "$f")"; elif [ -d "$f" ]; then echo "d $f"; else echo "f $f"; fi
         done ) | LC_ALL=C sort > "$out/$n.tree"
  : > "$out/$n.macho"
  : > "$out/$n.otool"
  while IFS= read -r f; do
    if file -b "$p/$f" | grep -q 'Mach-O'; then
      echo "$f" >> "$out/$n.macho"
      { echo "== $f"; otool -L "$p/$f" | tail -n +2; } >> "$out/$n.otool"
    fi
  done < <(cd "$p" && find . -type f | LC_ALL=C sort)
  if [ -d "$p/lib/links" ]; then
    for l in "$p"/lib/links/*; do
      echo "$(basename "$l") -> $(readlink "$l")"
    done | LC_ALL=C sort > "$out/$n.links"
  else
    : > "$out/$n.links"
  fi
  echo "$n: files=$(grep -c '^f ' "$out/$n.tree") symlinks=$(grep -c '^l ' "$out/$n.tree") macho=$(grep -c . "$out/$n.macho") lib/links=$(grep -c . "$out/$n.links")"
done
