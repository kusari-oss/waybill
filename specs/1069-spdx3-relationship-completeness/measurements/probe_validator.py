#!/usr/bin/env python3
"""#878 / m1069: does the pinned SPDX 3 validator accept the shapes this
feature emits? Builds variants of a committed corpus golden and validates
each. Usage: probe_validator.py <validator> <spdx-3.json>"""
import copy, json, subprocess, sys, tempfile, os

validator, golden = sys.argv[1], sys.argv[2]
# Corpus goldens mask document IRIs as `<masked>`, which is not a valid URI.
base = json.loads(open(golden).read().replace("<masked>", "masked"))
graph = base["@graph"]

def is_dep(e):
    return e.get("type") in ("Relationship", "LifecycleScopedRelationship") and e.get("relationshipType") == "dependsOn"

def grouped(doc, completeness=None):
    d = copy.deepcopy(doc)
    deps = [e for e in d["@graph"] if is_dep(e)]
    rest = [e for e in d["@graph"] if not is_dep(e)]
    by = {}
    for e in deps:
        by.setdefault((e["from"], e["type"], e.get("scope")), []).append(e)
    for (frm, typ, scope), es in by.items():
        g = copy.deepcopy(es[0])
        g["to"] = sorted({t for e in es for t in e["to"]})
        g["spdxId"] = es[0]["spdxId"] + "-grouped"
        if completeness:
            g["completeness"] = completeness
        rest.append(g)
    d["@graph"] = rest
    return d

def with_noassertion(doc, iri):
    d = copy.deepcopy(doc)
    pkg = next(e for e in d["@graph"] if e.get("type") == "software_Package")
    rel = {"type": "Relationship", "spdxId": pkg["spdxId"] + "-noassert",
           "creationInfo": pkg["creationInfo"], "from": pkg["spdxId"],
           "to": [iri], "relationshipType": "dependsOn", "completeness": "noAssertion"}
    d["@graph"].append(rel)
    return d

def run(name, doc):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(doc, f)
    r = subprocess.run([validator, "-j", f.name], capture_output=True, text=True)
    os.unlink(f.name)
    tail = (r.stdout + r.stderr).strip().splitlines()
    print(f"{name}: exit={r.returncode} {tail[-1][:160] if tail else ''}")

run("baseline golden", base)
run("grouped, no completeness", grouped(base))
for c in ("complete", "incomplete", "noAssertion"):
    run(f"grouped, completeness={c}", grouped(base, c))
run("grouped, completeness=bogus (control: must fail)", grouped(base, "bogus"))
for iri in ("NoAssertionElement", "https://spdx.org/rdf/3.0.1/terms/Core/NoAssertionElement"):
    run(f"dependsOn -> {iri}", with_noassertion(grouped(base, "complete"), iri))
