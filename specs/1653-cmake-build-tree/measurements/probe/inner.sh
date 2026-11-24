#!/usr/bin/env bash
# Spec 1653 SC-001(a): runs INSIDE the container started by run.sh.
# Builds the probe project under several CMake versions, generators and
# configurations, and keeps every build tree's metadata for analyze.py.
# No network access is used by any build: all dependency sources are local
# git repositories and tarballs generated below.
set -euo pipefail

SRC=/probe/project
OUT=/out
WORK=/work
DEPS=$WORK/deps
mkdir -p "$WORK" "$DEPS" "$OUT"

git config --global user.email probe@example.invalid
git config --global user.name probe
git config --global init.defaultBranch main
git config --global protocol.file.allow always

# --- dependency sources -----------------------------------------------------
for d in hdronly extra; do
  rm -rf "$DEPS/$d.src" && cp -r "$SRC/deps/$d" "$DEPS/$d.src"
  git -C "$DEPS/$d.src" init -q
  git -C "$DEPS/$d.src" add -A
  git -C "$DEPS/$d.src" commit -q -m init
  git clone -q --bare "$DEPS/$d.src" "$DEPS/$d.git"
done
git -C "$DEPS/hdronly.src" tag v1.2.3
git -C "$DEPS/hdronly.src" push -q "$DEPS/hdronly.git" v1.2.3
for d in urldep neverused extra bdep extproj mixedcase; do
  tar -C "$SRC/deps" -czf "$DEPS/$d.tar.gz" "$d"
done
URLDEP_SHA256=$(sha256sum "$DEPS/urldep.tar.gz" | cut -d' ' -f1)
EXTPROJ_SHA256=$(sha256sum "$DEPS/extproj.tar.gz" | cut -d' ' -f1)
BAD_SHA256=0000000000000000000000000000000000000000000000000000000000000000

# --- CMake versions ---------------------------------------------------------
declare -A CMAKES
CMAKES[apt]=/usr/bin/cmake
CMAKES[pip3.31]=/opt/cm331/bin/cmake
CMAKES[pip4.0]=/opt/cm40/bin/cmake

: > "$OUT/versions.txt"
for k in apt pip3.31 pip4.0; do
  echo "$k $(${CMAKES[$k]} --version | head -1)" >> "$OUT/versions.txt"
done
echo "ninja $(ninja --version)" >> "$OUT/versions.txt"
echo "make $(make --version | head -1)" >> "$OUT/versions.txt"
echo "cxx $(c++ --version | head -1)" >> "$OUT/versions.txt"

run() { # name, cmake, generator, configure-only?, extra args...
  local name=$1 cm=$2 gen=$3 build=$4; shift 4
  local b=$WORK/b/$name
  rm -rf "$b"; mkdir -p "$b"
  if [[ "$name" == *system* ]]; then
    # File-API query placed before configure (spec FR-010 / User Story 4).
    mkdir -p "$b/.cmake/api/v1/query"
    touch "$b/.cmake/api/v1/query/codemodel-v2" "$b/.cmake/api/v1/query/cache-v2" \
          "$b/.cmake/api/v1/query/toolchains-v1"
  fi
  local rc_c=0 rc_b=skipped
  "$cm" -S "$SRC/app" -B "$b" -G "$gen" -DDEPS="$DEPS" \
        -DURLDEP_SHA256="$URLDEP_SHA256" "$@" > "$b.configure.log" 2>&1 || rc_c=$?
  if [[ $build == yes && $rc_c == 0 ]]; then
    rc_b=0; "$cm" --build "$b" > "$b.build.log" 2>&1 || rc_b=$?
  fi
  echo "$name configure=$rc_c build=$rc_b" >> "$OUT/runs.txt"
  # Full listing: type, size, path relative to the build dir.
  (cd "$b" && find . -printf '%y\t%s\t%p\n' | sort -k3) > "$OUT/$name.listing.tsv"
  # Keep the metadata files, not objects or binaries.
  mkdir -p "$OUT/$name"
  (cd "$b" && find . \( -name CMakeCache.txt -o -name compile_commands.json \
        -o -path '*/.cmake/api/v1/reply/*' -o -path '*-stamp/*' \
        -o -path '*/_deps/*-subbuild/CMakeLists.txt' \) -type f -size -2M \
        -exec cp --parents {} "$OUT/$name/" \;)
  cp "$b.configure.log" "$OUT/$name/configure.log"
  [[ -f "$b.build.log" ]] && cp "$b.build.log" "$OUT/$name/build.log" || true
}

: > "$OUT/runs.txt"
for k in apt pip3.31 pip4.0; do
  cm=${CMAKES[$k]}
  # hdronly installed into a prefix, for the "found via find_package" case.
  rm -rf "$WORK/prefix-$k" "$WORK/hb-$k"
  "$cm" -S "$DEPS/hdronly.src" -B "$WORK/hb-$k" -DCMAKE_INSTALL_PREFIX="$WORK/prefix-$k" > /dev/null
  "$cm" --install "$WORK/hb-$k" > /dev/null
  for g in Ninja "Unix Makefiles"; do
    gk=$([[ $g == Ninja ]] && echo ninja || echo make)
    run "$k-$gk-fetch"     "$cm" "$g" yes -DEXTPROJ_SHA256="$EXTPROJ_SHA256"
    run "$k-$gk-system"    "$cm" "$g" yes -DEXTPROJ_SHA256="$EXTPROJ_SHA256" \
        -DCMAKE_PREFIX_PATH="$WORK/prefix-$k" -DWITH_EXTRA=ON -DBACKEND=B
    run "$k-$gk-confonly"  "$cm" "$g" no  -DEXTPROJ_SHA256="$EXTPROJ_SHA256"
    run "$k-$gk-failed"    "$cm" "$g" yes -DEXTPROJ_SHA256="$BAD_SHA256"
  done
done
echo done
