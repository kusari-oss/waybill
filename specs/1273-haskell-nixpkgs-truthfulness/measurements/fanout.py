#!/usr/bin/env python3
"""Milestone 1273 Phase 0 — the fan-out below an optional mark.

Called by `sc003-public-library.sh`. Re-measures the figure SC-003 quotes as
"8 components in total on the public library #955 was filed against", which
is the one number in spec.md inherited from an issue rather than re-run.

Usage:  fanout.py <cdx.json> <flake.lock> [marked-name ...]

"Reachable only through the marked pair" is the right metric for the defect:
a withdrawal restores the mark's own component AND confers runtime
reachability on everything below it, so the blast radius is the set the root
can no longer reach once the marked components are removed from the graph.

One correction the naive count needs: a compiler-supplied (boot) package is
present in every build of the project, so attributing it to the marked pair
overstates the defect. nixpkgs marks those by binding the name to `null` in
`configuration-ghc-<series>.nix` (milestone 926, #947), and this script
reads that binding from the same per-revision cache the scan used rather
than from a list of its own.
"""

import collections
import json
import pathlib
import re
import sys


def load(cdx_path):
    doc = json.load(open(cdx_path))
    name = {}
    for c in doc.get("components", []):
        name[c.get("bom-ref") or c.get("purl")] = c["name"]
    meta = doc.get("metadata", {}).get("component", {}) or {}
    if meta.get("bom-ref"):
        name[meta["bom-ref"]] = meta.get("name")

    adj = collections.defaultdict(set)
    for edge in doc.get("dependencies", []):
        src = name.get(edge["ref"], edge["ref"])
        for target in edge.get("dependsOn") or []:
            adj[src].add(name.get(target, target))

    hackage = {
        c["name"]
        for c in doc.get("components", [])
        if (c.get("purl") or "").startswith("pkg:hackage/")
    }
    return name.get(meta.get("bom-ref")), adj, hackage


def reach(adj, seeds, blocked=frozenset()):
    seen, queue = set(), list(seeds)
    while queue:
        for nxt in adj.get(queue.pop(), ()):
            if nxt in blocked or nxt in seen:
                continue
            seen.add(nxt)
            queue.append(nxt)
    return seen


def boot_names(flake_lock):
    """Names nixpkgs binds to `null`, i.e. supplied by the compiler."""
    nodes = json.load(open(flake_lock))["nodes"].values()
    rev = next(
        v["locked"]["rev"]
        for v in nodes
        if v.get("locked", {}).get("repo") == "nixpkgs"
    )
    cache = pathlib.Path.home() / ".cache/waybill/nixpkgs" / rev
    nulled = set()
    for path in sorted(cache.glob("configuration-ghc-*.nix")):
        nulled |= set(
            re.findall(r"^\s+([A-Za-z0-9_.'-]+) = null;", path.read_text(), re.M)
        )
    return rev, nulled


def main():
    cdx, flake_lock = sys.argv[1], sys.argv[2]
    marked = set(sys.argv[3:]) or {"butcher", "monad-loops"}

    root, adj, hackage = load(cdx)
    if not root:
        print("  no root component; cannot compute reachability")
        return

    all_reach = reach(adj, [root]) & hackage
    without = reach(adj, [root], marked) & hackage
    only_via = sorted(all_reach - without)

    rev, nulled = boot_names(flake_lock)
    boot = [n for n in only_via if n in nulled]
    fanout = [n for n in only_via if n not in nulled and n not in marked]
    marked_present = sorted(marked & set(only_via))

    print(f"  nixpkgs revision read for boot names:      {rev}")
    print(f"  hackage components:                        {len(hackage)}")
    print(f"  reachable from the root:                   {len(all_reach)}")
    print(f"  reachable without traversing the marked:   {len(without)}")
    print(f"  reachable ONLY through the marked:         {len(only_via)}")
    print(f"    the marked components themselves:  {len(marked_present)} {marked_present}")
    print(f"    non-boot fan-out below them:       {len(fanout)} {fanout}")
    print(f"    compiler-supplied (boot):          {len(boot)} {boot}")
    print("      ^ present in every build of the project; not attributable")
    print(f"  ==> the defect's population: {len(marked_present) + len(fanout)} components")


if __name__ == "__main__":
    main()
