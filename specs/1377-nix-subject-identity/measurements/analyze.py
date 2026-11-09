#!/usr/bin/env python3
"""Spike analysis: normalise scanner findings and compare sbomnix vs waybill.

A finding is (component name, version, vuln id), with the vuln id resolved to
a CVE where any alias provides one, so the three scanners are comparable.
Runtime relevance is judged against Nix's own runtime closure
(`nix path-info -r`), NOT sbomnix's runtime SBOM, which returned 0 components
for openssh despite an 11-path closure.
"""
import json, os, re, subprocess, sys, collections

W = "/Users/mlieberman/Projects/mikebom/target/nix-vuln-spike"
TARGETS = "curl openssh httpie gh ripgrep moat".split()
TOOLS = ("sbomnix-bt", "waybill", "sbomnix-rt")
SCANNERS = ("grype", "trivy", "osv")

PY = re.compile(r"^python3(\.\d+)?-")


def norm_name(n):
    n = (n or "").lower()
    return PY.sub("", n)


def canon(vid, aliases):
    if vid and vid.upper().startswith("CVE-"):
        return vid.upper()
    for a in aliases or []:
        if a and a.upper().startswith("CVE-"):
            return a.upper()
    return (vid or "").upper()


def load(p):
    try:
        with open(p) as f:
            return json.load(f)
    except Exception:
        return None


def grype(p):
    d = load(p) or {}
    out = set(); kinds = collections.Counter()
    for m in d.get("matches", []):
        a = m.get("artifact", {}); v = m.get("vulnerability", {})
        al = [r.get("id") for r in m.get("relatedVulnerabilities", [])]
        cid = canon(v.get("id"), al)
        out.add((norm_name(a.get("name")), a.get("version"), cid))
        for md in m.get("matchDetails", []):
            kinds[md.get("type")] += 1
    return out, kinds


def trivy(p):
    d = load(p) or {}
    out = set()
    for r in d.get("Results", []) or []:
        for v in r.get("Vulnerabilities", []) or []:
            out.add((norm_name(v.get("PkgName")), v.get("InstalledVersion"),
                     canon(v.get("VulnerabilityID"), [])))
    return out, collections.Counter()


def osv(p):
    d = load(p) or {}
    out = set()
    for r in d.get("results", []) or []:
        for pk in r.get("packages", []) or []:
            pkg = pk.get("package", {})
            for v in pk.get("vulnerabilities", []) or []:
                out.add((norm_name(pkg.get("name")), pkg.get("version"),
                         canon(v.get("id"), v.get("aliases"))))
    return out, collections.Counter()


READ = {"grype": grype, "trivy": trivy, "osv": osv}


def runtime_names(t):
    """pnames of Nix's runtime closure for a target, or None if unrealised."""
    try:
        op = subprocess.run(["nix", "eval", "--raw", f"path:{W}/targets/{t}#default.outPath"],
                            capture_output=True, text=True, timeout=120).stdout.strip()
        if not op or not os.path.exists(op):
            return None
        paths = subprocess.run(["nix", "path-info", "-r", op], capture_output=True,
                               text=True, timeout=120).stdout.split()
    except Exception:
        return None
    names = set()
    for p in paths:
        base = p.split("/")[-1].split("-", 1)[1]           # drop hash
        m = re.match(r"^(.*?)-(\d[\w.+~-]*)$", base)     # split trailing version
        names.add(norm_name(m.group(1) if m else base))
    return names


def main():
    report = {}
    print(f"{'target':8} {'scanner':6} | {'sbomnix-bt':>10} {'waybill':>8} {'shared':>7} "
          f"{'sbx-only':>8} {'wb-only':>8} | {'sbx-rt':>7} | runtime-relevant sbx/wb")
    for t in TARGETS:
        rt = runtime_names(t)
        for sc in SCANNERS:
            sets = {}
            for tool in TOOLS:
                p = f"{W}/out/{t}/{tool}.cdx.json.{sc}.json"
                if os.path.exists(p):
                    sets[tool], kinds = READ[sc](p)
                    if sc == "grype":
                        report.setdefault(t, {}).setdefault("grype-kinds", {})[tool] = dict(kinds)
            a = sets.get("sbomnix-bt", set()); b = sets.get("waybill", set())
            shared = a & b; ao = a - b; bo = b - a
            rtc = len(sets["sbomnix-rt"]) if "sbomnix-rt" in sets else "-"
            if rt is None:
                rr = "n/a (unrealised)"
            else:
                rr = f"{sum(1 for x in a if x[0] in rt)}/{sum(1 for x in b if x[0] in rt)}"
            print(f"{t:8} {sc:6} | {len(a):>10} {len(b):>8} {len(shared):>7} {len(ao):>8} "
                  f"{len(bo):>8} | {str(rtc):>7} | {rr}")
            report.setdefault(t, {})[sc] = {
                "sbomnix_only": sorted(ao), "waybill_only": sorted(bo), "shared": sorted(shared),
                "runtime_names": sorted(rt) if rt else None,
            }
    with open(f"{W}/findings.json", "w") as f:
        json.dump(report, f, indent=1, default=list)
    print("\ngrype match kinds (per tool):")
    for t in TARGETS:
        print(f"  {t:8} {report.get(t, {}).get('grype-kinds')}")


if __name__ == "__main__":
    main()
