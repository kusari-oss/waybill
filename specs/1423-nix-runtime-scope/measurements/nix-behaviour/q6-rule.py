"""Score candidate "statically linked Haskell library" rules against cabal metadata.

usage: python3 -I -B q6-rule.py <drv-closure.json> <runtime.paths> <cabal-closure.json> <root.drv>

drv-closure.json : `nix derivation show -r <root.drv>` (nix 2.34, format version 4)
runtime.paths    : `nix path-info -r <outPath>` output, one store path per line
cabal-closure    : output of cabal-closure.nix (truth: library deps, transitively)

A derivation is "Haskell" if its env carries the generic builder's
`setupCompilerEnvironmentPhase`. Rules start from every Haskell derivation that has an
output in the runtime closure ("runtime Haskell members") and walk derivation fields.
Matching to the truth is by output store path, never by name.
"""
import json
import os
import sys

drv_json, rt_path, truth_path, root_drv = sys.argv[1:5]
drvs = json.load(open(drv_json))["derivations"]
rt = {os.path.basename(l.strip()) for l in open(rt_path) if l.strip()}
truth_doc = json.load(open(truth_path))
truth = {os.path.basename(x["out"]): f'{x["name"]}-{x["version"]}' for x in truth_doc["closure"]}

out2drv = {}
for d, v in drvs.items():
    for o in v["outputs"].values():
        if o.get("path"):
            out2drv[o["path"]] = d


def env(d):
    v = drvs[d]
    return v.get("env", {}) if "structuredAttrs" not in v else v["structuredAttrs"]


def field_drvs(d, field):
    val = env(d).get(field, "")
    items = val.split() if isinstance(val, str) else val
    res = []
    for p in items:
        b = os.path.basename(p)
        if b in out2drv:
            res.append(out2drv[b])
    return res


def is_hs(d):
    return "setupCompilerEnvironmentPhase" in env(d)


def name_of(d):
    return drvs[d]["name"]


def out_of(d):
    return drvs[d]["outputs"].get("out", {}).get("path")


structured = sum(1 for v in drvs.values() if "structuredAttrs" in v)
hs_all = [d for d in drvs if is_hs(d)]
rt_members = sorted({out2drv[p] for p in rt if p in out2drv})
rt_unmatched = sorted(p for p in rt if p not in out2drv)
rt_hs = [d for d in rt_members if is_hs(d)]

print(f"root drv               : {os.path.basename(root_drv)}")
print(f"derivations in closure : {len(drvs)} (structuredAttrs: {structured}; Haskell generic-builder: {len(hs_all)})")
print(f"runtime paths          : {len(rt)}; matched to a closure output: {len(rt) - len(rt_unmatched)}; unmatched: {rt_unmatched}")
print(f"runtime members        : {[name_of(d) for d in rt_members]}")
print(f"runtime Haskell members: {[name_of(d) for d in rt_hs]}")
print(f"truth (cabal closure)  : {len(truth)} libraries")
in_rt = sorted(v for k, v in truth.items() if k in rt)
print(f"truth libs whose out path is itself in the runtime closure: {len(in_rt)} {in_rt}")
print()


def walk(start_fields, step_fields, start_from_inputs_drvs=False, step_inputs_drvs=False):
    seen = set()
    stack = []
    for m in rt_hs:
        if start_from_inputs_drvs:
            nxt = list(drvs[m]["inputs"]["drvs"])
        else:
            nxt = [x for f in start_fields for x in field_drvs(m, f)]
        stack += [x for x in nxt if is_hs(x)]
    while stack:
        d = stack.pop()
        if d in seen:
            continue
        seen.add(d)
        if step_inputs_drvs:
            nxt = list(drvs[d]["inputs"]["drvs"])
        else:
            nxt = [x for f in step_fields for x in field_drvs(d, f)]
        stack += [x for x in nxt if is_hs(x) and x not in seen]
    return seen - set(rt_hs)


def walk_from_cabal_direct():
    """Start from the root's cabal lib+exe deps (getCabalDeps, eval-time metadata, not in the .drv),
    then step over propagatedBuildInputs like R1."""
    start = [out2drv[os.path.basename(x["out"])] for x in truth_doc["direct"] if os.path.basename(x["out"]) in out2drv]
    seen, stack = set(), [d for d in start if is_hs(d)]
    while stack:
        d = stack.pop()
        if d in seen:
            continue
        seen.add(d)
        stack += [x for x in field_drvs(d, "propagatedBuildInputs") if is_hs(x) and x not in seen]
    return seen - set(rt_hs)


rules = {
    "R1 root:buildInputs+propagatedBuildInputs, then propagatedBuildInputs":
        walk(["buildInputs", "propagatedBuildInputs"], ["propagatedBuildInputs"]),
    "R2 buildInputs+propagatedBuildInputs everywhere":
        walk(["buildInputs", "propagatedBuildInputs"], ["buildInputs", "propagatedBuildInputs"]),
    "R3 inputs.drvs everywhere (Haskell drvs only)":
        walk([], [], start_from_inputs_drvs=True, step_inputs_drvs=True),
    "R4 propagatedBuildInputs only (root too)":
        walk(["propagatedBuildInputs"], ["propagatedBuildInputs"]),
    "R5 root: cabal lib+exe deps (getCabalDeps), then propagatedBuildInputs":
        walk_from_cabal_direct(),
}
for label, found in rules.items():
    # Score only what the rule ADDS to the runtime closure, against the truth libraries
    # the runtime closure does not already contain (for a dynamically linked exe, none).
    found_out = {out_of(d): name_of(d) for d in found}
    need = {k: v for k, v in truth.items() if k not in rt}
    tp = sorted(v for k, v in found_out.items() if k in need)
    fp = sorted(v for k, v in found_out.items() if k not in need)
    fn = sorted(v for k, v in need.items() if k not in found_out)
    prec = len(tp) / len(found_out) if found_out else float("nan")
    rec = len(tp) / len(need) if need else float("nan")
    print(f"[truth libs not already in the runtime closure: {len(need)}]")
    print(f"{label}\n  found={len(found_out)} true-pos={len(tp)} false-pos={len(fp)} false-neg={len(fn)}"
          f"  precision={prec:.3f} recall={rec:.3f}")
    print(f"  false positives: {fp}")
    print(f"  false negatives: {fn}")
