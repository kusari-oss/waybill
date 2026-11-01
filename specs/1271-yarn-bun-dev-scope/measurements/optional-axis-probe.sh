#!/bin/sh
# Where does each npm-ecosystem lockfile record the OPTIONAL axis for a
# dependency declared only under the root manifest's `optionalDependencies`?
#
# Companion to lockfile-dev-axis-probe.sh, which answered the same question
# for the development axis. This one exists because milestone 1271's T036
# cross-reader comparison found one component classified `optional` by the
# package-lock reader and unset by yarn v1, yarn Berry and pnpm v9 — and the
# repository's fixtures for those three are hand-written, so a hand-written
# fixture that simply lacks the marker would look exactly like a reader that
# cannot read one. The probe decides which it is before anything is filed.
#
# One real runtime dependency and one real optional dependency, both tiny and
# dependency-free. Real coordinates deliberately: the probe measures a package
# manager's output, and a synthetic name cannot be resolved.
#
# --ignore-scripts throughout: the probe reads text and never needs a
# package's install hooks to run.
#
# Usage: optional-axis-probe.sh [output-dir]
set -eu
OUT="${1:-$(mktemp -d)}"

for pm in npm pnpm yarn1 berry bun; do
  d="$OUT/$pm"
  mkdir -p "$d"
  cat > "$d/package.json" <<'JSON'
{
  "name": "optional-probe",
  "version": "1.0.0",
  "dependencies": { "ms": "2.1.3" },
  "optionalDependencies": { "tiny-invariant": "1.3.3" }
}
JSON
done

( cd "$OUT/npm"  && npm install --ignore-scripts --no-audit --no-fund >/dev/null 2>&1 ) || echo "npm: install failed"
( cd "$OUT/pnpm" && pnpm install --ignore-scripts >/dev/null 2>&1 )                      || echo "pnpm: install failed"
( cd "$OUT/yarn1" && yarn install --ignore-scripts --non-interactive --no-progress >/dev/null 2>&1 ) || echo "yarn1: install failed"
( cd "$OUT/berry" && yarn set version berry >/dev/null 2>&1 && yarn install >/dev/null 2>&1 )        || echo "berry: install failed"
( cd "$OUT/bun"  && bun install --ignore-scripts >/dev/null 2>&1 )                       || echo "bun: install failed"

for pm in npm pnpm yarn1 berry bun; do
  d="$OUT/$pm"
  lock=$(ls "$d"/package-lock.json "$d"/pnpm-lock.yaml "$d"/yarn.lock "$d"/bun.lock 2>/dev/null | head -1 || true)
  printf '\n===== %s (%s) =====\n' "$pm" "${lock:+$(basename "$lock")}"
  [ -n "${lock:-}" ] || { echo "  no lockfile produced"; continue; }
  echo "  lines mentioning 'optional' (any case):"
  grep -n -i optional "$lock" | sed 's/^/    /' || echo "    none"
  echo "  --- whole file ---"
  sed 's/^/    /' "$lock"
done
