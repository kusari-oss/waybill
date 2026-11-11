#!/usr/bin/env python3
"""T035 (SC-001, SC-002): score the runtime-only documents `run.sh` produced.

Three finding sets per target, each a set of (vulnerability id, PURL):

  truth   : the committed runtime truth. The grype 0.120.0 matches on the
            original document (OUT_DIR/<t>/waybill.cdx.json.grype.json, waybill
            f891b4fe) whose PURL is in runtime.py's R(t). R(t) is recomputed
            here exactly as runtime.py does (NV + EMB; needs rt/ and rtscan/
            under NOISE_DIR), so the 350 is re-derived, not copied.
  full    : grype on this branch's full document (US4_DIR/<t>/waybill.cdx.json).
  runtime : grype on the recipe's output (US4_DIR/<t>/runtime.cdx.json).

SC-001 compares runtime with truth finding by finding, in both directions.
SC-002 checks that every sbomnix runtime finding (OUT_DIR/<t>/sbomnix-rt.cdx.json
.grype.json) is in runtime. sbomnix and waybill name packages differently, so
that join is on (id, normalised name, version), with runtime.py's name
normalisation (lower case, `python3.NN-` prefix dropped).

It also re-checks, on the live documents, what T033 checks on a fixture:
the three formats drop the same PURLs, and no not-in-artifact member remains.

usage: score.py OUT_DIR NOISE_DIR US4_DIR
"""
import json, re, sys, collections

OUT, NZ, US4 = sys.argv[1], sys.argv[2], sys.argv[3]
TARGETS = "curl openssh httpie gh ripgrep".split()
PY = re.compile(r"^python3(\.\d+)?-")
SUF = re.compile(r"-(lib|bin|dev|man|out|doc|xcrun|info)$")


def nn(n):
    return PY.sub("", (n or "").lower())


# ---- truth: runtime.py, verbatim rule ----------------------------------------
def split_nv(p):
    base = p.split("/")[-1].split("-", 1)[1]
    m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)
    if not m:
        return base, None
    return m.group(1), SUF.sub("", m.group(2))


def truth_components(t, doc):
    paths = open(f"{NZ}/rt/{t}.paths").read().split()
    nvs = {(nn(n), v) for p in paths for n, v in [split_nv(p)]}
    emb = {l.strip() for l in open(f"{NZ}/rtscan/{t}.purls")
           if l.startswith(("pkg:cargo", "pkg:golang", "pkg:pypi"))}
    return {c["purl"] for c in doc["components"]
            if (nn(c["name"]), c.get("version")) in nvs or c["purl"] in emb}


# ---- documents ---------------------------------------------------------------
def load(p):
    return json.load(open(p))


def matches(grype):
    return [(m["vulnerability"]["id"], m["artifact"]["purl"],
             m["artifact"]["name"], m["artifact"]["version"]) for m in grype["matches"]]


def all_cdx(doc, subject=True):
    out = []

    def walk(cs):
        for c in cs or []:
            out.append(c)
            walk(c.get("components"))
    if subject and doc.get("metadata", {}).get("component"):
        walk([doc["metadata"]["component"]])
    walk(doc.get("components"))
    return out


def purls(doc):
    if "bomFormat" in doc:
        return {c["purl"] for c in all_cdx(doc) if c.get("purl")}
    if "spdxVersion" in doc:
        return {r["referenceLocator"] for p in doc.get("packages", [])
                for r in p.get("externalRefs", []) or [] if r.get("referenceType") == "purl"}
    return {e["software_packageUrl"] for e in doc["@graph"] if e.get("software_packageUrl")}


def not_in_artifact(doc):
    """The contract's rule (contracts/sbom-edit-scope-excluded.md), as PURLs."""
    if "bomFormat" in doc:
        out = set()
        for c in all_cdx(doc, subject=False):
            props = {p["name"] for p in c.get("properties", []) or []}
            if c.get("scope") == "excluded" and not props & {"waybill:lifecycle-scope",
                                                              "waybill:optional-derivation"}:
                out.add(c.get("purl"))
        return out
    if "spdxVersion" in doc:
        pid = {p["SPDXID"]: r["referenceLocator"] for p in doc.get("packages", [])
               for r in p.get("externalRefs", []) or [] if r.get("referenceType") == "purl"}
        return {pid.get(r["spdxElementId"]) for r in doc.get("relationships", [])
                if r.get("relationshipType") == "BUILD_TOOL_OF"}
    pid = {e["spdxId"]: e["software_packageUrl"] for e in doc["@graph"]
           if e.get("spdxId") and e.get("software_packageUrl")}
    return {pid.get(x) for e in doc["@graph"]
            if e.get("type") == "LifecycleScopedRelationship"
            and e.get("relationshipType") == "usesTool" and e.get("scope") == "build"
            for x in e.get("to", [])}


def c216(doc):
    for pr in doc.get("metadata", {}).get("properties", []) or []:
        if pr["name"] == "waybill:closure-scope-method":
            return json.loads(pr["value"])
    return None


def subject(doc):
    m = doc["metadata"]["component"]
    return f'{m.get("purl")}'


res, tot = {}, collections.Counter()
for t in TARGETS:
    odoc = load(f"{OUT}/{t}/waybill.cdx.json")
    R = truth_components(t, odoc)
    old = matches(load(f"{OUT}/{t}/waybill.cdx.json.grype.json"))
    truth = {(i, p) for i, p, _, _ in old if p in R}

    ndoc = load(f"{US4}/{t}/waybill.cdx.json")
    rdoc = load(f"{US4}/{t}/runtime.cdx.json")
    full = {(i, p) for i, p, _, _ in matches(load(f"{US4}/{t}/waybill.cdx.json.grype.json"))}
    rtm = matches(load(f"{US4}/{t}/runtime.cdx.json.grype.json"))
    runtime = {(i, p) for i, p, _, _ in rtm}
    runtime_nv = {(i, nn(n), v) for i, _, n, v in rtm}

    sbx = {(m["vulnerability"]["id"], nn(m["artifact"]["name"]), m["artifact"]["version"])
           for m in load(f"{OUT}/{t}/sbomnix-rt.cdx.json.grype.json")["matches"]}

    # Same PURLs dropped from each format, and nothing marked left behind.
    dropped, left = {}, {}
    for f in ("cdx", "spdx", "spdx3"):
        a, b = load(f"{US4}/{t}/waybill.{f}.json"), load(f"{US4}/{t}/runtime.{f}.json")
        dropped[f] = purls(a) - purls(b)
        left[f] = sorted(x for x in not_in_artifact(b) if x)

    # Component-level: what the recipe keeps that runtime.py's R(t) does not
    # name, and the reverse, with the C203 value that decided each.
    kept = {c["purl"]: c for c in all_cdx(rdoc, subject=False) if c.get("purl")}

    def c203(c):
        return next((p["value"] for p in c.get("properties", []) or []
                     if p["name"] == "waybill:scope-classification-source"), None)

    res[t] = dict(
        c216=c216(ndoc),
        subject=dict(full=subject(ndoc), runtime=subject(rdoc)),
        components=dict(full=len(all_cdx(ndoc, subject=False)),
                        runtime=len(all_cdx(rdoc, subject=False)),
                        truth_runtime=len(R),
                        kept_not_in_truth=sorted([p, c203(kept[p])] for p in set(kept) - R),
                        truth_not_kept=sorted(R - set(kept))),
        findings=dict(
            original_full=len(old),
            full=len(full),
            runtime=len(runtime),
            truth=len(truth),
            full_vs_original=dict(
                lost=sorted(map(list, {(i, p) for i, p, _, _ in old} - full)),
                added=sorted(map(list, full - {(i, p) for i, p, _, _ in old}))),
            runtime_vs_truth=dict(lost=sorted(map(list, truth - runtime)),
                                  added=sorted(map(list, runtime - truth))),
            sbomnix_runtime=len(sbx),
            sbomnix_missing_from_runtime=sorted(map(list, sbx - runtime_nv)),
        ),
        runtime_findings=sorted("\t".join(m) for m in {(i, p) for i, p, _, _ in rtm}),
        edit_stderr=[l.rstrip("\n") for l in open(f"{US4}/{t}/edit.cdx.log")
                     if " INFO " not in l and "\x1b[" not in l],
        formats=dict(
            dropped=dict((f, len(s)) for f, s in dropped.items()),
            same_purls_dropped=dropped["cdx"] == dropped["spdx"] == dropped["spdx3"],
            not_in_artifact_left=left,
        ),
    )
    for k in ("original_full", "full", "runtime", "truth", "sbomnix_runtime"):
        tot[k] += res[t]["findings"][k]
    tot["runtime_lost_vs_truth"] += len(truth - runtime)
    tot["runtime_added_vs_truth"] += len(runtime - truth)
    tot["sbomnix_missing"] += len(sbx - runtime_nv)

json.dump(dict(total=dict(tot), targets=res), sys.stdout, indent=1, sort_keys=True)
print()
