"""Compare three sets of Haskell libraries for a static executable:
  BIN   : units with defined symbols in the binary (q6-linked-units.py), restricted to
          names that are a Haskell derivation in the closure (boot units shipped inside
          GHC are reported separately)
  CABAL : cabal-closure.nix truth
  R1    : the candidate field rule (root buildInputs+propagatedBuildInputs, then
          propagatedBuildInputs through Haskell derivations)

usage: python3 -I -B q6-binary-vs-rule.py <drv-closure.json> <runtime.paths> <cabal-closure.json> <linked-units.tsv>
"""
import json
import os
import sys

drv_json, rt_path, cabal_path, units_path = sys.argv[1:5]
drvs = json.load(open(drv_json))["derivations"]
rt = {os.path.basename(l.strip()) for l in open(rt_path) if l.strip()}
out2drv = {o["path"]: d for d, v in drvs.items() for o in v["outputs"].values() if o.get("path")}


def env(d):
    v = drvs[d]
    return v.get("structuredAttrs", v.get("env", {}))


def is_hs(d):
    return "setupCompilerEnvironmentPhase" in env(d)


def fdrvs(d, f):
    val = env(d).get(f, "")
    items = val.split() if isinstance(val, str) else val
    return [out2drv[os.path.basename(p)] for p in items if os.path.basename(p) in out2drv]


hs_names = {drvs[d]["name"] for d in drvs if is_hs(d)}
rt_hs = [d for d in {out2drv[p] for p in rt if p in out2drv} if is_hs(d)]
seen, stack = set(), [x for m in rt_hs for f in ("buildInputs", "propagatedBuildInputs") for x in fdrvs(m, f) if is_hs(x)]
while stack:
    d = stack.pop()
    if d in seen:
        continue
    seen.add(d)
    stack += [x for x in fdrvs(d, "propagatedBuildInputs") if is_hs(x) and x not in seen]
r1 = {drvs[d]["name"] for d in seen - set(rt_hs)}
rt_hs_names = {drvs[d]["name"] for d in rt_hs}

cabal = {f'{x["name"]}-{x["version"]}' for x in json.load(open(cabal_path))["closure"]}
units = [l.split("\t")[0] for l in open(units_path) if l.strip()]
binary_all = {u for u in units if "(boot" not in u}
binary = {u for u in binary_all if u in hs_names} - rt_hs_names
boot = sorted(binary_all - hs_names)

print(f"BIN units with a Haskell derivation in the closure (excl. the runtime member itself): {len(binary)}")
print(f"BIN units with no derivation of their own (GHC boot libraries): {len(boot)} {boot}")
print(f"CABAL: {len(cabal)}   R1: {len(r1)}")
print(f"BIN - CABAL (linked, cabal metadata misses): {sorted(binary - cabal)}")
print(f"BIN - R1    (linked, rule misses)          : {sorted(binary - r1)}")
print(f"CABAL - BIN (declared, no surviving code)  : {len(cabal - binary)} {sorted(cabal - binary)}")
print(f"R1 - CABAL  (rule extra vs cabal)          : {sorted(r1 - cabal)}")
print(f"R1 - BIN    (rule extra vs binary)         : {len(r1 - binary)}")
