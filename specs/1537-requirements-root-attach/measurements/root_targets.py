"""Spec 1537 contract I-1 probe: the root's direct targets per format.

Usage:
  root_targets.py <cdx.json> <spdx23.json> <spdx3.json>   -> per-format counts, diff
  root_targets.py --dir <dir>   -> every <target>/{cdx,spdx-2.3,spdx-3}.json under dir

Targets are PURLs without qualifiers, extracted by the same rules as the
`root_targets` helpers in waybill-cli/tests/declared_requirements.rs:
  CycloneDX  dependsOn of metadata.component's bom-ref (nested bom-refs mapped to purl)
  SPDX 2.3   root DEPENDS_ON x, or scope-typed x *_DEPENDENCY_OF root (root = DESCRIBES target)
  SPDX 3     dependsOn relationships from SpdxDocument.rootElement[0]
Exit status 1 if any document set disagrees.
"""
import json
import pathlib
import sys


def strip(p):
    return p.split("?")[0] if isinstance(p, str) else p


def cdx(doc):
    root = doc.get("metadata", {}).get("component", {}).get("bom-ref")
    purl_of, stack = {}, list(doc.get("components", []))
    while stack:
        c = stack.pop()
        if "bom-ref" in c and c.get("purl"):
            purl_of[c["bom-ref"]] = c["purl"]
        stack.extend(c.get("components", []))
    out = set()
    for d in doc.get("dependencies", []):
        if d.get("ref") == root:
            out.update(strip(purl_of.get(t, t)) for t in d.get("dependsOn", []))
    return out


def spdx23(doc):
    rels = doc.get("relationships", [])
    roots = [r["relatedSpdxElement"] for r in rels if r["relationshipType"] == "DESCRIBES" and r["spdxElementId"] == "SPDXRef-DOCUMENT"]
    if not roots:
        return set()
    root = roots[0]
    purl_of = {}
    for p in doc.get("packages", []):
        refs = [e["referenceLocator"] for e in p.get("externalRefs", []) if e.get("referenceType") == "purl"]
        purl_of[p["SPDXID"]] = refs[0] if refs else p["SPDXID"]
    out = set()
    for r in rels:
        t = r["relationshipType"]
        if r["spdxElementId"] == root and t == "DEPENDS_ON":
            out.add(strip(purl_of.get(r["relatedSpdxElement"], r["relatedSpdxElement"])))
        if r["relatedSpdxElement"] == root and t.endswith("_DEPENDENCY_OF"):
            out.add(strip(purl_of.get(r["spdxElementId"], r["spdxElementId"])))
    return out


def spdx3(doc):
    g = doc.get("@graph", [])
    docs = [e for e in g if e.get("type") == "SpdxDocument"]
    if not docs or not docs[0].get("rootElement"):
        return set()
    root = docs[0]["rootElement"][0]
    purl_of = {e["spdxId"]: e["software_packageUrl"] for e in g if e.get("spdxId") and e.get("software_packageUrl")}
    out = set()
    for e in g:
        if e.get("type") in ("Relationship", "LifecycleScopedRelationship") and e.get("relationshipType") == "dependsOn" and e.get("from") == root:
            to = e.get("to", [])
            out.update(strip(purl_of.get(t, t)) for t in (to if isinstance(to, list) else [to]))
    return out


def compare(name, c, s2, s3):
    a, b, d = cdx(json.loads(c.read_text())), spdx23(json.loads(s2.read_text())), spdx3(json.loads(s3.read_text()))
    ok = a == b == d
    print(f"{name}\tcdx={len(a)}\tspdx23={len(b)}\tspdx3={len(d)}\t{'agree' if ok else 'DIFFER'}")
    if not ok:
        for label, x in (("cdx-only", a - b - d), ("spdx23-only", b - a - d), ("spdx3-only", d - a - b),
                         ("missing-from-cdx", (b & d) - a), ("missing-from-spdx23", (a & d) - b), ("missing-from-spdx3", (a & b) - d)):
            for t in sorted(x):
                print(f"  {label}\t{t}")
    return ok


if sys.argv[1] == "--dir":
    base = pathlib.Path(sys.argv[2])
    good = True
    for t in sorted(p for p in base.iterdir() if p.is_dir()):
        files = [t / "cdx.json", t / "spdx-2.3.json", t / "spdx-3.json"]
        if all(f.exists() for f in files):
            good &= compare(t.name, *files)
    sys.exit(0 if good else 1)
sys.exit(0 if compare(sys.argv[1], *(pathlib.Path(a) for a in sys.argv[1:4])) else 1)
