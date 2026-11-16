"""SC-008 / T021: what a root-only `complete` would change (research R11).

For each <target>/cdx.json under a regen artifact directory:
  * reachability from `metadata.component` alone, over `dependencies[]`.
    A nested component (CycloneDX `components[].components[]`) counts as
    reached when its parent is, because containment is how CycloneDX
    attaches it;
  * the document's own `waybill:graph-completeness` verdict;
  * for a document marked `complete` with root-unreachable components, the
    main module in the same document that reaches each one. A component
    that neither the root nor any main module reaches is an SC-003 failure
    (I-2), and blocks T025.

Two definitions follow the classifier, not a guess at it:
  * a file-tier component (`waybill:component-tier` = `file`) is not
    counted, as `compute_graph_completeness` filters it out (m194 US3);
  * a main module is a component carrying `waybill:demoted-from-main-module`.
    Every corpus document is scanned with `--root-name`, which retains the
    scan's main modules under that property. `waybill:workspace-member` is
    NOT a main-module marker: it is on 108 of python-flask's 108 components.

Reads emitted documents only. Usage: root_only.py <artifact-dir>
Exit status 1 when any SC-003 failure is found.
"""
import json
import pathlib
import sys

MAIN_MODULE_PROPS = {"waybill:demoted-from-main-module"}


def walk(components, parent, out):
    for c in components or []:
        out.append((c, parent))
        walk(c.get("components"), c.get("bom-ref"), out)


def props(c):
    return {p.get("name"): p.get("value") for p in c.get("properties", [])}


def reach(start, adj, parent_of):
    seen, stack = set(), [start]
    while stack:
        n = stack.pop()
        if n in seen:
            continue
        seen.add(n)
        stack.extend(adj.get(n, []))
    # Containment: a nested component is reached with its parent.
    changed = True
    while changed:
        changed = False
        for child, parent in parent_of.items():
            if parent in seen and child not in seen:
                seen.add(child)
                stack = [child]
                while stack:
                    n = stack.pop()
                    for t in adj.get(n, []):
                        if t not in seen:
                            seen.add(t)
                            stack.append(t)
                changed = True
    return seen


def analyse(path):
    doc = json.loads(path.read_text())
    root = doc["metadata"]["component"]["bom-ref"]
    pairs = []
    walk(doc.get("components"), None, pairs)
    parent_of = {c["bom-ref"]: p for c, p in pairs if p is not None and "bom-ref" in c}
    adj = {}
    for d in doc.get("dependencies", []):
        adj.setdefault(d["ref"], []).extend(d.get("dependsOn", []))
    from_root = reach(root, adj, parent_of)
    refs = [
        c["bom-ref"]
        for c, _ in pairs
        if "bom-ref" in c and props(c).get("waybill:component-tier") != "file"
    ]
    unreached = [r for r in refs if r not in from_root]
    verdict = props(doc["metadata"]).get("waybill:graph-completeness")
    mains = [c["bom-ref"] for c, _ in pairs if MAIN_MODULE_PROPS & set(props(c))]
    reached_by = {m: reach(m, adj, parent_of) for m in mains}
    via_main, nobody = {}, []
    for r in unreached:
        owners = sorted(m for m in mains if m != r and r in reached_by[m])
        if owners:
            via_main.setdefault(owners[0], []).append(r)
        elif r not in mains:
            nobody.append(r)
    return {
        "components": len(refs),
        "verdict": verdict,
        "root_unreached": len(unreached),
        "unreached_main_modules": sum(1 for r in unreached if r in mains),
        "via_main": {m: len(v) for m, v in via_main.items()},
        "nobody": nobody,
    }


def main():
    art = pathlib.Path(sys.argv[1])
    failures = 0
    print("| target | components | verdict | unreachable from root alone | of which main modules | reached by a main module | reached by nothing |")
    print("|---|---|---|---|---|---|---|")
    for cdx in sorted(art.glob("*/cdx.json")):
        r = analyse(cdx)
        reached = sum(r["via_main"].values())
        print(
            f"| {cdx.parent.name} | {r['components']} | {r['verdict']} | {r['root_unreached']} | "
            f"{r['unreached_main_modules']} | {reached} | {len(r['nobody'])} |"
        )
        if r["verdict"] == "complete" and r["nobody"]:
            failures += 1
            print(f"<!-- SC-003 failure in {cdx.parent.name}: {r['nobody'][:5]} -->")
    print(f"\nSC-003 failures (marked complete, reached by nothing): {failures}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
