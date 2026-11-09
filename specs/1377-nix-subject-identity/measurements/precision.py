#!/usr/bin/env python3
"""Precision pass: of EVERY finding each scanner reports on each tool's SBOM,
how many name a CVE that the component's own derivation already patches?

Those are definite false positives. Absence of a CVE-named patch does not make
a finding true (a fix can land under an unrelated patch name, or a version
bump), so the 'not-patched' remainder is an upper bound on true positives,
not a count of them.

Components are mapped to derivations through sbomnix's `nix:drv_path`
property, keyed on (normalised name, version) -- both tools describe the same
derivation set for a target.
"""
import json, re, collections, sys

sys.argv = ["x"]
HERE = __import__("os").path.dirname(__import__("os").path.abspath(__file__))
src = open(f"{HERE}/adjudicate.py").read()
exec(src.split("rows = {}")[0])          # reuse norm(), drv_patches(), patch_names_cve()

READERS = {}
exec(open(f"{HERE}/analyze.py").read().split("def runtime_names")[0])   # grype/trivy/osv readers
READERS = {"grype": grype, "trivy": trivy, "osv": osv}

tot = collections.Counter()
print(f"{'target':8} {'scanner':6} {'tool':10} | {'findings':>8} {'patched(FP)':>11} {'FP%':>5} | "
      f"runtime: {'findings':>8} {'patched':>7}")
for t in TARGETS:
    drv_of = collections.defaultdict(set)
    for c in comps(t, "sbomnix-bt"):
        for p in c.get("properties", []):
            if p["name"] == "nix:drv_path":
                drv_of[(norm(c["name"]), c.get("version"))].add(p["value"])
                drv_of[(norm(c["name"]), None)].add(p["value"])
    rt = set(r[t]["grype"].get("runtime_names") or [])
    for sc in ("grype", "trivy", "osv"):
        for tool in ("sbomnix-bt", "waybill"):
            p = f"{OUT}/{t}/{tool}.cdx.json.{sc}.json"
            if not os.path.exists(p):
                continue
            fs, _ = READERS[sc](p)
            if not fs:
                continue
            fp = rfs = rfp = 0
            for name, ver, cve in fs:
                drvs = drv_of.get((name, ver)) or drv_of.get((name, None)) or set()
                hit = any(patch_names_cve(drv_patches(d), cve) for d in drvs)
                fp += hit
                if name in rt:
                    rfs += 1; rfp += hit
            tot[(tool, sc, "f")] += len(fs); tot[(tool, sc, "fp")] += fp
            tot[(tool, sc, "rf")] += rfs; tot[(tool, sc, "rfp")] += rfp
            print(f"{t:8} {sc:6} {tool:10} | {len(fs):>8} {fp:>11} {100*fp/len(fs):>4.0f}% | "
                  f"         {rfs:>8} {rfp:>7}")
print("\nTOTALS (sum over targets; shared build-closure findings counted once per target)")
for tool in ("sbomnix-bt", "waybill"):
    for sc in ("grype", "trivy", "osv"):
        f = tot[(tool, sc, "f")]
        if f:
            print(f"  {tool:10} {sc:6} findings={f:5} patched-FP={tot[(tool,sc,'fp')]:4} "
                  f"({100*tot[(tool,sc,'fp')]/f:.0f}%)  runtime findings={tot[(tool,sc,'rf')]:4} "
                  f"runtime patched-FP={tot[(tool,sc,'rfp')]}")
