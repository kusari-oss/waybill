#!/usr/bin/env python3
"""Recall of waybill's runtime-only view (WR) against a per-package vulnerability truth.

Run: python3 -I -B recall.py <recall-corpus-dir>
Writes out/recall.json and out/<pkg>/{truth,classified}.tsv. Reads only emitted SBOMs, scanner
JSON, storecheck/ and cachecheck/ files written by the other scripts.

Join key: (canonical vuln id, normalized package name), with canon()/norm_name() from classify.py
(copied verbatim there from specs/1377-nix-subject-identity/measurements/analyze.py; version is NOT part of the key).

Truth = (a) grype findings on S (sbomnix runtime) classified "right-product"
      ∪ (b) osv-scanner HSEC advisories found on any arm (W, WR, S).

Classification rule for a grype finding (applied identically to S, W, WR; implemented in classify.py):
  haskell   = the matched component is a Haskell package: its purl is pkg:hackage, OR its
              lower-cased name+version equals a pkg:hackage component in that package's W document.
  if not haskell                                   -> right-product
  elif name in BUNDLED_C                           -> questionable (bundled C sources)
  elif description/related descriptions mention
       'haskell' or 'hackage'                      -> right-product
  else                                             -> collision (excluded; excerpt recorded)
"""
import collections, json, os, re, sys

R = sys.argv[1]
OUT = f"{R}/out"
PKGS = "shellcheck pandoc hledger cachix hlint hadolint postgrest haskell-language-server".split()
# The classifier and the identity helpers live in classify.py, the committed
# truth rule (research R7). Loaded by path: `python3 -I` does not put this
# script's directory on sys.path, so a plain `import classify` would fail --
# and would be unsafe if it did not.
import importlib.util, pathlib
_spec = importlib.util.spec_from_file_location("classify", pathlib.Path(__file__).with_name("classify.py"))
_cls = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_cls)
BUNDLED_C, norm_name, canon, ptype = _cls.BUNDLED_C, _cls.norm_name, _cls.canon, _cls.ptype
grype_rows, osv_rows, classify = _cls.grype_rows, _cls.osv_rows, _cls.classify


def load(p):
    with open(p) as f:
        return json.load(f)


def comps(doc):
    out, st = [], list(doc.get("components", []) or [])
    mc = (doc.get("metadata") or {}).get("component")
    if mc:
        out.append(mc)
    while st:
        c = st.pop(); out.append(c); st.extend(c.get("components", []) or [])
    return out


def props(c):
    d = collections.defaultdict(list)
    for p in c.get("properties", []) or []:
        d[p.get("name")].append(p.get("value"))
    return d


def hackage_nv(wdoc):
    return {(c.get("name", "").lower(), c.get("version")) for c in comps(wdoc) if ptype(c.get("purl")) == "hackage"}


def linkage(p):
    """Ask the binary: `otool -L` of the main executable (linkage/<p>.otool.txt). dynamic iff it
    loads any libHS*.dylib."""
    n = sum(1 for l in open(f"{R}/linkage/{p}.otool.txt") if "libHS" in l)
    return ("dynamic" if n else "static"), n


def closure_shape(p, wdoc):
    """lib-retaining iff the realised runtime closure (storecheck) holds a Haskell library store path,
    i.e. a path whose '<name>-<version>' is a pkg:hackage component of W (excluding the root's own outputs)."""
    hk = {f"{n}-{v}" for n, v in hackage_nv(wdoc)}
    root = (wdoc.get("metadata", {}).get("component", {}) or {}).get("name", "").lower()
    hits = []
    for line in open(f"{R}/storecheck/{p}.pathinfo-r.txt").read().split():
        base = line.split("/")[-1].split("-", 1)[1]
        base = re.sub(r"-(bin|doc|man|data|lib|dev|out)$", "", base)
        if base.lower() in hk and not base.lower().startswith(root + "-"):
            hits.append(base)
    ghc = [l for l in open(f"{R}/storecheck/{p}.pathinfo-r.txt").read().split() if re.search(r"-ghc-\d", l)]
    return ("dynamic" if hits else "static"), len(hits), len(ghc)


def main():
    res = {}
    agg = collections.Counter()
    agg_causes = collections.Counter()
    for p in PKGS:
        d = f"{OUT}/{p}"
        W = load(f"{d}/W.cdx.json"); WRd = load(f"{d}/WR.cdx.json")
        hk = hackage_nv(W)
        arms = {}
        for a in ("W", "WR", "S"):
            rows = grype_rows(f"{d}/{a}.cdx.json.grype.json") + osv_rows(f"{d}/{a}.cdx.json.osv.json")
            for r in rows:
                r["class"], r["why"] = classify(r, hk)
            arms[a] = rows
        # truth
        truth = {}
        for r in arms["S"]:
            if r["scanner"] == "grype" and r["class"] == "right-product":
                truth.setdefault((r["canon"], r["nname"]), {"src": set(), "rows": []})
                truth[(r["canon"], r["nname"])]["src"].add("a:grype-S"); truth[(r["canon"], r["nname"])]["rows"].append(r)
        for a in ("W", "WR", "S"):
            for r in arms[a]:
                if r["scanner"] == "osv" and (r["id"] or "").startswith("HSEC"):
                    truth.setdefault((r["canon"], r["nname"]), {"src": set(), "rows": []})
                    truth[(r["canon"], r["nname"])]["src"].add(f"b:osv-HSEC-{a}"); truth[(r["canon"], r["nname"])]["rows"].append(r)
        keys = {a: {(r["canon"], r["nname"]) for r in arms[a]} for a in arms}
        T = set(truth)
        hitW = T & keys["W"]; hitWR = T & keys["WR"]
        # component lookups for miss causes
        def bynn(doc):
            m = collections.defaultdict(list)
            for c in comps(doc):
                m[norm_name(c.get("name"))].append(c)
            return m
        Wc = bynn(W); WRc = bynn(WRd)
        misses = []
        for k in sorted(T - hitWR):
            t = truth[k]; tv = sorted({r["version"] for r in t["rows"] if r["version"]})
            tcpes = sorted({c for r in t["rows"] for c in r["cpes"]})
            wr = WRc.get(k[1], []); w = Wc.get(k[1], [])
            info = {"key": list(k), "truth_src": sorted(t["src"]), "truth_versions": tv, "truth_cpes": tcpes,
                    "W_has": [{"purl": c.get("purl"), "version": c.get("version"), "scope": c.get("scope"),
                               "cpe": c.get("cpe"), "cpe_candidates": props(c).get("waybill:cpe-candidates"),
                               "scope_source": props(c).get("waybill:scope-classification-source"),
                               "closure_role": props(c).get("waybill:closure-role")} for c in w],
                    "found_in_W": k in keys["W"]}
            if not w:
                cause = "component absent"
            elif not wr:
                cause = "dropped as non-runtime by scope"
            elif not any((c.get("version") or "") in tv for c in wr):
                cause = "version mismatch"
            elif not any(c.get("cpe") or props(c).get("waybill:cpe-candidates") for c in wr if (c.get("version") or "") in tv):
                cause = "present but no CPE"
            else:
                tvp = {":".join(x.split(":")[3:5]) for x in tcpes}
                wcp = set()
                for c in wr:
                    for x in ([c.get("cpe")] if c.get("cpe") else []) + [y for v in props(c).get("waybill:cpe-candidates", []) for y in (json.loads(v) if v.startswith("[") else [v])]:
                        wcp.add(":".join(x.split(":")[3:5]))
                cause = "present but wrong CPE" if tvp and not (tvp & wcp) else "other"
                info["wr_vendor_product"] = sorted(wcp); info["truth_vendor_product"] = sorted(tvp)
            info["cause"] = cause
            misses.append(info)
        cause_ct = collections.Counter(m["cause"] for m in misses)
        # WR findings outside truth, classified
        outside = collections.Counter(); outside_rows = []
        seen = set()
        for r in arms["WR"]:
            k = (r["canon"], r["nname"])
            if k in T or (k, r["class"]) in seen:
                continue
            seen.add((k, r["class"]))
            outside[r["class"]] += 1
            outside_rows.append({"key": list(k), "class": r["class"], "why": r["why"], "name": r["name"], "version": r["version"],
                                 "purl": r["purl"], "cpes": r["cpes"], "match": r["match"], "excerpt": r["desc"][:200]})
        # exclusions (collisions + questionable) per arm, distinct by key
        excl = {}
        for a in ("S", "W", "WR"):
            ex = {}
            for r in arms[a]:
                if r["class"] in ("collision", "questionable"):
                    ex[(r["canon"], r["nname"], r["version"])] = {"class": r["class"], "id": r["id"], "name": r["name"],
                                                                  "version": r["version"], "cpes": r["cpes"], "excerpt": r["desc"][:200]}
            excl[a] = [ex[k] for k in sorted(ex)]
        mp = {x["name"]: x["value"] for x in W.get("metadata", {}).get("properties", []) or []}
        c216 = json.loads(mp["waybill:closure-scope-method"]) if mp.get("waybill:closure-scope-method") else None
        sd, nhs, nghc = closure_shape(p, W)
        lk, nlibhs = linkage(p)
        r = {"link": lk, "libHS_dylibs": nlibhs, "closure_shape": "lib-retaining" if sd == "dynamic" else "exe-only",
             "runtime_haskell_lib_paths": nhs, "runtime_ghc_paths": nghc,
             "C190": mp.get("waybill:nix-closure-degraded"), "C216": c216,
             "truth": len(T), "truth_a": sum(1 for k in T if any(s.startswith("a:") for s in truth[k]["src"])),
             "truth_b": sum(1 for k in T if any(s.startswith("b:") for s in truth[k]["src"])),
             "W_hits": len(hitW), "WR_hits": len(hitWR),
             "W_recall": round(len(hitW) / len(T), 4) if T else None,
             "WR_recall": round(len(hitWR) / len(T), 4) if T else None,
             "misses_by_cause": dict(cause_ct), "misses": misses,
             "WR_outside_truth_by_class": dict(outside), "WR_outside_truth": outside_rows,
             "exclusions": excl,
             "truth_keys": [{"key": list(k), "src": sorted(truth[k]["src"]),
                             "versions": sorted({x["version"] or "" for x in truth[k]["rows"]})} for k in sorted(T)]}
        # --- adjudicated truth (supplementary; NOT the brief's definition) ---
        # drop a (b) key whose package is in neither the nix link closure (linkcheck/link-closure.json)
        # nor the realised runtime closure (storecheck); add HSEC found by osv on the link-closure
        # control document (linkcheck/<p>.link.cdx.json.osv.json).
        L = {(x["pname"].lower(), x["version"]) for x in load(f"{R}/linkcheck/link-closure.json")[p]}
        rt = set()
        for line in open(f"{R}/storecheck/{p}.pathinfo-r.txt").read().split():
            base = re.sub(r"-(bin|doc|man|data|lib|dev|out)$", "", line.split("/")[-1].split("-", 1)[1])
            mm = re.match(r"^(.*)-(\d[\w.+~-]*)$", base)
            if mm:
                rt.add((mm.group(1).lower(), mm.group(2)))
        dropped = []
        for k in sorted(T):
            t = truth[k]
            if all(s.startswith("b:") for s in t["src"]):
                nvs = {((x["name"] or "").lower(), x["version"]) for x in t["rows"]}
                if not (nvs & L) and not (nvs & rt):
                    dropped.append({"key": list(k), "name_versions": sorted(nvs)})
        added = []
        for rr in osv_rows(f"{R}/linkcheck/{p}.link.cdx.json.osv.json"):
            k = (rr["canon"], rr["nname"])
            if (rr["id"] or "").startswith("HSEC") and k not in T and k not in {tuple(a["key"]) for a in added}:
                cause = "component absent"
                wl = [c for c in comps(W) if norm_name(c.get("name")) == rr["nname"] and c.get("version") == rr["version"]]
                if wl and not any(ptype(c.get("purl")) == "hackage" for c in wl):
                    cause = "present under a non-hackage PURL (" + ",".join(sorted({c.get("purl") for c in wl})) + ")"
                added.append({"key": list(k), "id": rr["id"], "name": rr["name"], "version": rr["version"],
                              "in_W": k in keys["W"], "in_WR": k in keys["WR"], "cause_if_missed": cause})
        T2 = (T - {tuple(d_["key"]) for d_ in dropped}) | {tuple(a["key"]) for a in added}
        r["adjudicated"] = {"dropped_from_truth": dropped, "added_to_truth": added, "truth": len(T2),
                            "W_hits": len(T2 & keys["W"]), "WR_hits": len(T2 & keys["WR"]),
                            "W_recall": round(len(T2 & keys["W"]) / len(T2), 4) if T2 else None,
                            "WR_recall": round(len(T2 & keys["WR"]) / len(T2), 4) if T2 else None}
        agg["adj_truth"] += len(T2); agg["adj_W"] += len(T2 & keys["W"]); agg["adj_WR"] += len(T2 & keys["WR"])
        res[p] = r
        with open(f"{d}/truth.tsv", "w") as f:
            for k in sorted(T):
                f.write(f"{k[0]}\t{k[1]}\t{','.join(sorted(truth[k]['src']))}\t{'W' if k in hitW else '-'}\t{'WR' if k in hitWR else '-'}\n")
        agg["truth"] += len(T); agg["W_hits"] += len(hitW); agg["WR_hits"] += len(hitWR)
        for c, n in cause_ct.items():
            agg_causes[c] += n
    res["_aggregate"] = {"truth": agg["truth"], "W_hits": agg["W_hits"], "WR_hits": agg["WR_hits"],
                         "W_recall": round(agg["W_hits"] / agg["truth"], 4) if agg["truth"] else None,
                         "WR_recall": round(agg["WR_hits"] / agg["truth"], 4) if agg["truth"] else None,
                         "misses_by_cause": dict(agg_causes.most_common()),
                         "adjudicated": {"truth": agg["adj_truth"], "W_hits": agg["adj_W"], "WR_hits": agg["adj_WR"],
                                         "W_recall": round(agg["adj_W"] / agg["adj_truth"], 4),
                                         "WR_recall": round(agg["adj_WR"] / agg["adj_truth"], 4)}}
    with open(f"{OUT}/recall.json", "w") as f:
        json.dump(res, f, indent=1, default=sorted)
    print(f"{'package':24} {'link':7} {'closure':13} {'C216 method':16} {'truth':>5} {'a':>3} {'b':>3} {'W rec':>6} {'WR rec':>6} | adj {'T':>4} {'W':>6} {'WR':>6}  misses")
    for p in PKGS:
        r = res[p]
        ad = r["adjudicated"]
        print(f"{p:24} {r['link']:7} {r['closure_shape']:13} {(r['C216'] or {}).get('method', '-'):16} {r['truth']:>5} {r['truth_a']:>3} {r['truth_b']:>3} "
              f"{r['W_recall']!s:>6} {r['WR_recall']!s:>6} | adj {ad['truth']:>4} {ad['W_recall']!s:>6} {ad['WR_recall']!s:>6}  {r['misses_by_cause']}  outside={r['WR_outside_truth_by_class']}")
    print("aggregate", json.dumps(res["_aggregate"]))


if __name__ == "__main__":
    main()
