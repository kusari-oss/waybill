#!/usr/bin/env python3
"""Runtime-closure ground truth for waybill's closure documents.

R(t) = components of waybill.cdx.json that are in the Nix runtime closure of
the built output (`nix path-info -r`), by either:
  NV  : a runtime store path's (pname, version) equals the component's
        (norm_name, version); multi-output suffixes (-lib/-bin/...) stripped;
  EMB : the component's purl appears in a `waybill sbom scan --path` of some
        runtime store path (binary-embedded Go modules / Rust crates / pypi).
Writes <NOISE>/rtdoc/<t>.cdx.json = the waybill document restricted to R(t).
usage: runtime.py OUT_DIR NOISE_DIR
"""
import json, re, sys, os, collections

OUT, NZ = sys.argv[1], sys.argv[2]
PY = re.compile(r"^python3(\.\d+)?-")
SUF = re.compile(r"-(lib|bin|dev|man|out|doc|xcrun|info)$")


def nn(n):
    return PY.sub("", (n or "").lower())


def split_nv(p):
    base = p.split("/")[-1].split("-", 1)[1]
    m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)
    if not m:
        return base, None
    return m.group(1), SUF.sub("", m.group(2))


def props(c):
    return {p["name"]: p.get("value") for p in c.get("properties", []) or []}


res = {}
os.makedirs(f"{NZ}/rtdoc", exist_ok=True)
for t in "curl openssh httpie gh ripgrep".split():
    doc = json.load(open(f"{OUT}/{t}/waybill.cdx.json"))
    paths = open(f"{NZ}/rt/{t}.paths").read().split()
    nvs = {(nn(n), v): p for p in paths for n, v in [split_nv(p)]}
    emb = {l.strip() for l in open(f"{NZ}/rtscan/{t}.purls")
           if l.startswith(("pkg:cargo", "pkg:golang", "pkg:pypi"))}
    R, how = set(), collections.Counter()
    hit_paths = set()
    for c in doc["components"]:
        k = (nn(c["name"]), c.get("version"))
        a = k in nvs
        b = c["purl"] in emb
        if a:
            hit_paths.add(nvs[k])
        if a or b:
            R.add(c["purl"])
            how["NV" if a and not b else "EMB" if b and not a else "both"] += 1
    unmatched = sorted(p.split("/")[-1].split("-", 1)[1] for p in paths if p not in hit_paths)
    # precision / recall of the mark "runtime" (= scope not excluded)
    tp = fp = fn = 0
    fpl = collections.Counter(); fnl = []
    for c in doc["components"]:
        mr = c.get("scope") != "excluded"; r = c["purl"] in R
        pr = props(c)
        lab = (pr.get("waybill:closure-role") or "no-role") + "/" + c["purl"].split(":")[1].split("/")[0]
        if mr and r: tp += 1
        elif mr: fp += 1; fpl[lab] += 1
        elif r: fn += 1; fnl.append(f"{c['name']}@{c.get('version')} ({lab})")
    # finding-level, from the stored grype output
    g = json.load(open(f"{OUT}/{t}/waybill.cdx.json.grype.json"))
    comps = {c["purl"]: c for c in doc["components"]}
    ftp = ffp = ffn = 0
    for m in g["matches"]:
        p = m["artifact"]["purl"]; mr = comps[p].get("scope") != "excluded"; r = p in R
        if mr and r: ftp += 1
        elif mr: ffp += 1
        elif r: ffn += 1
    out = dict(doc)
    out["components"] = [c for c in doc["components"] if c["purl"] in R]
    keep = set(R) | {doc["metadata"]["component"].get("bom-ref")}
    out["dependencies"] = [dict(d, dependsOn=[x for x in d.get("dependsOn", []) if x in keep])
                           for d in doc.get("dependencies", []) if d.get("ref") in keep]
    out.pop("vulnerabilities", None)
    json.dump(out, open(f"{NZ}/rtdoc/{t}.cdx.json", "w"))
    res[t] = dict(components=len(doc["components"]), runtime=len(R), how=dict(how),
                  rt_paths=len(paths), rt_paths_unmatched=unmatched,
                  comp=dict(tp=tp, fp=fp, fn=fn, fp_by=dict(fpl), fn_list=fnl),
                  finding=dict(tp=ftp, fp=ffp, fn=ffn, total=len(g["matches"])))
json.dump(res, sys.stdout, indent=1)
