#!/usr/bin/env python3
"""Replicates corpus invariant I7 (orphan_explained) over one CDX doc.

Mirrors `waybill-cli/tests/corpus_harness_195/invariants/checks/
orphan_explained.rs`: a component unreachable from the subject by any
dependency edge must carry one of five explaining markers. Used by T043
to check that relocating the `--exclude-scope` filter above the m167
orphan classifier keeps #1218's guarantee under the flag.

A run that prints `UNEXPLAINED=0` everywhere looks exactly like a run
that parsed nothing, so validate the checker before trusting a zero:

    python3 -c "
    import json, sys
    d = json.load(open(sys.argv[1]))
    KEYS = {'waybill:orphan-reason', 'waybill:version-absence',
            'waybill:component-tier', 'waybill:unresolved-reason',
            'waybill:closure-role'}
    for c in d.get('components', []):
        c['properties'] = [p for p in c.get('properties', [])
                           if p['name'] not in KEYS]
    json.dump(d, open('control.cdx.json', 'w'))
    " <some.cdx.json>
    python3 i7.py control.cdx.json     # must report UNEXPLAINED > 0

On the T043 `go-opentelemetry` excluded document that control reports 3.
"""
import json, sys, collections
EXPLAINERS = [
    ("waybill:orphan-reason", None),
    ("waybill:version-absence", "by-construction"),
    ("waybill:component-tier", "file"),
    ("waybill:unresolved-reason", None),
    ("waybill:closure-role", None),
]
def props(c): return {p["name"]: p["value"] for p in c.get("properties", [])}
def ref(c): return c.get("bom-ref") or c.get("purl")
for path in sys.argv[1:]:
    d = json.load(open(path))
    root = (d.get("metadata", {}).get("component") or {})
    root_ref = root.get("bom-ref") or root.get("purl")
    adj = collections.defaultdict(list)
    for dep in d.get("dependencies", []):
        adj[dep.get("ref")] = dep.get("dependsOn") or []
    seen, queue = {root_ref}, [root_ref]
    while queue:
        n = queue.pop()
        for t in adj.get(n, []):
            if t not in seen:
                seen.add(t); queue.append(t)
    offenders = []
    for c in d.get("components", []):
        r = ref(c)
        if r == root_ref or r in seen: continue
        p = props(c)
        if any((p.get(k) is not None) if v is None else (p.get(k) == v) for k, v in EXPLAINERS):
            continue
        offenders.append(r)
    total = len([c for c in d.get("components", []) if ref(c) != root_ref])
    unreachable = len([c for c in d.get("components", []) if ref(c) != root_ref and ref(c) not in seen])
    print(f"{path}: components={total} unreachable={unreachable} UNEXPLAINED={len(offenders)}")
    for o in offenders[:8]: print("    ", o)
