#!/usr/bin/env python3
"""T026 (SC-003): score waybill's reachability walk at the finding level.

Three markings are scored against one truth, over one set of findings:

  truth : runtime.py's R(t) -- a component is runtime when a runtime store
          path's (pname, version) matches it (NV) or its PURL is embedded in a
          runtime store path (EMB). Computed here exactly as runtime.py does.
  reach : reach.py's candidate, recomputed exactly as reach.py does, so the
          committed 95.7% / 94.6% is reproduced by this script before the
          walk is compared with it.
  walk  : the document waybill emitted with the store query disabled
          (run.sh): a component is runtime when it is not `scope: excluded`.

Findings are the stored grype matches on the original document
(OUT_DIR/<t>/waybill.cdx.json.grype.json); each is joined to the walk
document by PURL. A finding whose PURL the walk document lacks is counted,
not guessed at.

usage: score.py OUT_DIR NOISE_DIR WALK_DIR
"""
import json, re, sys, collections

OUT, NZ, WALK = sys.argv[1], sys.argv[2], sys.argv[3]
TARGETS = "curl openssh httpie gh ripgrep".split()
PY = re.compile(r"^python3(\.\d+)?-")
SUF = re.compile(r"-(lib|bin|dev|man|out|doc|xcrun|info)$")
ART = ["buildInputs", "propagatedBuildInputs", "depsHostHost",
       "depsTargetTarget", "depsTargetTargetPropagated"]


def nn(n):
    return PY.sub("", (n or "").lower())


# ---- truth: runtime.py ------------------------------------------------------
def split_nv(p):
    base = p.split("/")[-1].split("-", 1)[1]
    m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)
    if not m:
        return base, None
    return m.group(1), SUF.sub("", m.group(2))


def truth(t, doc):
    paths = open(f"{NZ}/rt/{t}.paths").read().split()
    nvs = {(nn(n), v) for p in paths for n, v in [split_nv(p)]}
    emb = {l.strip() for l in open(f"{NZ}/rtscan/{t}.purls")
           if l.startswith(("pkg:cargo", "pkg:golang", "pkg:pypi"))}
    return {c["purl"] for c in doc["components"]
            if (nn(c["name"]), c.get("version")) in nvs or c["purl"] in emb}


# ---- reach: reach.py --------------------------------------------------------
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


def reach_marker(t):
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
    hosts = {o.get("path", "").split("/")[-1] for k in seen
             for o in (drvs[k].get("outputs") or {}).values()}

    def runtime(c):
        if (nn(c["name"]), c.get("version")) in reached_nv:
            return True
        for pr in c.get("properties", []) or []:
            if pr["name"] == "waybill:source-files":
                return any(f.startswith("/nix/store/") and f.split("/")[3] in hosts
                           for f in json.loads(pr["value"]))
        return False
    return runtime


# ---- scoring ----------------------------------------------------------------
def prf(tp, fp, fn):
    p = tp / (tp + fp) if tp + fp else None
    r = tp / (tp + fn) if tp + fn else None
    return p, r


def c203(c):
    for pr in c.get("properties", []) or []:
        if pr["name"] == "waybill:scope-classification-source":
            return pr.get("value")
    return None


def c216(doc):
    for pr in doc.get("metadata", {}).get("properties", []) or []:
        if pr["name"] == "waybill:closure-scope-method":
            return json.loads(pr["value"])
    return None


def all_components(doc):
    out = []

    def walk(cs):
        for c in cs or []:
            out.append(c)
            walk(c.get("components"))
    walk(doc.get("components"))
    return out


res, tot = {}, collections.Counter()
for t in TARGETS:
    doc = json.load(open(f"{OUT}/{t}/waybill.cdx.json"))
    comps = {c["purl"]: c for c in doc["components"]}
    R = truth(t, doc)
    reach = reach_marker(t)
    wdoc = json.load(open(f"{WALK}/{t}/waybill.cdx.json"))
    wcomps = {}
    for c in all_components(wdoc):
        wcomps.setdefault(c["purl"], c)
    g = json.load(open(f"{OUT}/{t}/waybill.cdx.json.grype.json"))
    n = collections.Counter()
    disagree = collections.Counter()
    for m in g["matches"]:
        p = m["artifact"]["purl"]
        r = p in R
        a = reach(comps[p])
        n["findings"] += 1
        n["reach_" + ("tp" if a and r else "fp" if a else "fn" if r else "tn")] += 1
        wc = wcomps.get(p)
        if wc is None:
            n["walk_unjoined"] += 1
            continue
        w = wc.get("scope") != "excluded"
        n["walk_" + ("tp" if w and r else "fp" if w else "fn" if r else "tn")] += 1
        if w != a:
            disagree[(f"{wc['name']}@{wc.get('version')}",
                      "walk-runtime" if w else "walk-marked",
                      "truth-runtime" if r else "truth-build",
                      c203(wc) or "-")] += 1
    ev = collections.Counter(c203(c) or "-" for c in all_components(wdoc)
                             if any(pr["name"] == "waybill:closure-role"
                                    for pr in c.get("properties", []) or [])
                             or c203(c) is not None)
    res[t] = dict(counts=dict(n), c216=c216(wdoc), c203_values=dict(ev),
                  disagreements_with_reach=[
                      dict(component=k[0], walk=k[1], truth=k[2], c203=k[3], findings=v)
                      for k, v in sorted(disagree.items())])
    tot.update(n)

summary = {}
for who in ("reach", "walk"):
    p, r = prf(tot[f"{who}_tp"], tot[f"{who}_fp"], tot[f"{who}_fn"])
    summary[who] = dict(tp=tot[f"{who}_tp"], fp=tot[f"{who}_fp"], fn=tot[f"{who}_fn"],
                        marked_runtime=tot[f"{who}_tp"] + tot[f"{who}_fp"],
                        precision=p, recall=r)
summary["walk"]["unjoined_findings"] = tot["walk_unjoined"]
summary["findings"] = tot["findings"]
summary["truth_runtime_findings"] = tot["reach_tp"] + tot["reach_fn"]
json.dump(dict(total=summary, targets=res), sys.stdout, indent=1, sort_keys=True)
