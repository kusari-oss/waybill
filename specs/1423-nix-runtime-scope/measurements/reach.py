#!/usr/bin/env python3
"""Candidate marking: root-relative reachability over artifact-input edges.

reached(t) = derivations reachable from the target's root .drv by following
only ARTIFACT_FIELDS (classify.rs) edges, transitively. Compared against the
runtime closure (`nix path-info -r`) mapped to derivations by output path.
usage: reach.py OUT_DIR NOISE_DIR
"""
import json, re, sys, collections

OUT, NZ = sys.argv[1], sys.argv[2]
ART = ["buildInputs", "propagatedBuildInputs", "depsHostHost",
       "depsTargetTarget", "depsTargetTargetPropagated"]
PY = re.compile(r"^python3(\.\d+)?-")


def nn(n):
    return PY.sub("", (n or "").lower())


def field(d, f):
    sa = d.get("structuredAttrs")
    if isinstance(sa, dict) and f in sa:
        v = sa[f]
        return [x for x in (v if isinstance(v, list) else [v]) if isinstance(x, str)]
    v = (d.get("env") or {}).get(f, "")
    return v.split() if isinstance(v, str) else []


def nv_of(key, d):
    env = d.get("env") or {}
    sa = d.get("structuredAttrs") if isinstance(d.get("structuredAttrs"), dict) else {}
    p = env.get("pname") or sa.get("pname"); v = env.get("version") or sa.get("version")
    if p:
        return nn(p), v
    base = key.split("-", 1)[1][:-4]
    m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)
    return (nn(m.group(1)), m.group(2)) if m else (nn(base), None)


res = {}
for t in "curl openssh httpie gh ripgrep moat".split():
    raw = json.load(open(f"{NZ}/drv/{t}.json"))
    drvs = raw.get("derivations", raw)
    root = open(f"{NZ}/drv/{t}.root").read().strip().split("/")[-1]
    out_idx = {}
    for k, d in drvs.items():
        for o in (d.get("outputs") or {}).values():
            if o.get("path"):
                out_idx[o["path"].split("/")[-1]] = k
    seen = {root}; q = collections.deque([root])
    while q:
        k = q.popleft()
        for f in ART:
            for p in field(drvs[k], f):
                dk = out_idx.get(p.split("/")[-1])
                if dk and dk not in seen:
                    seen.add(dk); q.append(dk)
    reached_nv = {nv_of(k, drvs[k]) for k in seen}
    r = {"drvs": len(drvs), "reached": len(seen)}
    try:
        paths = open(f"{NZ}/rt/{t}.paths").read().split()
    except FileNotFoundError:
        paths = None
    if paths is not None:
        rt = {out_idx[p.split("/")[-1]] for p in paths if p.split("/")[-1] in out_idx}
        r.update(rt_drvs=len(rt), rt_unmapped=len(paths) - sum(1 for p in paths if p.split("/")[-1] in out_idx),
                 tp=len(seen & rt), fp=len(seen - rt), fn=len(rt - seen),
                 fn_list=sorted(k.split("-", 1)[1] for k in rt - seen),
                 fp_sample=sorted(k.split("-", 1)[1] for k in seen - rt))
    # findings under the candidate marking
    doc = json.load(open(f"{OUT}/{t}/waybill.cdx.json"))
    comps = {c["purl"]: c for c in doc["components"]}
    reached_hosts = {o.get("path", "").split("/")[-1] for k in seen for o in (drvs[k].get("outputs") or {}).values()}
    def marked(c):
        if (nn(c["name"]), c.get("version")) in reached_nv:
            return True
        for pr in c.get("properties", []) or []:
            if pr["name"] == "waybill:source-files":
                return any(f.startswith("/nix/store/") and f.split("/")[3] in reached_hosts for f in json.loads(pr["value"]))
        return False
    g = json.load(open(f"{OUT}/{t}/waybill.cdx.json.grype.json"))
    r["findings_total"] = len(g["matches"])
    r["findings_reach_marked"] = sum(1 for m in g["matches"] if marked(comps[m["artifact"]["purl"]]))
    r["components_reach_marked"] = sum(1 for c in doc["components"] if marked(c))
    keep = {p for p, c in comps.items() if marked(c)}
    out = dict(doc); out["components"] = [c for c in doc["components"] if c["purl"] in keep]
    out.pop("vulnerabilities", None); out.pop("dependencies", None)
    json.dump(out, open(f"{NZ}/reachdoc-{t}.cdx.json", "w"))
    res[t] = r
json.dump(res, sys.stdout, indent=1)
