"""hlint (dynamically linked): runtime Haskell library outputs vs the cabal library closure.
usage: python3 -I -B q7-hs-extra.py <drv-closure.json> <runtime.paths> <cabal-closure.json>"""
import json, os, sys
drvs = json.load(open(sys.argv[1]))["derivations"]
rt = {os.path.basename(l.strip()) for l in open(sys.argv[2]) if l.strip()}
cabal = {os.path.basename(x["out"]) for x in json.load(open(sys.argv[3]))["closure"]}
hs_out = {}
for d, v in drvs.items():
    e = v.get("structuredAttrs", v.get("env", {}))
    if "setupCompilerEnvironmentPhase" in e and "out" in v["outputs"]:
        hs_out[v["outputs"]["out"]["path"]] = v["name"]
rt_hs = {p: n for p, n in hs_out.items() if p in rt}
root = [n for n in rt_hs.values() if n.startswith("hlint-")]
extra = sorted(n for p, n in rt_hs.items() if p not in cabal and n not in root)
print(f"runtime Haskell `out` paths: {len(rt_hs)} (incl. root {root}); cabal library closure: {len(cabal)}; "
      f"cabal libs in runtime closure: {len(cabal & set(rt_hs))}")
print(f"runtime Haskell libraries NOT in the cabal library closure: {len(extra)}")
print(extra)
