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
        # The m053 `git describe` version ladder embeds commit-count and hash
        # into the version of any component whose version is derived from the
        # repository itself. That moves on EVERY commit, so an unmasked
        # self-scan comparison reports a difference for every commit made
        # between capture and check -- which looks exactly like a regression
        # and is not one. Found while verifying SC-009 for m924.
        s = re.sub(r"(nightly\.\d+)-\d+-g[0-9a-f]{7,}", r"\1-<describe>", s)
        # Content-addressed identifiers. These are BASE32 hashes over content
        # that INCLUDES the version string masked just above, so masking the
        # version alone is not enough -- the derived ids still move. Masking
        # them is this repo's documented practice for golden diffs; a real
        # content change still shows up in the content itself.
        s = re.sub(r"SPDXRef-([A-Za-z]+)-[A-Z2-7]{12,}", r"SPDXRef-\1-<cid>", s)
        s = re.sub(r"(spdx/)[A-Z2-7]{20,}", r"\1<cid>", s)
        s = re.sub(r"(/spdx3/doc-)[A-Za-z2-7]+", r"\1<cid>", s)
        s = re.sub(r"(/(?:pkg|rel|anno|file)-)[A-Z2-7]{12,}", r"\1<cid>", s)
        return s
    return o

if __name__ == "__main__":
    print(json.dumps(mask(json.load(open(sys.argv[1]))), sort_keys=True, indent=1))
