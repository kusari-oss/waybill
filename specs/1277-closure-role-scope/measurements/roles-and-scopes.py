#!/usr/bin/env python3
"""Role and scope distribution across the committed corpus goldens.

Reads committed goldens only -- no scan, no network, no nix. Run with -I so a
planted module beside the goldens cannot be imported.

    python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py
    python3 -I .../roles-and-scopes.py --census    # the per-target scope census

`--census` prints one row per corpus target: components, how many carry a
CycloneDX `scope`, which values, and how many carry the C203 classification
source. It is the instrument for T003's before-census and T049's
after-census, so both read the same code and a diff of the two outputs is a
measurement rather than a comparison of two hand-written tables.

Package coordinates printed here are real (hspec, QuickCheck, happy): this is
a measurement of real corpus targets, not a fixture. Fixtures use synthetic
`waybill-fixture-*` names (#285, #640).
"""
import collections
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
CORPUS = ROOT / "waybill-cli/tests/fixtures/public_corpus"
ROLE = "waybill:closure-role"


def props(obj):
    return {p["name"]: p["value"] for p in obj.get("properties", [])}


CLASSIFICATION_SOURCE = "waybill:scope-classification-source"


def census():
    """One row per corpus target: scope and classification-source counts."""
    rows = []
    for t in sorted(p.name for p in CORPUS.iterdir() if p.is_dir()):
        cdx = CORPUS / t / "cdx.json"
        if not cdx.exists():
            continue
        doc = json.loads(cdx.read_text())
        comps = doc.get("components", [])
        scopes = collections.Counter()
        roles = collections.Counter()
        src = collections.Counter()
        for c in comps:
            pr = props(c)
            if c.get("scope"):
                scopes[c["scope"]] += 1
            if pr.get(ROLE):
                roles[pr[ROLE]] += 1
            if pr.get(CLASSIFICATION_SOURCE):
                src[pr[CLASSIFICATION_SOURCE]] += 1
        rows.append((t, len(comps), sum(roles.values()), sum(scopes.values()),
                     dict(scopes), sum(src.values()), dict(src)))
    w = max(len(r[0]) for r in rows)
    print("| %-*s | comps | roles | scoped | scope values | C203 | C203 values |" % (w, "target"))
    print("|%s|-------|-------|--------|--------------|------|-------------|" % ("-" * (w + 2)))
    for t, n, nr, ns, sv, nc, cv in rows:
        print("| %-*s | %5d | %5d | %6d | %-12s | %4d | %-11s |"
              % (w, t, n, nr, ns, (sv or "-"), nc, (cv or "-")))
    tot = lambda i: sum(r[i] for r in rows)
    print("\ntotals: %d components, %d with a role, %d scoped, %d with C203, across %d targets"
          % (tot(1), tot(2), tot(3), tot(5), len(rows)))
    return 0


def main():
    if "--census" in sys.argv:
        return census()
    targets = sorted(p.name for p in CORPUS.iterdir() if p.is_dir())
    with_roles, without = [], []
    for t in targets:
        cdx = CORPUS / t / "cdx.json"
        if not cdx.exists():
            continue
        doc = json.loads(cdx.read_text())
        comps = doc.get("components", [])
        roles = collections.Counter()
        hackage = collections.Counter()
        pairs = collections.Counter()
        for c in comps:
            p = props(c)
            r = p.get(ROLE)
            if r is None:
                continue
            roles[r] += 1
            if (c.get("purl") or "").startswith("pkg:hackage/"):
                hackage[r] += 1
            if c.get("scope"):
                pairs[(r, c["scope"])] += 1
        (with_roles if roles else without).append(t)
        if not roles:
            continue
        print(f"### {t}: {len(comps)} components, {sum(roles.values())} with a closure-role")
        for r, n in sorted(roles.items()):
            print(f"    {r:16} {n:5}   pkg:hackage of those: {hackage[r]}")
        print(f"    role+scope pairs present today: {dict(pairs) or 'NONE'}")
        print()
        print("    components already carrying a scope:")
        for c in comps:
            if not c.get("scope"):
                continue
            p = props(c)
            print(f"      {c.get('purl','?'):46} scope={c['scope']:9} "
                  f"role={p.get(ROLE,'-'):14} lifecycle={p.get('waybill:lifecycle-scope','-')}")
        print()
        print("    pkg:hackage in a tooling/unreferenced role (the I9 overlap candidates):")
        for c in comps:
            p = props(c)
            if p.get(ROLE) in ("build-tooling", "unreferenced") and \
                    (c.get("purl") or "").startswith("pkg:hackage/"):
                print(f"      {c.get('purl'):46} role={p[ROLE]}")
        print()
    print(f"targets WITH closure roles ({len(with_roles)}): {', '.join(with_roles)}")
    print(f"targets WITHOUT ({len(without)}): {len(without)} -- the byte-identity control")
    return 0


if __name__ == "__main__":
    sys.exit(main())
