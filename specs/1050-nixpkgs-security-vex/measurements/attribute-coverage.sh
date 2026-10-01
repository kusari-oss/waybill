#!/usr/bin/env bash
# What fraction of a closure's members can have their declaration reached,
# and how often would a naive lookup attach one to the wrong component?
#
# This is the number the feature's value rests on. Run it against a real
# project before trusting the implementation's own figure.
#
# Usage: attribute-coverage.sh <project-dir> <closure.json>
#   closure.json is the output of:
#     nix derivation show -r --option allow-import-from-derivation false \
#       "path:<project-dir>#default"
set -uo pipefail
PROJ="${1:?project dir}"; CLOSURE="${2:?closure json}"

MEMBERS=$(mktemp); EXPR=$(mktemp)
python3 - "$CLOSURE" > "$MEMBERS" <<'PY'
import json,sys
d=json.load(open(sys.argv[1])); drvs=d.get("derivations",d)
print(json.dumps(sorted({(v.get("env",{}) or {}).get("pname") for v in drvs.values()
                         if (v.get("env",{}) or {}).get("pname")})))
PY

python3 - "$MEMBERS" "$PROJ" > "$EXPR" <<'PY'
import json,sys
names=json.load(open(sys.argv[1])); proj=sys.argv[2]
lst=" ".join(f'"{n}"' for n in names)
print(f'''let
  flake = builtins.getFlake "path:{proj}";
  pkgs = flake.inputs.nixpkgs.legacyPackages.${{builtins.currentSystem}};
  sets = [
    {{ id = "top";     set = pkgs; }}
    {{ id = "haskell"; set = pkgs.haskellPackages or {{}}; }}
    {{ id = "python3"; set = pkgs.python3Packages or {{}}; }}
    {{ id = "perl";    set = pkgs.perlPackages or {{}}; }}
  ];
  probeIn = s: n:
    let
      # `or null`: a missing attribute is NOT a throw and escapes tryEval.
      cand = s.set.${{n}} or null;
      raw = if cand == null then null else
        {{ setId = s.id; out = cand.outPath;
           kv = cand.meta.knownVulnerabilities or []; }};
      # deepSeq INSIDE the guard: tryEval returns a lazy value, so a throw
      # would otherwise escape at serialisation time, outside it.
      r = builtins.tryEval (builtins.deepSeq raw raw);
    in if r.success then r.value else null;
  first = n: builtins.foldl' (a: s: if a != null then a else probeIn s n) null sets;
in builtins.listToAttrs (map (n: {{ name = n; value = first n; }}) [ {lst} ])''')
PY

START=$(python3 -c 'import time;print(time.time())')
ATTRS=$(mktemp)
nix eval --json --impure --file "$EXPR" > "$ATTRS" 2>/dev/null || { echo "eval failed" >&2; exit 1; }
END=$(python3 -c 'import time;print(time.time())')
python3 -c "print(f'evaluation wall seconds: {$END-$START:.1f}')"

python3 - "$CLOSURE" "$ATTRS" <<'PY'
import json,sys
from collections import Counter
# Both sides MUST be reduced to a basename. The closure JSON omits the
# /nix/store/ prefix that outPath carries; comparing raw yields ZERO matches
# and reads as "the mechanism does not work".
base=lambda p: p.rsplit("/",1)[-1]
d=json.load(open(sys.argv[1])); drvs=d.get("derivations",d)
attrs=json.load(open(sys.argv[2]))
mem={}
for v in drvs.values():
    env=v.get("env",{}) or {}
    p=env.get("pname")
    if not p: continue
    mem.setdefault((p,env.get("version","")), set()).update(
        base(o["path"]) for o in (v.get("outputs") or {}).values() if o.get("path"))
no_attr=conf=mis=0; where=Counter(); declared=0
for (p,ver),paths in mem.items():
    a=attrs.get(p)
    if a is None: no_attr+=1; continue
    if base(a["out"]) in paths:
        conf+=1; where[a["setId"]]+=1
        if a["kv"]: declared+=1
    else: mis+=1
t=len(mem)
print(f"members (distinct pname+version) : {t}")
print(f"  CONFIRMED by output path       : {conf} ({100*conf//t}%)  {dict(where)}")
print(f"  no attribute in any set        : {no_attr} ({100*no_attr//t}%)")
print(f"  found but path differs         : {mis} ({100*mis//t}%)  <- false attributions prevented")
print(f"  confirmed members declaring    : {declared}")
PY
