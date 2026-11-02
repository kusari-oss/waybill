#!/usr/bin/env bash
# Probe G — yarn v1 (Yarn Classic), milestone 1272 US4.
#
# Probes B and C established that a v1 lockfile has no root or workspace
# entry, so a ROOT optionalDependencies declaration has nowhere to land,
# and that v1 writes zero occurrences of the substring `optional` for a
# root-only declaration. This probe asks the two questions US4's
# implementation actually depends on:
#
#   G1  Does a v1 lockfile record a TRANSITIVE optional edge, and in
#       what syntax -- the per-entry `optionalDependencies:` sub-block?
#   G2  When a package is reachable BOTH as a transitive optional child
#       and by a hard path, does v1 record anything that distinguishes
#       it? (It should not: v1 records declarations, not a closure.)
#
# Real coordinates are deliberate: this script measures the FORMAT yarn
# emits, which synthetic names cannot do. The committed fixture derived
# from it uses synthetic names per FR-015.
set -euo pipefail

YARN="${YARN:-yarn}"
"$YARN" --version

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# ---- G1: a transitive optional edge -------------------------------
# chokidar declares fsevents under optionalDependencies. Root declares
# chokidar as a plain runtime dep, so the only optional declaration in
# the whole graph is a transitive one.
mkdir -p "$work/g1"
cat > "$work/g1/package.json" <<'JSON'
{ "name": "g1", "version": "1.0.0", "private": true,
  "dependencies": { "chokidar": "3.5.3" } }
JSON
( cd "$work/g1" && "$YARN" install --silent --no-progress >/dev/null 2>&1 )
echo "=== G1: does the lockfile name an optionalDependencies sub-block? ==="
grep -c 'optionalDependencies:' "$work/g1/yarn.lock" || echo "0 occurrences"
echo "--- the chokidar entry, verbatim ---"
awk '/^chokidar@/{p=1} p&&/^$/{exit} p' "$work/g1/yarn.lock"
echo "--- is fsevents itself an entry, and does it carry a marker? ---"
awk '/^fsevents@/{p=1} p&&/^$/{exit} p' "$work/g1/yarn.lock"

# ---- G2: contested -- optional child ALSO reachable hard ----------
# Root additionally declares fsevents as a plain runtime dependency, so
# the same package is both chokidar's optional child and a hard root dep.
#
# The declared RANGE must match chokidar's own (`~2.3.2`) or the two
# cases are not comparable: yarn keys an entry by descriptor, so a
# different range yields a different key and a different resolved
# version, and the diff below would report a difference the optional
# axis had no part in. The first run of this probe made exactly that
# mistake and reported DIFFER.
mkdir -p "$work/g2"
cat > "$work/g2/package.json" <<'JSON'
{ "name": "g2", "version": "1.0.0", "private": true,
  "dependencies": { "chokidar": "3.5.3", "fsevents": "~2.3.2" } }
JSON
( cd "$work/g2" && "$YARN" install --silent --no-progress >/dev/null 2>&1 )
echo
echo "=== G2: contested case -- the fsevents entry, verbatim ==="
awk '/^fsevents@/{p=1} p&&/^$/{exit} p' "$work/g2/yarn.lock"
echo "--- do G1 and G2 differ in the fsevents entry? ---"
if diff <(awk '/^fsevents@/{p=1} p&&/^$/{exit} p' "$work/g1/yarn.lock") \
        <(awk '/^fsevents@/{p=1} p&&/^$/{exit} p' "$work/g2/yarn.lock") >/dev/null
then echo "IDENTICAL -- v1 records no closure; the reader must derive it"
else echo "DIFFER -- v1 does distinguish; read the diff above"
fi
echo
echo "=== root optionalDependencies: does it appear anywhere? ==="
mkdir -p "$work/g3"
cat > "$work/g3/package.json" <<'JSON'
{ "name": "g3", "version": "1.0.0", "private": true,
  "optionalDependencies": { "fsevents": "2.3.2" } }
JSON
( cd "$work/g3" && "$YARN" install --silent --no-progress >/dev/null 2>&1 )
grep -c 'optional' "$work/g3/yarn.lock" || echo "0 occurrences of 'optional'"
cat "$work/g3/yarn.lock"
