"""Spec 1503 golden review (T025): old vs new CycloneDX golden for one target.

Reads only the two documents and the measurement's members-*.tsv (which the
binary produced). Checks:
  - same component set (by bom-ref and by purl): R4, identity unchanged;
  - every component whose fields changed is a reclassified member, and only
    the allowed properties changed;
  - every added or removed edge touches a reclassified component, or (for a
    removed edge a->c) is replaced in the new graph by a path a -> r ... -> c
    through a reclassified component r (SC-003);
  - C184 edges equals the accounting's shipped figure; C184 member-roles and
    edge-ineligible-components equal the new document's own counts.
Prints a JSON summary.

usage: python3 -I -B review1503.py <old cdx> <new cdx> <members-after.tsv> <edge-accounting-after.txt>
"""
import collections
import json
import re
import sys

ROLE = "waybill:closure-member-role"
ALLOWED = {ROLE, "waybill:orphan-reason"}


def comps(doc):
    out = {}

    def walk(cs):
        for c in cs or []:
            out[c["bom-ref"]] = c
            walk(c.get("components"))

    walk(doc.get("components"))
    md = doc.get("metadata", {}).get("component")
    if md:
        out[md["bom-ref"]] = md
    return out


def props(c):
    return {p["name"]: p["value"] for p in c.get("properties", [])}


def edges(doc):
    return {(d["ref"], t) for d in doc.get("dependencies", []) for t in d.get("dependsOn", [])}


def c184(doc):
    for p in doc.get("metadata", {}).get("properties", []):
        if p["name"] == "waybill:nix-closure":
            return json.loads(p["value"])
    return None


def main(old_p, new_p, members_p, acct_p):
    old, new = json.load(open(old_p)), json.load(open(new_p))
    co, cn = comps(old), comps(new)
    res = {"components_old": len(co), "components_new": len(cn)}
    res["same_bom_refs"] = set(co) == set(cn)
    res["same_purls"] = {c.get("purl") for c in co.values()} == {c.get("purl") for c in cn.values()}

    reclass = {}
    changed_other_fields = []
    changed_props = collections.Counter()
    for ref in set(co) & set(cn):
        a, b = co[ref], cn[ref]
        pa, pb = props(a), props(b)
        if pa.get(ROLE) != pb.get(ROLE):
            reclass[ref] = (pa.get(ROLE), pb.get(ROLE))
        diff_props = {k for k in set(pa) | set(pb) if pa.get(k) != pb.get(k)}
        for k in diff_props:
            changed_props[k] += 1
        strip = lambda c: {k: v for k, v in c.items() if k not in ("properties", "components")}
        if strip(a) != strip(b):
            changed_other_fields.append(ref)
        if diff_props - ALLOWED or (diff_props and ref not in reclass):
            changed_props["!! outside reclassified/allowed: " + ref] += 1
    res["reclassified_components"] = collections.Counter(f"{a}->{b}" for a, b in reclass.values())
    res["changed_property_counts"] = changed_props
    res["components_with_non_property_changes"] = changed_other_fields

    eo, en = edges(old), edges(new)
    added, removed = en - eo, eo - en
    r = set(reclass)
    adj = collections.defaultdict(set)
    for a, b in en:
        adj[a].add(b)

    def reaches(src, dst):
        seen, stack = {src}, [src]
        while stack:
            x = stack.pop()
            if x == dst:
                return True
            for y in adj[x]:
                if y not in seen:
                    seen.add(y)
                    stack.append(y)
        return False

    unexplained_added = [e for e in added if e[0] not in r and e[1] not in r]
    removed_incident = [e for e in removed if e[0] in r or e[1] in r]
    replaced, unexplained_removed = 0, []
    for a, c in removed:
        if a in r or c in r:
            continue
        if any(x in r and reaches(x, c) for x in adj[a]):
            replaced += 1
        else:
            unexplained_removed.append((a, c))
    res.update(
        edges_old=len(eo), edges_new=len(en), added=len(added), removed=len(removed),
        added_incident=len(added) - len(unexplained_added), removed_incident=len(removed_incident),
        removed_replaced_by_path_through_reclassified=replaced,
        unexplained_added=sorted(unexplained_added)[:20], unexplained_added_count=len(unexplained_added),
        unexplained_removed=sorted(unexplained_removed)[:20], unexplained_removed_count=len(unexplained_removed),
    )

    k_old, k_new = c184(old), c184(new)
    shipped = int(re.search(r"projected, identity level \(shipped\)\s+(\d+)", open(acct_p).read()).group(1))
    roles_doc = collections.Counter(props(c).get(ROLE) for c in cn.values() if props(c).get(ROLE))
    inelig = sum(1 for c in cn.values() if props(c).get(ROLE) in ("scaffolding", "fragment"))
    res["c184"] = {
        "edges_old": k_old["edges"], "edges_new": k_new["edges"], "accounting_shipped": shipped,
        "edges_match_accounting": k_new["edges"] == shipped,
        "member_roles_old": k_old["member-roles"], "member_roles_new": k_new["member-roles"],
        "member_roles_match_document": {k: v for k, v in k_new["member-roles"].items() if v} == dict(roles_doc),
        "edge_ineligible_old": k_old["edge-ineligible-components"], "edge_ineligible_new": k_new["edge-ineligible-components"],
        "edge_ineligible_match_document": k_new["edge-ineligible-components"] == inelig,
        "identifiers_wanted": [k_old["identifiers"]["wanted"], k_new["identifiers"]["wanted"]],
        "licenses_wanted": [k_old["licenses"]["wanted"], k_new["licenses"]["wanted"]],
        "identifiers_received": [k_old["identifiers"]["received"], k_new["identifiers"]["received"]],
    }
    # SC-004: edges incident to each spec-1492 patch member (source, .patch name)
    patch = {}
    for ref, c in cn.items():
        if props(c).get(ROLE) == "source" and (c.get("name") or "").endswith(".patch"):
            patch[c["name"] + " " + ref[-12:]] = (
                sum(1 for e in eo if ref in e), sum(1 for e in en if ref in e))
    res["sc004_patch_edges_old_new"] = patch
    print(json.dumps(res, indent=1, default=lambda o: dict(o) if isinstance(o, collections.Counter) else str(o)))


if __name__ == "__main__":
    main(*sys.argv[1:5])
