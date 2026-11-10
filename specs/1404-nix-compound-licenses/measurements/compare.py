#!/usr/bin/env python3
"""Compare license fields between a before and an after run of probe.sh.

Usage: compare.py <OUT_DIR> <target>...

Per format, every component's license field is keyed by the format's own
element id, and the two runs are compared as sorted sets: gained, lost,
changed. Then everything that is not a license is compared too, with the
per-run volatile fields (timestamps, serial numbers, document namespaces and
the SPDX 3 IRIs derived from them) masked, so a fix that moved anything else
is caught rather than assumed away.
"""
import json
import re
import sys


def load(p):
    return json.load(open(p))


def cdx_licenses(doc):
    out = {}
    comps = list(doc.get("components", []))
    if "component" in doc.get("metadata", {}):
        comps.append(doc["metadata"]["component"])
    for c in comps:
        out[c.get("bom-ref") or c["name"]] = json.dumps(c.get("licenses"), sort_keys=True)
    return out


def spdx23_licenses(doc):
    return {
        p["SPDXID"]: json.dumps([p.get("licenseDeclared"), p.get("licenseConcluded")])
        for p in doc.get("packages", [])
    }


def spdx3_licenses(doc):
    g = doc.get("@graph", [])
    by_id = {e.get("spdxId"): e for e in g}
    rel = {}
    for e in g:
        if e.get("type") == "Relationship" and e.get("relationshipType") in (
            "hasDeclaredLicense", "hasConcludedLicense"):
            for t in e.get("to", []):
                lic = by_id.get(t, {})
                rel.setdefault(e["from"], []).append(
                    (e["relationshipType"],
                     lic.get("simplelicensing_licenseExpression") or lic.get("name") or t))
    return {
        e["spdxId"]: json.dumps(sorted(rel.get(e["spdxId"], [])))
        for e in g if e.get("type") == "software_Package"
    }


def diff(before, after):
    gained = sorted(k for k in after if k in before and before[k] in ("null", "[]", "[null, null]", '["NOASSERTION", "NOASSERTION"]') and after[k] != before[k])
    changed = sorted(k for k in after if k in before and after[k] != before[k] and k not in gained)
    lost_keys = sorted(set(before) ^ set(after))
    return gained, changed, lost_keys


VOLATILE = re.compile(r"(urn:uuid:[0-9a-f-]+|\d{4}-\d\d-\d\dT[0-9:.]+Z|https://[^\"]*waybill[^\"]*/[0-9a-f-]{8,}[^\"]*)")


def strip_licenses_cdx(doc):
    doc = json.loads(json.dumps(doc))
    for c in doc.get("components", []) + [doc.get("metadata", {}).get("component", {})]:
        c.pop("licenses", None)
    return doc


def strip_licenses_spdx23(doc):
    doc = json.loads(json.dumps(doc))
    for p in doc.get("packages", []):
        p.pop("licenseDeclared", None)
        p.pop("licenseConcluded", None)
    doc.pop("hasExtractedLicensingInfos", None)
    return doc


def strip_licenses_spdx3(doc):
    doc = json.loads(json.dumps(doc))
    lic_types = ("simplelicensing_LicenseExpression", "expandedlicensing_CustomLicense")
    lic_ids = {e["spdxId"] for e in doc.get("@graph", []) if e.get("type") in lic_types}
    doc["@graph"] = [
        e for e in doc.get("@graph", [])
        if e.get("type") not in lic_types
        and not (e.get("type") == "Relationship"
                 and e.get("relationshipType") in ("hasDeclaredLicense", "hasConcludedLicense"))
        and e.get("spdxId") not in lic_ids
    ]
    return doc


def canon_lines(doc):
    text = VOLATILE.sub("<v>", json.dumps(doc, sort_keys=True, indent=0))
    return sorted(text.splitlines())


def main():
    out_dir, targets = sys.argv[1], sys.argv[2:]
    for t in targets:
        b, a = f"{out_dir}/before/{t}", f"{out_dir}/after/{t}"
        for fmt, f, lic, strip in (
            ("cdx", "waybill.cdx.json", cdx_licenses, strip_licenses_cdx),
            ("spdx-2.3", "waybill.spdx23.json", spdx23_licenses, strip_licenses_spdx23),
            ("spdx-3", "waybill.spdx3.json", spdx3_licenses, strip_licenses_spdx3),
        ):
            db, da = load(f"{b}/{f}"), load(f"{a}/{f}")
            lb, la = lic(db), lic(da)
            gained, changed, keyset = diff(lb, la)
            rest_b, rest_a = canon_lines(strip(db)), canon_lines(strip(da))
            print(f"{t} {fmt}: elements={len(la)} gained={len(gained)} "
                  f"changed-or-lost={len(changed)} keyset-diff={len(keyset)} "
                  f"non-license-lines-differing={sum(1 for x, y in zip(rest_b, rest_a) if x != y) + abs(len(rest_b) - len(rest_a))}")
            for k in gained:
                print(f"    + {k}: {la[k]}")
            for k in changed:
                print(f"    ~ {k}: {lb[k]} -> {la[k]}")
            for k in keyset:
                print(f"    ? {k} only in {'after' if k in la else 'before'}")


if __name__ == "__main__":
    main()
