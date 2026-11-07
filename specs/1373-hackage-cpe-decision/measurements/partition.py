#!/usr/bin/env python3 -I
"""Recompute C209's three counts from an emitted document, independently.

SC-001 requires the implementation's figures be checkable without reference to
the implementation. This reads the document a consumer reads and the published
ecosystem list a consumer could fetch, and applies FR-003's rule.

  python3 -I partition.py <document.cdx.json> <osv-ecosystems.txt>
"""
import collections
import json
import sys

# PURL type -> the OSV ecosystem that indexes it.
#
# Exactly the three types for which BOTH halves of FR-003 are satisfied
# against committed output: present in OSV's published ecosystem list, AND
# probed to return that ecosystem's advisories for a known-vulnerable version.
#
# A sixteen-row version of this table was written first and cut: thirteen
# types satisfied list membership alone, which asserts coverage rather than
# measuring it. Cutting them changed no figure -- across all 17 corpus targets
# zero components are classified addressable on an unprobed type, because
# `cargo`, `golang`, `maven` and the rest already carry CPEs from existing
# arms and never reach that branch.
#
# `generic` deliberately has no row: it is in neither the list nor a
# successful probe. Its single `OSV-2017-79` hit is an OSS-Fuzz finding keyed
# on a project name, which is why the rule needs both halves and not either.
#
# This table MUST stay in step with `OSV_INDEXED_PURL_TYPES` in
# `waybill-cli/src/cli/scan_cmd.rs`. If they drift, this probe and the code it
# checks agree by construction and the check is worthless.
PURL_TO_OSV = {
    "hackage": "hackage",
    "pypi": "pypi",
    "npm": "npm",
}


def main() -> None:
    doc = json.load(open(sys.argv[1]))
    declared = {l.split(":")[0].strip().lower() for l in open(sys.argv[2]) if l.strip()}

    unknown = {v for v in PURL_TO_OSV.values() if v not in declared}
    if unknown:
        sys.exit(f"FR-003 violated: not in OSV's declared list: {sorted(unknown)}")

    buckets = collections.Counter()
    detail = collections.defaultdict(collections.Counter)
    for c in doc.get("components", []):
        # File-tier entries are not in the partition, and the exclusion was
        # found by measurement rather than designed: nine corpus targets
        # disagreed with their own emitted row until it was added.
        #
        # They are emitted by the file-tier emitter *after* the resolver, so
        # the slice `fill_missing_cpes` counts never contains them --
        # `components.len()` there is smaller than the document's. They also
        # should not be counted: `utils.sh` is a file, not software a
        # vulnerability feed could address, and calling it `unaddressable`
        # would make that word mean two different things in one row.
        if c.get("type") == "file" or not c.get("purl"):
            continue
        purl = c.get("purl") or ""
        t = purl.split(":")[1].split("/")[0] if purl.startswith("pkg:") else "(none)"
        if c.get("cpe") or c.get("cpes"):
            b = "with_candidates"
        elif PURL_TO_OSV.get(t, "") in declared:
            b = "addressable_without_cpe"
        else:
            b = "unaddressable"
        buckets[b] += 1
        detail[b][t] += 1

    total = sum(
        1
        for c in doc.get("components", [])
        if c.get("type") != "file" and c.get("purl")
    )
    for b in ("with_candidates", "addressable_without_cpe", "unaddressable"):
        print(f"{b:26}{buckets[b]:>6}   {dict(detail[b])}")
    print(f"{'sum':26}{sum(buckets.values()):>6}   resolved components={total}")
    if sum(buckets.values()) != total:
        sys.exit("counts do not partition the component set")

    emitted = next((p["value"] for p in doc.get("metadata", {}).get("properties", [])
                    if p["name"] == "waybill:cpe-coverage"), None)
    if emitted:
        e = json.loads(emitted)
        agree = all(e.get(k) == buckets[k] for k in
                    ("with_candidates", "addressable_without_cpe", "unaddressable"))
        print(f"\nemitted row: {emitted}")
        print("AGREES" if agree else "DISAGREES — the row and the document do not match")
        if not agree:
            sys.exit(1)


if __name__ == "__main__":
    main()
