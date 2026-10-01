#!/usr/bin/env bash
# Measure Nix build-closure vs runtime-closure size and identity for one package.
#
# Issue #1034 — "how deep should waybill follow a Nix closure?". The issue
# measured the BUILD closure only, and noted the runtime closure "was not
# measured here (it needs realisation)". It does not: `nix path-info -r
# --store https://cache.nixos.org` reads the closure out of binary-cache
# narinfo metadata without downloading or building anything.
#
# Usage:
#   ./probe-closure-depth.sh <nixpkgs-rev> <attr> [system]
#   ./probe-closure-depth.sh cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73 \
#       haskellPackages.haskell-language-server
#   ./probe-closure-depth.sh cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73 \
#       haskellPackages.haskell-language-server x86_64-linux
#
# A [system] argument probes another platform WITHOUT a machine of that
# platform: cross-system evaluation needs no builder, and `nix path-info
# --store https://cache.nixos.org` is metadata over HTTP. The x86_64-linux
# figures in results-*.md were taken this way from an aarch64-darwin host.
#
# Evaluation only. Nothing is built. The evaluated code is nixpkgs at the
# pinned revision, which is third-party; see the --nix-eval flag docs for the
# same caveat.
set -euo pipefail

REV="${1:?nixpkgs revision (40-char hex)}"
ATTR="${2:?attribute path, e.g. haskellPackages.haskell-language-server}"
SYSTEM="${3:-}"
OUTDIR="${OUTDIR:-$(mktemp -d)}"
# A caller-supplied OUTDIR is not created by mktemp, and every write below
# then fails. Previously those failures were invisible: `|| true` on the
# build step swallowed them and the script still exited 0.
mkdir -p "${OUTDIR}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NIX_CONFIG="experimental-features = nix-command flakes"

if [ -n "${SYSTEM}" ]; then
    # legacyPackages.<system> rather than the bare attr, so the evaluation
    # targets another platform instead of the host's.
    FLAKE="github:NixOS/nixpkgs/${REV}#legacyPackages.${SYSTEM}.${ATTR}"
    SYSLABEL="${SYSTEM} (cross-evaluated from $(nix eval --raw --impure --expr 'builtins.currentSystem'))"
else
    FLAKE="github:NixOS/nixpkgs/${REV}#${ATTR}"
    SYSLABEL="$(nix eval --raw --impure --expr 'builtins.currentSystem')"
fi
echo "flake   : ${FLAKE}"
echo "system  : ${SYSLABEL}"
echo "nix     : $(nix --version)"
echo "outdir  : ${OUTDIR}"
echo

echo "--- build closure (evaluation only) ---"
# No `|| true` here: a failed evaluation must fail the probe rather than
# fall through to classifying a file that was never written.
/usr/bin/time -p nix derivation show -r --no-write-lock-file "${FLAKE}" \
    > "${OUTDIR}/build.json" 2> "${OUTDIR}/build.time"
if [ ! -s "${OUTDIR}/build.json" ]; then
    echo "  evaluation produced no output; refusing to report numbers" >&2
    exit 1
fi
tail -3 "${OUTDIR}/build.time" | sed 's/^/  /'
python3 "${HERE}/classify-closure.py" build "${OUTDIR}/build.json"
echo

echo "--- runtime closure (binary-cache metadata, nothing built) ---"
OUT="$(nix eval --raw --no-write-lock-file "${FLAKE}.outPath")"
echo "  outPath: ${OUT}"
if nix path-info -r --store https://cache.nixos.org "${OUT}" \
        > "${OUTDIR}/runtime.txt" 2>/dev/null; then
    python3 "${HERE}/classify-closure.py" runtime "${OUTDIR}/runtime.txt"
else
    echo "  NOT substitutable from cache.nixos.org — the runtime closure cannot"
    echo "  be read without realising the package. Report it as unmeasured"
    echo "  rather than substituting the build closure for it."
fi
echo
echo "raw outputs kept in ${OUTDIR}"
