#!/usr/bin/env python3
"""Classify the `meta.license` shapes of a closure's members.

Usage: classify.py <closure.json> <raw-rows.json>

<closure.json> is `nix derivation show -r` output; <raw-rows.json> is
raw-licenses.nix output. Only rows whose `.drv` basename is a closure member
count — that is the m1281 join (FR-002), so a row for a store path outside the
closure would never reach a component and is not part of the population.

"lost before #1404" means the m1281 projection (`one`, which reads only
`spdxId` / `shortName` / `fullName`) yields a declaration with neither field
for at least one element, which `to_expression` turns into nothing.
"""
import json
import sys
from collections import Counter


def basename(p):
    return p.rsplit("/", 1)[-1]


def kind(lic):
    if lic is None:
        return "null"
    if isinstance(lic, str):
        return "string"
    if isinstance(lic, list):
        return "list"
    if isinstance(lic, dict):
        return lic.get("licenseType", "simple(no licenseType)")
    return type(lic).__name__


def walk(lic, ops, leaves, depth=0):
    """Collect operators and leaves; return the tree depth."""
    if isinstance(lic, list):
        return max([walk(x, ops, leaves, depth) for x in lic] or [depth])
    if not isinstance(lic, dict):
        leaves.append(("string" if isinstance(lic, str) else "other", None))
        return depth
    t = lic.get("licenseType", "simple")
    if t == "compound":
        ops.add(lic.get("operator"))
        return max([walk(x, ops, leaves, depth + 1) for x in lic.get("licenses", [])] or [depth + 1])
    if t in ("exception", "plus"):
        ops.add(lic.get("operator"))
        d = walk(lic.get("license"), ops, leaves, depth + 1)
        if "exception" in lic:
            d = max(d, walk(lic["exception"], ops, leaves, depth + 1))
        return d
    leaves.append(("simple", lic.get("spdxId")))
    return depth


def old_projection_loses(lic):
    elems = lic if isinstance(lic, list) else ([] if lic is None else [lic])
    for e in elems:
        if isinstance(e, dict):
            if e.get("spdxId") is None and e.get("shortName") is None and e.get("fullName") is None:
                return True
        elif not isinstance(e, str):
            return True
    return False


def main():
    closure = json.load(open(sys.argv[1]))
    drvs = closure.get("derivations", closure)
    members = {basename(k) for k in drvs}
    rows = json.load(open(sys.argv[2]))
    joined = [r for r in rows if r["drv"] in members]

    by_drv = {}
    for r in joined:
        by_drv.setdefault(r["drv"], []).append(r)

    kinds = Counter()
    elem_kinds = Counter()
    lost = []
    ops_seen = Counter()
    depths = Counter()
    nospdx_leaf_in_compound = []
    for drv, rs in sorted(by_drv.items()):
        lic = rs[0]["license"]
        kinds[kind(lic)] += 1
        for e in (lic if isinstance(lic, list) else [lic]):
            if e is not None:
                elem_kinds[kind(e)] += 1
        ops, leaves = set(), []
        d = walk(lic, ops, leaves)
        if ops:
            depths[d] += 1
            for o in ops:
                ops_seen[o] += 1
            if any(s is None for _, s in leaves):
                nospdx_leaf_in_compound.append((drv, rs[0]["attr"]))
        if old_projection_loses(lic):
            lost.append((drv, rs[0]["attr"], sorted(o for o in ops if o)))

    print(json.dumps({
        "closure_members": len(members),
        "members_with_a_row": len(by_drv),
        "top_level_shape": dict(sorted(kinds.items())),
        "element_shape": dict(sorted(elem_kinds.items())),
        "members_using_an_operator": sum(depths.values()),
        "operators (members using each)": dict(sorted(ops_seen.items())),
        "operator nesting depth (members)": {str(k): v for k, v in sorted(depths.items())},
        "operator members with a leaf lacking spdxId": nospdx_leaf_in_compound,
        "lost_before_1404": len(lost),
        "lost_members": lost,
    }, indent=2))


if __name__ == "__main__":
    main()
