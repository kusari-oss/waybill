import importlib.util, json, sys, collections
spec = importlib.util.spec_from_file_location("classify", sys.argv[1]); C = importlib.util.module_from_spec(spec); spec.loader.exec_module(C)
D = sys.argv[2]
def walk(o):
    if isinstance(o, dict):
        yield o
        for v in o.values(): yield from walk(v)
    elif isinstance(o, list):
        for v in o: yield from walk(v)
print("target\tS_rp\tS_collisions\tS_questionable\tW_rp\tW_collisions\tW_questionable\tS_only_rp\tof_which_patched\tremaining_real_gap\tW_only_rp")
gaps = []
for t in sys.argv[3:]:
    s, w = f"{D}/sbomnix/{t}", f"{D}/waybill/{t}"
    gold = json.load(open(f"{w}/cdx.json"))
    comps = [o for o in walk(gold) if isinstance(o, dict) and "purl" in o and "bom-ref" in o]
    hk = {((c.get("name") or "").lower(), c.get("version") or "") for c in comps if c["purl"].startswith("pkg:hackage/")}
    patched = {(rv["id"].upper(), C.norm_name(c.get("name"))) for c in comps for p in (c.get("pedigree") or {}).get("patches", []) for rv in (p.get("resolves") or [])}
    def cls(rows):
        out = collections.defaultdict(set)
        for r in rows:
            c, _ = C.classify(r, hk); out[c].add((r["canon"], C.norm_name(r["name"])))
        return out
    Sc = cls(C.grype_rows(f"{s}/buildtime.grype.json"))
    Wc = cls(C.grype_rows(f"{w}/full.vex.grype.json") + C.osv_rows(f"{w}/full.osv.json"))
    S, W = Sc["right-product"], Wc["right-product"]
    so = S - W; pat = so & patched; gap = so - patched
    gaps += [(t, a, b) for a, b in sorted(gap)]
    print("\t".join(map(str, [t, len(S), len(Sc["collision"] - S), len(Sc["questionable"] - S), len(W), len(Wc["collision"] - W), len(Wc["questionable"] - W), len(so), len(pat), len(gap), len(W - S)])))
print("\nremaining real gap (sbomnix right-product, not patched, not found by waybill):")
for g in gaps: print("  " + "\t".join(g))
