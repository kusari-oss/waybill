#!/usr/bin/env python3
"""Per-target versionless populations in the four seeded goldens.

Answers one question the spec assumed rather than measured: if the Pants
resolve anchors left the document, would I8's precondition stop being met?
I8 applies when at least one component has an absent or empty version, and
passes when every such component carries an accepted explanation. So the
answer per target is "yes" only if the anchors are the ONLY versionless
components it has.

The accepted-explanation set mirrors `is_explained` in
waybill-cli/tests/corpus_harness_195/invariants/checks/versionless_explained.rs.

Run: python3 specs/1217-must-pass-registry/measurements/versionless-populations.py
"""
import json
import pathlib

GOLDENS = pathlib.Path("waybill-cli/tests/fixtures/public_corpus")
SEEDED = [
    "pants-clojure-polyglot",
    "pants-example-django",
    "pants-example-jvm",
    "pants-example-python",
]
# Keys `is_explained` accepts, in its order.
C197 = ("waybill:version-absence", "by-construction")
TIER_FILE = ("waybill:component-tier", "file")
OTHER_KEYS = (
    "waybill:unresolved-reason",
    "waybill:haskell-version-unresolved-reason",
    "waybill:closure-role",
)


def props(component):
    return {p["name"]: p["value"] for p in component.get("properties", [])}


def versionless(component):
    return not (component.get("version") or "").strip()


EXPECTATIONS = pathlib.Path(
    "waybill-cli/tests/corpus_harness_195/expectations.rs"
)

for target in SEEDED:
    doc = json.loads((GOLDENS / target / "cdx.json").read_text())
    components = doc.get("components", [])
    vless = [c for c in components if versionless(c)]
    anchors = [c for c in vless if props(c).get(C197[0]) == C197[1]]
    survivors = [c for c in vless if c not in anchors]
    explained = [
        c
        for c in survivors
        if props(c).get(TIER_FILE[0]) == TIER_FILE[1]
        or any(k in props(c) for k in OTHER_KEYS)
    ]
    # What I8 would report with the anchors removed from the document.
    if not survivors:
        would_be = "NotApplicable  <- the registry's trap fires here"
    elif len(explained) == len(survivors):
        would_be = "Pass           <- trap does NOT fire; cell stays green"
    else:
        would_be = "Fail"
    print(f"{target}")
    print(f"  components={len(components)} versionless={len(vless)} anchors={len(anchors)}")
    print(f"  anchors: {sorted(c['name'] for c in anchors)}")
    print(f"  versionless survivors if anchors leave: {len(survivors)} "
          f"({len(explained)} explained)")
    print(f"  I8 without anchors: {would_be}")

# Second question, for issue #1230: is each anchor's PRESENCE asserted
# anywhere? The registry catches a cell that stops applying; it cannot catch
# an anchor leaving a document that stays applicable for other reasons. A
# claim about one target belongs in the per-target expectation layer, so that
# is where the assertion is looked for.
print()
print("anchor presence asserted in expectations.rs (issue #1230):")
source = EXPECTATIONS.read_text()
for target in SEEDED:
    doc = json.loads((GOLDENS / target / "cdx.json").read_text())
    anchors = [
        c
        for c in doc.get("components", [])
        if props(c).get(C197[0]) == C197[1]
    ]
    for anchor in anchors:
        purl = anchor.get("purl", "")
        mark = "yes" if purl in source else "NO"
        print(f"  {mark:>3}  {target:24} {purl}")
