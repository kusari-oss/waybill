#!/usr/bin/env bash
# What shape are nixpkgs `meta.knownVulnerabilities` entries, and can a
# scanner key on them?
#
# The design question this answers: a CVE identifier maps onto a VEX
# `vulnerability.name`; prose does not. If the entries are mostly prose,
# VEX is the wrong carrier for most of the signal.
#
# Scoped to `pkgs/by-name`, where the attribute name is the directory name,
# so the sample is addressable without evaluating all of nixpkgs (which
# costs minutes and gigabytes). That is a sample, not a census — say so
# when quoting the numbers.
#
# Usage: known-vulnerabilities.sh [path-to-nixpkgs-checkout]
set -uo pipefail

NIXPKGS="${1:-}"
if [ -z "$NIXPKGS" ]; then
  NIXPKGS=$(nix eval --raw --impure --expr 'builtins.toString <nixpkgs>' 2>/dev/null) || {
    echo "pass a nixpkgs checkout path, or make <nixpkgs> resolvable" >&2; exit 1; }
fi
echo "nixpkgs: $NIXPKGS" >&2

ATTRS=$(mktemp)
grep -rl 'knownVulnerabilities' "$NIXPKGS/pkgs/by-name" 2>/dev/null \
  | awk -F/ '{print $(NF-1)}' | sort -u > "$ATTRS"
echo "attributes declaring knownVulnerabilities under by-name: $(wc -l < "$ATTRS")" >&2

EXPR=$(mktemp --suffix=.nix 2>/dev/null || mktemp -t kv)
python3 - "$ATTRS" > "$EXPR" <<'PY'
import sys
attrs=[l.strip() for l in open(sys.argv[1]) if l.strip()]
names=" ".join(f'"{a}"' for a in attrs)
print(f'''let
  p = import <nixpkgs> {{ config.allowUnfree = true; config.allowBroken = true; }};
  probe = n:
    let
      raw = let v = p.${{n}}; in {{
        kv = v.meta.knownVulnerabilities or [];
        version = v.version or "";
      }};
      # deepSeq INSIDE the guard: tryEval returns a lazy value, so without
      # it the throw escapes at serialisation time, outside the guard, and
      # the whole evaluation dies on the first unfree package.
      r = builtins.tryEval (builtins.deepSeq raw raw);
    in {{ name = n; value = if r.success then r.value else {{ kv = null; version = null; }}; }};
in builtins.listToAttrs (map probe [ {names} ])''')
PY

nix eval --json --impure --file "$EXPR" 2>/dev/null | python3 -c '
import json,re,sys
d=json.load(sys.stdin)
CVE=re.compile(r"CVE-\d{4}-\d+")
ok={k:v for k,v in d.items() if v["kv"] is not None}
declaring={k:v for k,v in ok.items() if v["kv"]}
entries=[(k,e) for k,v in declaring.items() for e in v["kv"]]
cve=[(k,e) for k,e in entries if CVE.search(e)]
prose=[(k,e) for k,e in entries if not CVE.search(e)]
print(f"probed {len(d)}, evaluated {len(ok)}, declaring {len(declaring)}")
print(f"entries {len(entries)}: CVE-bearing {len(cve)}, prose {len(prose)}")
print(f"distinct CVE ids: {len({m for _,e in cve for m in CVE.findall(e)})}")
print()
for k,e in prose: print(f"  PROSE  {k}: {e[:100]}")
'
