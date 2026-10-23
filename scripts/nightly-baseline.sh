#!/usr/bin/env bash
# Compute the nightly channel's version baseline from the repository's
# stable tags, read on stdin (#1263).
#
# Usage:  git tag -l 'v*' | scripts/nightly-baseline.sh
# Prints: the baseline X.Y.Z for today's nightly, e.g. `0.13.1`.
#
# WHY TAGS AND NOT `Cargo.toml`. The baseline used to come from
# `Cargo.toml`'s version, which made the nightly channel's ordering depend
# on `main` having been bumped off the version just released — a
# human-opened PR (RELEASING.md step 10). The window between tag-push and
# that PR merging is open on every release, and on 2026-10-23 it stayed
# open: `v0.12.1-nightly.20261023` published while `v0.12.1` was the
# latest stable, and SemVer orders a prerelease BEFORE its release, so a
# newer build sorted older. Nothing detected it.
#
# Deriving from the tags makes `v<released>-nightly.*` unmintable while
# that version is newest, because the baseline is always at least one
# patch ahead. The ordering is structural rather than asserted, which is
# the same property Rust gets by keeping master's version one release
# ahead of anything shipped.
#
# NOT `sort -V`. It disagrees with SemVer on prereleases —
# `printf 'v0.12.1\nv0.12.1-nightly.20261023\n' | sort -V` puts the
# prerelease last — so a comparison built on it would have called the
# inverted tag correct. Stable tags are filtered to bare `vX.Y.Z` first
# and then compared as numeric triples.
set -euo pipefail

# Bare `vX.Y.Z` only: no prerelease, no build metadata. That excludes
# every `-nightly.`, `-alpha.` and `-rc.` tag, which must not seed a
# baseline — a nightly built off a nightly would ratchet forever.
latest=$(grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
  | sed 's/^v//' \
  | sort -t. -k1,1n -k2,2n -k3,3n \
  | tail -1 || true)

if [ -z "$latest" ]; then
  # Fresh repository with no stable release yet. `0.0.1` is the lowest
  # baseline that still sorts after nothing, and the first real release
  # will exceed it.
  echo "0.0.1"
  exit 0
fi

IFS=. read -r major minor patch <<<"$latest"
baseline="${major}.${minor}.$((patch + 1))"

# Assert the result, rather than trusting the arithmetic above: a later
# refactor must not be able to reintroduce the inversion quietly. The
# baseline has to be STRICTLY greater than the latest stable, compared as
# triples. `vX.Y.Z-nightly.D` then sorts after `vX.Y.Z'` whenever
# X.Y.Z > X.Y.Z', because a prerelease suffix only lowers a version
# within its own triple.
highest=$(printf '%s\n%s\n' "$latest" "$baseline" \
  | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
if [ "$highest" != "$baseline" ] || [ "$baseline" = "$latest" ]; then
  echo "nightly-baseline.sh: computed baseline $baseline does not outrank the latest stable $latest" >&2
  exit 1
fi

echo "$baseline"
