import importlib.util, json, sys, collections
spec = importlib.util.spec_from_file_location("classify", sys.argv[1]); C = importlib.util.module_from_spec(spec); spec.loader.exec_module(C)
D = sys.argv[2]
def walk(o):
    if isinstance(o, dict):
        yield o
        for v in o.values(): yield from walk(v)
    elif isinstance(o, list):
        for v in o: yield from walk(v)
def keys(p): return {(r["canon"], C.norm_name(r["name"])) for r in C.grype_rows(p)}
def osvkeys(p): return {(r["canon"], C.norm_name(r["name"])) for r in C.osv_rows(p)}
print("target\tmode\tS\tW_vex\tW_osv\tboth\tS_only\tS_only_patched\tS_only_W_has_no_cpe\tS_only_W_has_cpe\tS_only_W_lacks\tW_only")
agg = collections.Counter()
for t in sys.argv[3:]:
    s, w = f"{D}/sbomnix/{t}", f"{D}/waybill/{t}"
    gold = json.load(open(f"{w}/cdx.json"))
    comps = [o for o in walk(gold) if isinstance(o, dict) and "purl" in o and "bom-ref" in o]
    patched = {(rv["id"].upper(), C.norm_name(c.get("name"))) for c in comps for p in (c.get("pedigree") or {}).get("patches", []) for rv in (p.get("resolves") or [])}
    by_name = collections.defaultdict(list)
    for c in comps: by_name[C.norm_name(c.get("name"))].append(c)
    for mode, sd, wd in (("runtime", "runtime", "wr"), ("buildtime", "buildtime", "full")):
        S = keys(f"{s}/{sd}.grype.json"); WV = keys(f"{w}/{wd}.vex.grype.json"); WO = osvkeys(f"{w}/{wd}.osv.json")
        W = WV | WO
        s_only = S - W
        pat = {k for k in s_only if k in patched}
        rest = s_only - pat
        nocpe = {k for k in rest if by_name.get(k[1]) and not any(c.get("cpe") for c in by_name[k[1]])}
        hascpe = {k for k in rest if by_name.get(k[1]) and any(c.get("cpe") for c in by_name[k[1]])}
        lacks = {k for k in rest if not by_name.get(k[1])}
        print("\t".join(map(str, [t, mode, len(S), len(WV), len(WO), len(S & W), len(s_only), len(pat), len(nocpe), len(hascpe), len(lacks), len(W - S)])))
        if mode == "buildtime":
            for k in nocpe: agg[("nocpe", k[1])] += 1
            for k in hascpe: agg[("hascpe", k[1])] += 1
            for k in lacks: agg[("lacks", k[1])] += 1
json.dump({f"{a}|{b}": n for (a, b), n in agg.items()}, open(f"{D}/agg.json", "w"), indent=1, sort_keys=True)
