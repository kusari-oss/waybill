#!/usr/bin/env bash
# Milestone 1272 — how does pnpm v9 express PEER-optionality, and what
# happens to a peer that is also reachable at runtime?
#
# Added during implementation (T063), not during Phase 0. The first two
# probes established where the optional marker lives and what it means;
# neither asked how the peer-precedence guard FR-009 requires is meant
# to read its input on v9. Writing the fix surfaced the question:
#
#   `pnpm_lock.rs` read `is_peer` from `tbl.get("peer")` -- the
#   `packages:` entry. A real pnpm v9 lockfile contains NO `peer:` key
#   anywhere. The guard could never fire. That was invisible while the
#   optional branch was also dead, and stops being invisible the moment
#   it is not.
#
# Four shapes, because the answer differs by reachability:
#
#   p1  runtime parent, peer ALSO a runtime root dep
#   p2  optional parent, peer ALSO a runtime root dep
#   p3  optional parent, optional peer, NO runtime path to either
#   p4  runtime parent, peer declared optional at the root
#
# Real coordinates deliberately -- the subject is the lockfile FORMAT and
# a synthetic name cannot be installed. Nothing here is committed as a
# fixture; see `../../..`'s FR-015 for the rule that applies to those.
# Writes only inside its own scratch directory.
#
# Usage: ./peer-context-probe.sh [outdir]
set -uo pipefail

OUT="${1:-$(mktemp -d)}"
mkdir -p "$OUT"
echo "probe root: $OUT"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
printf 'node %s   pnpm %s\n' "$(node --version)" "$(pnpm --version)"

seed() { mkdir -p "$1" && printf '%s\n' "$2" > "$1/package.json"; }

# debug@4.3.4 declares supports-color as an OPTIONAL peer.
seed "$OUT/p1" '{"name":"probe","version":"1.0.0","private":true,
  "dependencies":{"debug":"4.3.4","supports-color":"9.4.0"}}'
seed "$OUT/p2" '{"name":"probe","version":"1.0.0","private":true,
  "optionalDependencies":{"debug":"4.3.4"},
  "dependencies":{"supports-color":"9.4.0"}}'
seed "$OUT/p3" '{"name":"probe","version":"1.0.0","private":true,
  "optionalDependencies":{"debug":"4.3.4","supports-color":"9.4.0"}}'
seed "$OUT/p4" '{"name":"probe","version":"1.0.0","private":true,
  "dependencies":{"debug":"4.3.4"},
  "optionalDependencies":{"supports-color":"9.4.0"}}'

for d in p1 p2 p3 p4; do
  ( cd "$OUT/$d" && pnpm install --ignore-scripts --no-frozen-lockfile >/dev/null 2>&1 )
  echo
  echo "################ $d ################"
  echo "--- packages: any 'peer' or 'optional' key ---"
  sed -n '/^packages:/,/^snapshots:/p' "$OUT/$d/pnpm-lock.yaml" \
    | grep -E '^  [a-z@]|peer|optional' || echo "  (none)"
  echo "--- snapshots: keys and flags ---"
  sed -n '/^snapshots:/,$p' "$OUT/$d/pnpm-lock.yaml" \
    | grep -E '^  [a-z@]|optional: true|peer' || echo "  (none)"
done

echo
echo "=== the two load-bearing checks ==="
echo "1. a bare 'peer:' key anywhere in any of the four lockfiles?"
grep -l '^\s*peer: ' "$OUT"/p*/pnpm-lock.yaml 2>/dev/null \
  || echo "   NO -- so \`tbl.get(\"peer\")\` can never fire on v9"
echo "2. peer-context suffixes on snapshot keys (one canonical identity,"
echo "   several keys -- the reason the optional flag must AND-fold):"
grep -h '^  .*(' "$OUT"/p*/pnpm-lock.yaml 2>/dev/null | sort -u | sed 's/^/   /'

echo
echo "probe tree left at: $OUT"
