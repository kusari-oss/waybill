#!/usr/bin/env python3
"""Of the grype findings gained on the new rows, which name a CVE that the
component's own derivation already patches (precision.py's definite-FP test)?

Usage: TARGETS_DIR=... OUT_DIR=<after arm> MEAS=<1377 measurements dir> \
       python3 -I patched-gained.py <diffsets gained tsv: target, name, version>
Reads the gained (CVE, name, version) triples from the after arm's grype JSON
for the seven row names, and prints each with patched / not-patched.
"""
import collections, json, os, sys

M = os.environ.get("MEAS") or sys.exit("set MEAS")
sys.argv = ["x"]
src = open(f"{M}/adjudicate.py").read()
exec(src.split("rows = {}")[0])
exec(open(f"{M}/analyze.py").read().split("def runtime_names")[0])

ROWS = {"cjson", "fontforge", "giflib", "libass", "libsndfile", "libssh", "unbound"}
for t in TARGETS:
    drv_of = collections.defaultdict(set)
    for c in comps(t, "sbomnix-bt"):
        for p in c.get("properties", []):
            if p["name"] == "nix:drv_path":
                drv_of[(norm(c["name"]), c.get("version"))].add(p["value"])
    p = f"{OUT}/{t}/waybill.cdx.json.grype.json"
    fs, _ = grype(p)
    counts = collections.Counter()
    for name, ver, cve in sorted(fs):
        if name not in ROWS:
            continue
        drvs = drv_of.get((name, ver)) or set()
        patches = set()
        for d in drvs:
            patches |= set(drv_patches(d))
        hit = any(patch_names_cve(drv_patches(d), cve) for d in drvs)
        counts[(name, hit)] += 1
        if hit:
            print(f"{t}\t{name}\t{ver}\t{cve}\tPATCHED-BY-NIX")
    for (name, hit), n in sorted(counts.items()):
        print(f"{t}\t{name}\t{'patched' if hit else 'not-patched'}\t{n}")
