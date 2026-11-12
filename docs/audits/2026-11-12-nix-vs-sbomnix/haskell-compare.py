#!/usr/bin/env python3
"""waybill vs sbomnix on the recall-corpus Haskell closure targets.
Usage: python3 -I -B compare.py <classify.py> <work dir with w/<t>/> <ci dir with <t>/> <public_corpus dir> <target>...
Arms: S = sbomnix 1.9.0 runtime CycloneDX (CI artifact) and grype's findings on it (CI artifact);
      WR = waybill's committed golden, runtime-only view (scope=excluded dropped), grype with and without its VEX.
Keys are (canon id, normalized name), classify.py's join key; name collisions excluded by classify.py's rule.
patched = (CVE, name) in the waybill golden's pedigree.patches[].resolves (nixpkgs backports)."""
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("classify", sys.argv[1]); C = importlib.util.module_from_spec(spec); spec.loader.exec_module(C)
W, CI, G = sys.argv[2], sys.argv[3], sys.argv[4]
def walk(o):
    if isinstance(o, dict):
        yield o
        for v in o.values(): yield from walk(v)
    elif isinstance(o, list):
        for v in o: yield from walk(v)
def keyset(rows, hk):
    out = set()
    for r in rows:
        cls, _ = C.classify(r, hk)
        if cls == "right-product": out.add((r["canon"], C.norm_name(r["name"])))
    return out
print("target\tS_grype_rp\tS_patched\tS_osv\tWR_grype_rp\tWR_vex_rp\tWR_patched_after_vex\tWR_osv\ttruth\tWR_recall\tS_only_real\tWR_only_real")
for t in sys.argv[5:]:
    gold = json.load(open(f"{G}/{t}/cdx.json"))
    comps = [o for o in walk(gold) if isinstance(o, dict) and "purl" in o and "bom-ref" in o]
    hk = {((c.get("name") or "").lower(), c.get("version") or "") for c in comps if c["purl"].startswith("pkg:hackage/")}
    patched = {(rv["id"].upper(), C.norm_name(c.get("name"))) for c in comps for p in (c.get("pedigree") or {}).get("patches", []) for rv in (p.get("resolves") or [])}
    S = keyset(C.grype_rows(f"{CI}/{t}/S.cdx.json.grype.json"), hk)
    S_osv = keyset(C.osv_rows(f"{W}/w/{t}/S.osv.json"), hk)
    WR = keyset(C.grype_rows(f"{W}/w/{t}/wr.grype.json"), hk)
    WRV = keyset(C.grype_rows(f"{W}/w/{t}/wr.vex.grype.json"), hk)
    W_osv = keyset(C.osv_rows(f"{W}/w/{t}/wr.osv.json"), hk)
    truth = {(C.canon(f["id"], []), C.norm_name(f["name"])) for f in json.load(open(f"{CI}/{t}/recall-truth.json"))["findings"]}
    found = WRV | W_osv
    suppressed_patched = (WR - WRV) & patched
    hit = {k for k in truth if k in found or k in suppressed_patched}
    s_only = sorted((S - patched) - (WR | W_osv))
    w_only = sorted((WRV | W_osv) - S - patched)
    print("\t".join(map(str, [t, len(S), len(S & patched), len(S_osv), len(WR), len(WRV), len(WRV & patched), len(W_osv), len(truth), f"{len(hit)}/{len(truth)}", len(s_only), len(w_only)])))
    with open(f"{W}/w/{t}/diff.txt", "w") as fh:
        fh.write("S-only real (not patched, not found by waybill):\n" + "\n".join(f"  {a}\t{b}" for a, b in s_only) + "\n")
        fh.write("WR-only (found by waybill, not by sbomnix, not patched):\n" + "\n".join(f"  {a}\t{b}" for a, b in w_only) + "\n")
        fh.write("truth missed by waybill:\n" + "\n".join(f"  {a}\t{b}" for a, b in sorted(truth - hit)) + "\n")
        fh.write("sbomnix findings on patched (component, CVE):\n" + "\n".join(f"  {a}\t{b}" for a, b in sorted(S & patched)) + "\n")
