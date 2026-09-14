#!/usr/bin/env python3
"""Probe: does `waybill:graph-completeness` ever say `complete` about a
graph that is actually well-connected?

Two independent checks, deliberately not sharing code with waybill:

  (A) corpus-wide — cross-tabulate the m770 quality-corpus report's
      independently-measured `flat` against the document's own
      self-reported `graph_completeness`.
  (B) single-document — count components carrying
      `waybill:orphan-reason` in a CDX file that declares itself
      `complete`. This needs no external baseline: it is a
      contradiction inside one file.

Usage:
  probe_completeness.py corpus <run-*.json>
  probe_completeness.py doc    <cdx.json> [cdx.json ...]
"""
import json, sys, collections


def prop(obj, name):
    for p in obj.get("properties", []) or []:
        if p["name"] == name:
            return p["value"]
    return None


def corpus(path):
    d = json.load(open(path))
    ms = d["measurements"]
    print(f"{'target':<26} {'edges':>6} {'depth':>6} {'flat':>6} {'self-report':>12}")
    print("-" * 60)
    for m in sorted(ms, key=lambda x: x["name"]):
        flag = "  <-- flat yet complete" if m.get("flat") and m.get("graph_completeness") == "complete" else ""
        print(f"{m['name']:<26} {str(m.get('edges')):>6} {str(m.get('max_depth')):>6} "
              f"{str(m.get('flat')):>6} {str(m.get('graph_completeness')):>12}{flag}")
    comp = [m for m in ms if m.get("graph_completeness") == "complete"]
    nonflat = [m["name"] for m in comp if not m.get("flat")]
    print(f"\n'complete' fired {len(comp)}/{len(ms)} times; "
          f"{sum(1 for m in comp if m.get('flat'))} of those on a FLAT graph.")
    print(f"well-connected graphs earning 'complete': {nonflat or 'NONE'}")
    return 1 if comp and not nonflat else 0


def doc(paths):
    rc = 0
    for path in paths:
        d = json.load(open(path))
        comps = d.get("components", [])
        gc = prop(d.get("metadata", {}), "waybill:graph-completeness")
        orph = [c for c in comps if prop(c, "waybill:orphan-reason")]
        tiers = collections.Counter(prop(c, "waybill:sbom-tier") for c in comps)
        cov = prop(d.get("metadata", {}), "waybill:go-transitive-coverage")
        print(f"=== {path} ===")
        print(f"  components={len(comps)}  sbom-tiers={dict(tiers)}")
        print(f"  marked waybill:orphan-reason : {len(orph)}")
        print(f"  go-transitive-coverage       : {cov}")
        print(f"  graph-completeness           : {gc}")
        if gc == "complete" and orph:
            print(f"  CONTRADICTION: declares 'complete' while marking "
                  f"{len(orph)}/{len(comps)} components as orphans")
            rc = 1
        if gc == "complete" and cov == "unknown":
            print("  CONTRADICTION: declares 'complete' while go coverage is 'unknown'")
            rc = 1
        print()
    return rc


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__); sys.exit(2)
    sys.exit(corpus(sys.argv[2]) if sys.argv[1] == "corpus" else doc(sys.argv[2:]))
