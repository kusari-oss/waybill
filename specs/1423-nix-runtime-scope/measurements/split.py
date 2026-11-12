#!/usr/bin/env python3
"""Split waybill grype findings by runtime relevance and by waybill's marks.

Finding identity and runtime-relevance (RT-name) follow analyze.py exactly:
finding = (norm_name, version, canonical CVE); runtime-relevant iff norm_name is
a pname of `nix path-info -r` of the built output.
RT-path is a second, stricter definition: a component is runtime if
 (a) a runtime-closure store path has pname == name AND version == version, or
 (b) one of its waybill:source-files lies inside a runtime-closure store path
     (catches Go modules / Rust crates compiled into a runtime binary).
usage: split.py OUT_DIR RT_DIR
"""
import json, re, sys, collections

OUT, RTD = sys.argv[1], sys.argv[2]
TARGETS = "curl openssh httpie gh ripgrep moat".split()
PY = re.compile(r"^python3(\.\d+)?-")


def norm_name(n):
    return PY.sub("", (n or "").lower())


def canon(vid, aliases):
    if vid and vid.upper().startswith("CVE-"):
        return vid.upper()
    for a in aliases or []:
        if a and a.upper().startswith("CVE-"):
            return a.upper()
    return (vid or "").upper()


def rt_paths(t):
    try:
        return open(f"{RTD}/{t}.paths").read().split()
    except FileNotFoundError:
        return None


def split_nv(p):
    base = p.split("/")[-1].split("-", 1)[1]
    m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)
    return (m.group(1), m.group(2)) if m else (base, None)


def props(c):
    return {p["name"]: p.get("value") for p in c.get("properties", []) or []}


def mark(c):
    pr = props(c)
    if c.get("scope") == "excluded":
        return "excluded"
    role = pr.get("waybill:closure-role")
    if role:
        return f"role={role}"
    if pr.get("waybill:lifecycle-scope"):
        return f"lifecycle={pr['waybill:lifecycle-scope']}"
    return "unmarked"


def main():
    summary = {}
    for t in TARGETS:
        doc = json.load(open(f"{OUT}/{t}/waybill.cdx.json"))
        comps = {c["purl"]: c for c in doc["components"]}
        g = json.load(open(f"{OUT}/{t}/waybill.cdx.json.grype.json"))
        paths = rt_paths(t)
        rt_names = rt_nv = rt_set = None
        if paths is not None:
            rt_names = {norm_name(split_nv(p)[0]) for p in paths}
            rt_nv = set()
            for p in paths:
                n, v = split_nv(p)
                rt_nv.add((norm_name(n), v))
                # multi-output suffixes: 'brotli-1.2.0-lib' splits as version '1.2.0-lib'
                if v:
                    rt_nv.add((norm_name(n), re.sub(r"-(lib|bin|dev|man|out|doc|xcrun)$", "", v)))
            rt_set = set(paths)

        def rt_path(c):
            if rt_set is None:
                return None
            if (norm_name(c["name"]), c.get("version")) in rt_nv:
                return True
            sf = props(c).get("waybill:source-files")
            if sf:
                for f in json.loads(sf):
                    sp = "/".join(f.split("/")[:4])
                    if sp in rt_set:
                        return True
            return False

        findings = collections.defaultdict(set)  # key -> purls
        for m in g.get("matches", []):
            a = m["artifact"]; v = m["vulnerability"]
            al = [r.get("id") for r in m.get("relatedVulnerabilities", [])]
            key = (norm_name(a.get("name")), a.get("version"), canon(v.get("id"), al))
            findings[key].add(a["purl"])
        rows = collections.Counter()
        detail = collections.defaultdict(collections.Counter)
        for key, purls in findings.items():
            cs = [comps[p] for p in purls]
            mk = "|".join(sorted({mark(c) for c in cs}))
            rn = "n/a" if rt_names is None else ("rt" if key[0] in rt_names else "build")
            rp = "n/a" if rt_set is None else ("rt" if any(rt_path(c) for c in cs) else "build")
            rows[(rn, rp, mk)] += 1
            detail[(rn, rp, mk)][f"{cs[0]['name']}@{cs[0].get('version')} [{cs[0]['purl'].split(':')[1].split('/')[0]}]"] += 1
        # component-level precision/recall of "marked runtime" (= not excluded)
        cl = None
        if rt_set is not None:
            tp = fp = fn = tn = 0
            fps, fns = [], []
            for c in doc["components"]:
                marked_rt = c.get("scope") != "excluded"
                is_rt = rt_path(c)
                if marked_rt and is_rt: tp += 1
                elif marked_rt and not is_rt: fp += 1; fps.append(c)
                elif not marked_rt and is_rt: fn += 1; fns.append(c)
                else: tn += 1
            cl = dict(tp=tp, fp=fp, fn=fn, tn=tn,
                      fp_by_mark=dict(collections.Counter(mark(c) + "/" + c["purl"].split(":")[1].split("/")[0] for c in fps)),
                      fn_list=[f"{c['name']}@{c.get('version')} {mark(c)}" for c in fns])
        summary[t] = dict(total=len(findings), rows={"/".join(k): v for k, v in rows.items()},
                          detail={"/".join(k): dict(v) for k, v in detail.items()}, comp=cl)
    json.dump(summary, sys.stdout, indent=1)


if __name__ == "__main__":
    main()
