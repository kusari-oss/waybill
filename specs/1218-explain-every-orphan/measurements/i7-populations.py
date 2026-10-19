#!/usr/bin/env python3
"""I7's population, per target, from the committed CDX goldens.

A faithful reimplementation of
`waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs`
as of main 1311224e: same subject resolution (`metadata.component.bom-ref`),
same adjacency (`dependencies[].dependsOn`), same BFS, same five explaining
markers. Offline -- reads only the goldens, so it needs no corpus lane.

Usage: i7-populations.py <repo-root>
"""
import json
import pathlib
import sys
from collections import defaultdict, deque

EXPLAIN_ANY = ("waybill:orphan-reason", "waybill:unresolved-reason", "waybill:closure-role")
EXPLAIN_EQ = (("waybill:version-absence", "by-construction"), ("waybill:component-tier", "file"))


def prop(component, name):
    for p in component.get("properties") or []:
        if p.get("name") == name:
            return p.get("value")
    return None


def ref(component):
    return component.get("bom-ref") or component.get("purl")


def is_explained(component):
    if any(prop(component, n) is not None for n in EXPLAIN_ANY):
        return True
    return any(prop(component, n) == v for n, v in EXPLAIN_EQ)


def reachable(cdx, root):
    adj = defaultdict(list)
    for d in cdx.get("dependencies") or []:
        if (frm := d.get("ref")) is None:
            continue
        adj[frm].extend(t for t in (d.get("dependsOn") or []) if isinstance(t, str))
    seen, q = {root}, deque([root])
    while q:
        for nxt in adj.get(q.popleft(), []):
            if nxt not in seen:
                seen.add(nxt)
                q.append(nxt)
    return seen


def ecosystem(purl):
    if not purl or not purl.startswith("pkg:"):
        return "<none>"
    return purl[4:].split("/", 1)[0].split("@", 1)[0]


def check(cdx):
    components = cdx.get("components") or []
    root = (cdx.get("metadata") or {}).get("component", {}).get("bom-ref")
    if root is None:
        return "NotApplicable", [], "no resolvable subject"
    if not [c for c in components if ref(c) != root]:
        return "NotApplicable", [], "document has no component other than the subject"
    live = reachable(cdx, root)
    off = [c for c in components if (r := ref(c)) and r != root and r not in live and not is_explained(c)]
    return ("Fail" if off else "Pass"), off, ""


def main(root):
    base = pathlib.Path(root) / "waybill-cli/tests/fixtures/public_corpus"
    rows, total = [], 0
    for target in sorted(p.name for p in base.iterdir() if p.is_dir()):
        path = base / target / "cdx.json"
        if not path.is_file():
            rows.append((target, "no golden", 0, {}))
            continue
        verdict, off, why = check(json.loads(path.read_text()))
        total += len(off)
        by_eco = defaultdict(list)
        for c in off:
            by_eco[ecosystem(c.get("purl"))].append(ref(c))
        rows.append((target, verdict if not why else f"{verdict} ({why})", len(off), by_eco))

    width = max(len(r[0]) for r in rows)
    print(f"{'target':<{width}}  {'verdict':<12} {'count':>5}  populations")
    print("-" * (width + 40))
    for target, verdict, count, by_eco in rows:
        pops = ", ".join(f"{e}={len(v)}" for e, v in sorted(by_eco.items())) or "-"
        print(f"{target:<{width}}  {verdict:<12} {count:>5}  {pops}")
    print(f"\ntotal unexplained: {total}")

    print("\n=== offenders, per failing target ===")
    for target, _, count, by_eco in rows:
        if not count:
            continue
        print(f"\n{target} ({count})")
        for eco, refs in sorted(by_eco.items()):
            for r in sorted(refs):
                print(f"  [{eco}] {r}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")
