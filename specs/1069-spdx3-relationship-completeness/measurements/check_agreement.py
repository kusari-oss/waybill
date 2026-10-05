#!/usr/bin/env python3
"""#878 / m1069: per corpus target, CycloneDX compositions[] and SPDX 3
relationship completeness must agree (SC-001/SC-002), and SPDX 3 must have at
most one dependsOn relationship per (from, type, scope) (SC-003).
Usage: check_agreement.py <corpus-goldens-dir>"""
import json, os, sys
from collections import defaultdict
root = sys.argv[1]
bad = 0
for t in sorted(os.listdir(root)):
    cdx = json.load(open(f"{root}/{t}/cdx.json"))
    sp = json.load(open(f"{root}/{t}/spdx-3.json"))
    claim = defaultdict(set)
    for c in cdx.get("compositions", []):
        for d in c.get("dependencies", []):
            claim[c["aggregate"]].add(d)
    cdx_root = cdx["metadata"]["component"]["bom-ref"]
    g = sp["@graph"]
    purl = {e["spdxId"]: e.get("software_packageUrl") for e in g if "spdxId" in e}
    doc = next(e for e in g if e.get("type") == "SpdxDocument")
    root_iri = doc["rootElement"][0]
    deps = [e for e in g if e.get("relationshipType") == "dependsOn"]
    shape = defaultdict(int)
    q = defaultdict(set)
    for e in deps:
        shape[(e["from"], e["type"], e.get("scope"))] += 1
        who = cdx_root if e["from"] == root_iri else purl.get(e["from"])
        q[who].add(e.get("completeness", "-"))
    problems = []
    if any(n > 1 for n in shape.values()):
        problems.append("shape: >1 relationship per (from,type,scope)")
    for u in claim["unknown"]:
        if not q.get(u) or not q[u] <= {"incomplete", "noAssertion"}:
            problems.append(f"unknown {u} -> {sorted(q.get(u, []))}")
    for who, cs in q.items():
        for c in cs:
            if c == "complete" and who not in claim["complete"]:
                problems.append(f"{who} complete only in SPDX 3")
            if c in ("incomplete", "noAssertion") and who not in claim["unknown"]:
                problems.append(f"{who} {c} only in SPDX 3")
            if c == "-" and (who in claim["complete"] or who in claim["unknown"]):
                problems.append(f"{who} claimed by CycloneDX, unqualified in SPDX 3")
    counts = defaultdict(int)
    for e in deps:
        counts[e.get("completeness", "-")] += 1
    na = sum(1 for e in deps if e["to"] == ["NoAssertionElement"])
    status = "OK " if not problems else "BAD"
    bad += bool(problems)
    print(f"{status} {t}: cdx unknown={len(claim['unknown'])} complete={len(claim['complete'])} | spdx3 dependsOn={len(deps)} {dict(sorted(counts.items()))} NoAssertionElement={na}")
    for p in problems[:5]:
        print("     ", p)
sys.exit(1 if bad else 0)
