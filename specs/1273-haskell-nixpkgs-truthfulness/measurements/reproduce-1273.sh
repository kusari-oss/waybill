#!/usr/bin/env bash
# Milestone 1273 — reproduce all four issues this feature covers.
#
# All four (#1278, #1279, #1280, #1281) were filed against waybill 0.13.1
# (`96e7a545`). This script re-measures them against whatever binary it is
# pointed at, because a spec built on a stale defect report describes a
# defect that may no longer exist. The numbers in spec.md's Context table
# come from running this, not from the issues.
#
# #1279/#1280/#1281 need a cached nixpkgs revision. The default is the one
# #1281 was measured against, so the run is offline and reproducible.
set -euo pipefail

WAYBILL="${WAYBILL:-./target/debug/waybill}"
NIXPKGS_REV="${NIXPKGS_REV:-a799d3e3886da994fa307f817a6bc705ae538eeb}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

scan() { # <dir> <out.json> ; echoes the log to stderr
  "$WAYBILL" --offline sbom scan --path "$1" --no-deep-hash \
    --format cyclonedx-json --output "cyclonedx-json=$2" 2>&1
}

# ── #1278 — an hpack-only project ──────────────────────────────────
mkdir -p "$work/hpack/src"
cat > "$work/hpack/package.yaml" <<'YAML'
name: waybill-fixture-hpack
version: 0.1.0
license: MIT
dependencies:
  - base >= 4 && < 5
  - text
  - aeson
library:
  source-dirs: src
executables:
  waybill-fixture-hpack:
    main: Main.hs
    dependencies:
      - waybill-fixture-hpack
YAML
echo 'module Lib where' > "$work/hpack/src/Lib.hs"

echo "=== #1278 — hpack-only project ==="
RUST_LOG=info scan "$work/hpack" "$work/hpack.json" > "$work/hpack.log" || true
python3 - "$work/hpack.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
hk = [x for x in c["components"] if (x.get("purl") or "").startswith("pkg:hackage")]
props = {p["name"]: p["value"] for p in (c.get("metadata", {}).get("properties") or [])}
print(f"  hackage components: {len(hk)}   (package.yaml declares 3)")
print(f"  all components:     {[x['name'] for x in c['components']]}")
print(f"  graph-completeness: {props.get('waybill:graph-completeness')}")
PY
# The reader IS dispatched to the file -- it sees it and says nothing. This
# is sharper than the issue states, so it is asserted here.
sed 's/\x1b\[[0-9;]*m//g' "$work/hpack.log" | grep -o '"haskell": [0-9]*' | head -1 \
  | sed 's/^/  per_reader_dispatch_counts /'
echo -n "  log diagnostic naming package.yaml: "
if sed 's/\x1b\[[0-9;]*m//g' "$work/hpack.log" | grep -qi "package\.yaml"; then
  echo "PRESENT"; else echo "ABSENT"; fi
# After US1 the document itself carries the account (C200), which is the half
# FR-018 is about: a log reaches only the operator who ran the scan.
echo -n "  document record of what the hpack reader did: "
python3 - "$work/hpack.json" <<'PY' || true
import json, sys
c = json.load(open(sys.argv[1]))
props = {p["name"]: p["value"] for p in (c.get("metadata", {}).get("properties") or [])}
raw = props.get("waybill:hpack-read-outcome")
if not raw:
    print("ABSENT  <-- the defect (#1278)")
else:
    o = json.loads(raw)
    print(f'seen={len(o.get("manifests-seen", []))} '
          f'read={len(o.get("manifests-read", []))} '
          f'unread={len(o.get("manifests-unread", []))} '
          f'partial={len(o.get("manifests-partial", []))}')
PY

# ── #1281 — the optional mark, with and without flake.lock ─────────
mkdir -p "$work/flag"
cat > "$work/flag/waybill-fixture-flag.cabal" <<'CABAL'
cabal-version: 2.4
name:          waybill-fixture-flag
version:       0.1.0
license:       MIT

flag cli
  description: Build the optional CLI.
  default:     False
  manual:      True

library
  build-depends: base >= 4 && < 5, text
  exposed-modules: Lib

executable waybill-fixture-flag-cli
  main-is: Main.hs
  if flag(cli)
    buildable: True
  else
    buildable: False
  build-depends: base, waybill-fixture-flag, aeson, async
CABAL

echo
echo "=== #1281 — case A: .cabal only (the mark should hold) ==="
scan "$work/flag" "$work/flagA.json" > /dev/null || true
report_scope() {
  python3 - "$1" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
for n in ("aeson", "async"):
    m = [x for x in c["components"] if x["name"] == n]
    if not m:
        print(f"  {n}: ABSENT"); continue
    x = m[0]
    p = {q["name"]: q["value"] for q in x.get("properties", [])}
    print(f"  {n} @ {x.get('version') or '-'} | scope={x.get('scope')} "
          f"| lifecycle={p.get('waybill:lifecycle-scope')} "
          f"| derivation={p.get('waybill:optional-derivation')}")
PY
}
report_scope "$work/flagA.json"

cat > "$work/flag/flake.lock" <<LOCK
{
  "nodes": {
    "nixpkgs": {
      "locked": { "lastModified": 1730000000,
        "narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "owner": "NixOS", "repo": "nixpkgs",
        "rev": "$NIXPKGS_REV", "type": "github" },
      "original": { "owner": "NixOS", "ref": "nixpkgs-unstable",
        "repo": "nixpkgs", "type": "github" }
    },
    "root": { "inputs": { "nixpkgs": "nixpkgs" } }
  },
  "root": "root",
  "version": 7
}
LOCK

echo
echo "=== #1281 — case B: + flake.lock pinning nixpkgs (the mark is withdrawn) ==="
RUST_LOG=info scan "$work/flag" "$work/flagB.json" > "$work/flagB.log" || true
report_scope "$work/flagB.json"
echo "  the closure, and what the reconciliation did:"
# `|| true` on every grep: a zero match exits 1 and `set -o pipefail` would
# kill the script, which is how this probe silently stopped printing its last
# two sections after the #1281 fix changed the log line it keyed on.
sed 's/\x1b\[[0-9;]*m//g' "$work/flagB.log" \
  | grep -oE "runtime closure resolved declared=[0-9]+ transitive=[0-9]+" \
  | sed 's/^/    /' || true
sed 's/\x1b\[[0-9;]*m//g' "$work/flagB.log" \
  | grep -oE "reviewed=[0-9]+ withdrawn=[0-9]+ declined=[0-9]+|restored=[0-9]+" \
  | sed 's/^/    /' || true
echo "    (declared=5 CONTAINS aeson and async. Before the #1281 fix that was"
echo "     the whole problem: the closure reached them by construction and"
echo "     withdrew both marks. After it, a second closure seeded from"
echo "     declared-minus-marked reaches neither, so both marks stand.)"

# ── #1280 and #1279 — read off case B's resolved document ──────────
echo
echo "=== #1280 (licenses) and #1279 (unresolved-reason) ==="
python3 - "$work/flagB.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
hk = [x for x in c["components"] if (x.get("purl") or "").startswith("pkg:hackage")]
has = lambda x, k: any(q["name"] == k for q in x.get("properties", []))
reason = "waybill:unresolved-reason"
with_lic = [x for x in hk if x.get("licenses")]
with_reason = [x for x in hk if has(x, reason)]
resolved = [x for x in hk if x.get("version")]
both = [x for x in resolved if has(x, reason)]
print(f"  hackage components:                      {len(hk)}")
print(f"  #1280  carrying a license:               {len(with_lic)}/{len(hk)}")
print(f"  #1279  carrying {reason}: {len(with_reason)}/{len(hk)}")
print(f"  #1279  RESOLVED *and* still unresolved:  {len(both)}   <-- the contradiction")
for x in both:
    p = {q["name"]: q["value"] for q in x.get("properties", [])}
    print(f"           {x['name']} @ {x['version']} :: {p.get(reason)}")
PY
