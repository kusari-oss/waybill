#!/usr/bin/env bash
# Classify a release tag's channel (#1266).
#
# Usage:  scripts/release-channel.sh v0.13.0
# Prints: `stable` for a bare `vX.Y.Z` tag, `prerelease` for anything else
#         (`-nightly.`, `-alpha.`, `-rc.`, build metadata).
#
# WHY THIS IS A SCRIPT AND NOT AN INLINE REGEX. `release.yml` serves both
# channels: stable tags arrive by tag-push, nightlies by workflow_dispatch
# from `nightly.yml`. Two steps need the channel, and before this script
# only one of them tested for it — `Compute prerelease flag from tag` kept
# GitHub's "Latest release" badge off nightlies, while `Compute image tags`
# emitted `:latest` unconditionally from the tag name. So every 06:00Z
# nightly moved `ghcr.io/<owner>/waybill:latest` onto itself, and
# `docker pull ghcr.io/<owner>/waybill` served a nightly until the next
# stable published. Measured on 2026-10-23: the nightly container job
# pushed `:0.12.1-nightly.20261023`, `:v0.12.1-nightly.20261023` AND
# `:latest` at 06:37Z, and `:latest` stayed on that build until v0.13.0
# published at 18:55Z.
#
# A second copy of the predicate is how the first one drifted out of
# reach, so the channel test lives here, in one place both steps call,
# with mutation tests in `waybill-cli/tests/release_gates.rs`.
#
# The grammar is SemVer's: a prerelease suffix is `-...`, build metadata
# is `+...`, and `stable` means neither is present. Deliberately NOT
# `sort -V` or any ordering comparison — this is a shape test on a single
# tag, and `sort -V` disagrees with SemVer on prereleases anyway
# (see `nightly-baseline.sh`).
set -euo pipefail

if [ "$#" -ne 1 ] || [ -z "${1:-}" ]; then
  echo "usage: release-channel.sh <tag>" >&2
  exit 2
fi

tag="$1"

if [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "stable"
else
  echo "prerelease"
fi
