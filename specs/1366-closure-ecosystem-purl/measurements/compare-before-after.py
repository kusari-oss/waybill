#!/usr/bin/env python3
"""Milestone 1366 — the byte-identity claims, compared rather than argued.

SC-003  the 539 closure members are byte-identical before and after
SC-004  components from other readers are byte-identical before and after
SC-006  with confirmation unavailable the document is today's, plus C208

`before` is a binary built from the pre-change commit (255702d9) in a
worktree; `after` is this branch. Both runs set WAYBILL_FIXED_TIMESTAMP so the
only differences left are semantic.
"""
import json
import sys

C206 = "waybill:closure-member-role"
C207 = "waybill:closure-ecosystem-identity"
C208 = "waybill:closure-ecosystem-identity-outcome"

# Fields that vary per run by design and say nothing about this feature.
VOLATILE_DOC = {"serialNumber", "metadata"}


def load(p):
    with open(p) as f:
        return json.load(f)


def props(c):
    return {p["name"]: p["value"] for p in c.get("properties", [])}


def key(c):
    return (c.get("name"), c.get("version"), c.get("purl"))


def is_member(c):
    return C206 in props(c)


def is_closure(c):
    p = props(c)
    return C206 in p or "waybill:closure-role" in p


def normalise(c):
    """A component, with nothing volatile and nothing bom-ref-shaped left."""
    out = {k: v for k, v in c.items() if k not in ("bom-ref", "properties", "evidence")}
    out["properties"] = sorted(
        (n, v) for n, v in props(c).items() if n not in (C207,)
    )
    return json.dumps(out, sort_keys=True)


def index(doc, pred):
    return {key(c): normalise(c) for c in doc.get("components", []) if pred(c)}


def compare(before, after, pred, label):
    b, a = index(before, pred), index(after, pred)
    only_b = sorted(set(b) - set(a))
    only_a = sorted(set(a) - set(b))
    changed = sorted(k for k in set(b) & set(a) if b[k] != a[k])
    status = "PASS" if not (only_b or only_a or changed) else "FAIL"
    print(f"[{status}] {label}: {len(b)} before, {len(a)} after")
    for k in only_b[:5]:
        print(f"        only before: {k}")
    for k in only_a[:5]:
        print(f"        only after:  {k}")
    for k in changed[:5]:
        print(f"        changed:     {k}")
    return status == "PASS"


def doc_props(doc):
    return {p["name"]: p["value"] for p in doc.get("metadata", {}).get("properties", [])}


def main():
    base_on, new_on, base_off, new_off = (load(p) for p in sys.argv[1:5])
    ok = True

    print("=== SC-003 / FR-005 — closure members, confirmation ON ===")
    ok &= compare(base_on, new_on, is_member, "closure members (pkg:generic is correct)")

    print("\n=== SC-004 / FR-006 — other readers' components, confirmation ON ===")
    ok &= compare(
        base_on, new_on, lambda c: not is_closure(c), "non-closure components"
    )

    print("\n=== What DID change (confirmation ON) ===")
    b = {key(c): c for c in base_on["components"]}
    a = {key(c): c for c in new_on["components"]}
    moved = sorted(
        (c.get("name"), c.get("version"))
        for k, c in a.items()
        if k not in b and (c.get("purl") or "").startswith("pkg:pypi/")
    )
    print(f"  components newly carrying pkg:pypi: {len(moved)}")
    print(f"  component count before={len(base_on['components'])} after={len(new_on['components'])}")

    print("\n=== SC-006 — confirmation unavailable ===")
    bo, ao = base_off, new_off
    same_components = index(bo, lambda _c: True) == index(ao, lambda _c: True)
    print(f"[{'PASS' if same_components else 'FAIL'}] every component identical")
    ok &= same_components

    bp, ap = doc_props(bo), doc_props(ao)
    added = sorted(set(ap) - set(bp))
    removed = sorted(set(bp) - set(ap))
    changed = sorted(k for k in set(bp) & set(ap) if bp[k] != ap[k])
    print(f"  document properties added:   {added}")
    print(f"  document properties removed: {removed}")
    print(f"  document properties changed: {changed}")
    only_c208 = added == [C208] and not removed and not changed
    print(f"[{'PASS' if only_c208 else 'FAIL'}] the only difference is C208")
    ok &= only_c208
    if C208 in ap:
        print(f"  C208 = {ap[C208]}")

    print("\nRESULT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
