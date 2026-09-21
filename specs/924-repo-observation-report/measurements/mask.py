#!/usr/bin/env python3
"""Mask per-run nondeterministic fields so two SBOMs are comparable.

Used by T007 and T043 for SC-009. The fields below are volatile BY DESIGN --
verified in m923 T009, where a structural diff of two offline CycloneDX runs
showed exactly two differing leaves out of a 4.2 MB document.
"""
import json, sys, re

def mask(o):
    if isinstance(o, dict):
        out = {}
        for k, v in o.items():
            if k in ("serialNumber", "timestamp", "created", "creationInfo"):
                out[k] = "<masked>"
            elif k == "name" and isinstance(v, str) and v.startswith("waybill-"):
                out[k] = "<masked-tool>"
            else:
                out[k] = mask(v)
        return out
    if isinstance(o, list):
        return [mask(x) for x in o]
    if isinstance(o, str):
        s = re.sub(r"\d{4}-\d{2}-\d{2}T[\d:.]+Z?", "<ts>", o)
        s = re.sub(r"urn:uuid:[0-9a-fA-F-]{36}", "<uuid>", s)
        return s
    return o

if __name__ == "__main__":
    print(json.dumps(mask(json.load(open(sys.argv[1]))), sort_keys=True, indent=1))
