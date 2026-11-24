"""Spec 1651 T016 (SC-001 to SC-003): run the built `waybill sbom edit
--retier design=source` on committed corpus goldens, and model a consumer
that drops `design` and `scope: excluded` components, then everything left
unreachable from the root (kusaridev/iac#3552's rule, as Pico's code states
it; this is a model, not Pico).

usage: python3 -I -B retier_consumer_model.py WAYBILL_BIN GOLDENS_DIR SCRATCH_DIR TARGET...
"""
import json
import pathlib
import shutil
import subprocess
import sys

BIN, GOLDENS, SCRATCH = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])


def comps(d):
    out, stack = {}, list(d.get("components", []))
    while stack:
        c = stack.pop()
        out[c.get("bom-ref")] = c
        stack.extend(c.get("components", []))
    return out


def prop(c, name):
    return next((p["value"] for p in c.get("properties", []) if p["name"] == name), None)


def lost_required(d):
    cs = comps(d)
    edges = {x["ref"]: x.get("dependsOn", []) for x in d.get("dependencies", [])}
    root = d["metadata"]["component"]["bom-ref"]

    def reach(removed):
        seen, todo = {root}, [root]
        while todo:
            for t in edges.get(todo.pop(), []):
                if t not in seen and t not in removed:
                    seen.add(t)
                    todo.append(t)
        return seen

    before = reach(set())
    removed = {r for r, c in cs.items() if c.get("scope") == "excluded" or prop(c, "waybill:sbom-tier") == "design"}
    after = reach(removed)
    gone = removed | {r for r in before if r not in after}
    required = {r for r, c in cs.items() if c.get("scope", "required") == "required"}
    return len(required & gone), len(required), len(gone), len(cs)


def strip(d):
    cs = comps(d)
    for c in cs.values():
        c["properties"] = [p for p in c.get("properties", []) if p["name"] not in ("waybill:sbom-tier", "waybill:retiered-from")]
    return {r: json.dumps(c, sort_keys=True) for r, c in cs.items()}, sorted(json.dumps(x, sort_keys=True) for x in d.get("dependencies", []))


print("| Target | `required` lost, unedited | `required` lost, after `--retier design=source` | components lost after (dev-scoped) | components and edges otherwise identical | C194 `changed` = components carrying C243 |")
print("|---|---|---|---|---|---|")
for t in sys.argv[4:]:
    # A TARGET may be a path to a CycloneDX document instead of a golden name.
    src = pathlib.Path(t) if t.endswith(".json") else GOLDENS / t / "cdx.json"
    t = src.parent.name if t.endswith(".json") else t
    work = SCRATCH / t
    work.mkdir(parents=True, exist_ok=True)
    orig = work / "orig.cdx.json"
    shutil.copyfile(src, orig)
    out = work / "retiered.cdx.json"
    r = subprocess.run([BIN, "sbom", "edit", "--retier", "design=source", "-o", str(out), str(orig)], capture_output=True, text=True)
    if r.returncode != 0:
        print(f"| {t} | edit failed: {r.stderr.strip()[:200]} |")
        continue
    a, b = json.loads(orig.read_text()), json.loads(out.read_text())
    la, lb = lost_required(a), lost_required(b)
    rec = json.loads(prop(b["metadata"], "waybill:derivation"))
    changed = sum(o["changed"] for o in rec["operations"] if o["category"] == "retier")
    # Counted before `strip`, which removes the marker in place.
    markers = sum(1 for c in comps(b).values() if prop(c, "waybill:retiered-from") is not None)
    same = strip(a) == strip(b)
    print(f"| {t} | {la[0]} of {la[1]} | {lb[0]} of {lb[1]} | {lb[2]} of {lb[3]} | {same} | {changed} = {markers}: {changed == markers} |")
