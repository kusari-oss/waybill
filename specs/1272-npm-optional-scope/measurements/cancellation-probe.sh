#!/usr/bin/env bash
# Milestone 1272 — does a non-optional path CANCEL optionality?
#
# The sibling probe (optional-axis-probe.sh) established WHERE each
# manager records the optional axis. It did not build the one graph shape
# that decides FR-006: a package reachable by BOTH an optional and a
# non-optional path. The spec's Assumptions section records that rule as
# inferred rather than measured. This probe measures it.
#
# Only npm and pnpm can answer. They are the two managers that write a
# precomputed closure (`optional: true` per package / per snapshot);
# yarn v1, Berry and bun record only the edge on the declaring parent,
# so for those three the closure is waybill's to compute and there is no
# manager answer to read. All five are run anyway, so the record shows
# what each file does and does not contain.
#
# Case C1 (contested): root declares `debug` runtime and `ms` optional.
#   debug depends on ms non-optionally, so ms is reachable BOTH ways.
#   Cancellation predicts: ms is NOT marked optional.
# Case C2 (control): root declares `debug` optional and nothing runtime.
#   ms is then reachable ONLY through an optional edge.
#   Propagation predicts: ms IS marked optional.
#
# C2 is a positive control, not decoration. If both cases returned the
# same answer the probe would be reporting its own harness rather than
# the managers -- the failure mode recorded in
# `feedback_a_probe_can_lie_about_the_system`. The two cases MUST differ.
#
# The probe also prints every `ms` entry per case: if debug's range and
# the pinned optional range resolve to different versions there are two
# ms nodes, the shape under test does not exist, and the case is void.
#
# Real coordinates are used deliberately -- the subject is the lockfile
# FORMAT and a synthetic name cannot be installed. Nothing here is
# committed as a fixture, so the synthetic-name rule for fixtures does
# not apply. Writes only inside its own scratch directory.
#
# Usage: ./cancellation-probe.sh [outdir]
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

seed() { mkdir -p "$1" && printf '%s\n' "$2" > "$1/package.json"; }

# ms is reachable via debug (runtime) AND declared optional at the root.
C1_MANIFEST='{"name":"probe","version":"1.0.0","private":true,
  "dependencies":{"debug":"4.3.4"},
  "optionalDependencies":{"ms":"2.1.2"}}'
# ms is reachable ONLY through the optional debug edge.
C2_MANIFEST='{"name":"probe","version":"1.0.0","private":true,
  "optionalDependencies":{"debug":"4.3.4"}}'

for probe in c1-contested c2-optional-only; do
  case "$probe" in
    c1-contested)      MANIFEST="$C1_MANIFEST" ;;
    c2-optional-only)  MANIFEST="$C2_MANIFEST" ;;
  esac
  echo
  echo "################ CASE: $probe ################"

  d="$OUT/$probe-npm";  seed "$d" "$MANIFEST"
  ( cd "$d" && npm install --ignore-scripts --package-lock-only >/dev/null 2>&1 )
  echo "--- npm: every ms entry, and every entry carrying optional ---"
  node -e 'const l=require(process.argv[1]+"/package-lock.json");
    for(const[k,v]of Object.entries(l.packages||{})){
      if(/(^|\/)ms$/.test(k))console.log("  ms-node ",k,JSON.stringify({version:v.version,optional:v.optional||false}));}
    for(const[k,v]of Object.entries(l.packages||{}))
      if(v.optional)console.log("  optional",k);' "$d" 2>&1

  d="$OUT/$probe-pnpm"; seed "$d" "$MANIFEST"
  ( cd "$d" && pnpm install --ignore-scripts --no-frozen-lockfile >/dev/null 2>&1 )
  echo "--- pnpm: snapshots entries and which carry optional: true ---"
  sed -n '/^snapshots:/,$p' "$d/pnpm-lock.yaml" 2>/dev/null | grep -E "^  [a-z@]|optional: true" || echo "  (no snapshots section)"

  d="$OUT/$probe-yarnv1"; seed "$d" "$MANIFEST"
  ( cd "$d" && yarn install --ignore-scripts --non-interactive >/dev/null 2>&1 )
  echo "--- yarn v1: occurrences of 'optional' ---"
  grep -n 'optional' "$d/yarn.lock" 2>/dev/null || echo "  ZERO"

  d="$OUT/$probe-berry"; seed "$d" "$MANIFEST"
  node -e 'const f=process.argv[1]+"/package.json";const j=require(f);
    j.packageManager="yarn@4.6.0";
    require("fs").writeFileSync(f,JSON.stringify(j,null,2));' "$d"
  echo 'nodeLinker: node-modules' > "$d/.yarnrc.yml"
  ( cd "$d" && corepack yarn install --no-immutable >/dev/null 2>&1 )
  echo "--- yarn Berry: dependenciesMeta in the LOCKFILE ---"
  grep -n -A2 'dependenciesMeta' "$d/yarn.lock" 2>/dev/null || echo "  ABSENT from lockfile"

  d="$OUT/$probe-bun"; seed "$d" "$MANIFEST"
  ( cd "$d" && bun install --ignore-scripts >/dev/null 2>&1 )
  echo "--- bun: occurrences of 'optional' ---"
  grep -n 'optional' "$d/bun.lock" 2>/dev/null || echo "  ZERO"
done

echo
echo "probe tree left at: $OUT"
