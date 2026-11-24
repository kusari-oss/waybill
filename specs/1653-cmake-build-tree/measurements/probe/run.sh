#!/usr/bin/env bash
# Spec 1653 SC-001(a): capture real CMake build trees for the probe project.
# Usage: run.sh <output-dir>
# Needs Docker. The container fetches apt and PyPI packages; the builds
# themselves use only local sources (see inner.sh).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?output dir}
mkdir -p "$out"
image=debian:bookworm-slim
docker pull -q "$image" >/dev/null
docker image inspect --format '{{index .RepoDigests 0}}' "$image" > "$out/image.txt"
docker run --rm -v "$here:/probe:ro" -v "$(cd "$out" && pwd):/out" "$image" bash -c '
  set -euo pipefail
  apt-get update -qq >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    cmake ninja-build make g++ git ca-certificates python3-venv >/dev/null
  python3 -m venv /opt/cm331 && /opt/cm331/bin/pip install -q cmake==3.31.6
  python3 -m venv /opt/cm40  && /opt/cm40/bin/pip install -q cmake==4.0.3
  bash /probe/inner.sh
'
