#!/usr/bin/env bash
# Milestone 1272 — where does each npm-ecosystem manager record the
# optional axis? Runs five real managers against two manifest shapes and
# prints where each writes the flag.
#
# Probe A: a ROOT declaration (`optionalDependencies` at the top level).
# Probe B: a TRANSITIVE one (a runtime parent declaring an optional dep),
#          which is what distinguishes "precomputed closure" from
#          "edge recorded on the parent".
#
# Real coordinates are used deliberately: the subject is the lockfile
# FORMAT, and a synthetic name cannot be installed. Nothing here is
# committed as a fixture, so the synthetic-name rule for fixtures does
# not apply. Writes only inside its own scratch directory.
#
# Usage: ./optional-axis-probe.sh [outdir]
set -uo pipefail

OUT="${1:-$(mktemp -d)}"
mkdir -p "$OUT"
echo "probe root: $OUT"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0

echo "=== versions ==="
for t in node npm yarn corepack pnpm bun; do
  printf '%-9s ' "$t"
  command -v "$t" >/dev/null 2>&1 && "$t" --version 2>&1 | head -1 || echo ABSENT
done

# $1 = dir, $2 = package.json body
seed() { mkdir -p "$1" && printf '%s\n' "$2" > "$1/package.json"; }

ROOT_MANIFEST='{"name":"probe","version":"1.0.0","private":true,
  "dependencies":{"ms":"2.1.3"},
  "optionalDependencies":{"lodash.noop":"3.0.1"},
  "devDependencies":{"escape-string-regexp":"4.0.0"}}'
# chokidar is a runtime dep that declares fsevents optional.
TRANS_MANIFEST='{"name":"probe","version":"1.0.0","private":true,
  "dependencies":{"chokidar":"3.6.0"}}'

for probe in root transitive; do
  case "$probe" in
    root) MANIFEST="$ROOT_MANIFEST" ;;
    transitive) MANIFEST="$TRANS_MANIFEST" ;;
  esac
  echo
  echo "################ PROBE: $probe ################"

  d="$OUT/$probe-npm";  seed "$d" "$MANIFEST"
  ( cd "$d" && npm install --ignore-scripts --package-lock-only >/dev/null 2>&1 )
  echo "--- npm: entries carrying optional ---"
  node -e 'const l=require(process.argv[1]+"/package-lock.json");
    for(const[k,v]of Object.entries(l.packages||{}))
      if(v.optional||v.dev)console.log(" ",k,JSON.stringify({optional:v.optional,dev:v.dev}));' "$d"

  d="$OUT/$probe-pnpm"; seed "$d" "$MANIFEST"
  ( cd "$d" && pnpm install --ignore-scripts --no-frozen-lockfile >/dev/null 2>&1 )
  echo "--- pnpm: importers optionalDependencies + snapshots optional ---"
  sed -n '/^importers:/,/^packages:/p' "$d/pnpm-lock.yaml" 2>/dev/null | grep -A3 optionalDependencies
  sed -n '/^snapshots:/,$p' "$d/pnpm-lock.yaml" 2>/dev/null | grep -B2 'optional: true'

  d="$OUT/$probe-yarnv1"; seed "$d" "$MANIFEST"
  ( cd "$d" && yarn install --ignore-scripts --non-interactive >/dev/null 2>&1 )
  echo "--- yarn v1: occurrences of 'optional' ---"
  grep -n 'optional' "$d/yarn.lock" 2>/dev/null || echo "  ZERO"

  d="$OUT/$probe-berry"; seed "$d" "$MANIFEST"
  # packageManager pins Berry; node-modules linker keeps the run cheap.
  node -e 'const f=process.argv[1]+"/package.json";const j=require(f);
    j.packageManager="yarn@4.6.0";
    require("fs").writeFileSync(f,JSON.stringify(j,null,2));' "$d"
  echo 'nodeLinker: node-modules' > "$d/.yarnrc.yml"
  ( cd "$d" && corepack yarn install --no-immutable >/dev/null 2>&1 )
  echo "--- yarn Berry: dependenciesMeta in the LOCKFILE ---"
  grep -n -A2 'dependenciesMeta' "$d/yarn.lock" 2>/dev/null || echo "  ABSENT from lockfile"

  d="$OUT/$probe-bun"; seed "$d" "$MANIFEST"
  ( cd "$d" && bun install --ignore-scripts >/dev/null 2>&1 )
  echo "--- bun: workspaces optionalDependencies + packages metadata ---"
  grep -n 'optionalDependencies' "$d/bun.lock" 2>/dev/null || echo "  ZERO"
done

echo
echo "probe tree left at: $OUT"
