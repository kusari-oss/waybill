#!/usr/bin/env python3
"""The name-collision classifier, and the recall-truth file built with it.

This is the committed truth rule research R7 cites. `recall.py` imports it
(by path, because `python3 -I` does not put the script's directory on
sys.path), and `.github/workflows/recall-truth.yml` runs it to produce
`waybill-cli/tests/fixtures/public_corpus/<target>/recall-truth.json`.

Run:
  python3 -I -B classify.py truth \\
      --grype S.cdx.json.grype.json      # grype on sbomnix's runtime CycloneDX
      --osv link.cdx.json.osv.json       # osv-scanner on the link-closure document
      --link-closure link-closure.json   # `nix eval ... link-closure.nix ... result`
      --target-pin <full sha> \\
      --tool sbomnix=1.9.0 --tool grype=0.120.0 --tool grype-db=<built> --tool osv-scanner=2.7.0 \\
      --out recall-truth.json
  python3 -I -B classify.py link-cdx --link-closure link-closure.json --out link.cdx.json

Classification of one grype finding (verbatim from the recall-corpus probe):

  haskell   = the matched component's purl is pkg:hackage, OR its lower-cased
              (name, version) is a Haskell package of the target. recall.py
              takes that set from waybill's own document; the truth builder
              takes it from nix (link-closure.nix `universe`), so the truth
              never depends on the tool it scores.
  if not haskell                                    -> right-product
  elif name in BUNDLED_C                            -> questionable (bundled C sources)
  elif any CVE description mentions haskell/hackage -> right-product
  else                                              -> collision (excluded, excerpt recorded)

An osv-scanner advisory is always right-product: it is keyed by ecosystem.

Truth = grype right-product findings on sbomnix's runtime document
      + HSEC advisories osv-scanner reports for the Haskell link closure.
Collisions and questionable findings are listed under `excluded`, with the
CVE excerpt, so a reviewer can check each one against this rule.

REVIEWED is a second, independent check: the (name, CPE vendor:product)
pairs a person decided by reading the CVE text during the stage 2 corpus work
(../stage2-classify.tsv). The rule above must agree with every reviewed pair
it meets. A disagreement means the rule, or its Haskell set, is wrong for
that target, and the truth file is not written.

Exit status 2 when the inputs cannot produce an honest truth file: a
right-product grype finding with no CPE (the proxy has nothing to match on),
a disagreement with REVIEWED, or a link closure with no cabal metadata (an
empty closure would silently drop every HSEC).
"""
import argparse, collections, datetime, json, re, sys

# Haskell packages that vendor C library sources (cmark-gfm-style). Membership is the rule; listed
# here so the questionable bucket is explicit. Only names present in a closure can fire.
BUNDLED_C = {"cmark-gfm", "cmark", "libyaml", "direct-sqlite", "lua", "hslua-core", "lzma-clib", "zlib-clib"}

# --- copied verbatim from specs/1377-nix-subject-identity/measurements/analyze.py ---
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
# --- end copy ---


def load(p):
    with open(p) as f:
        return json.load(f)


def ptype(purl):
    m = re.match(r"^pkg:([^/]+)/", purl or "")
    return m.group(1) if m else "none"


def grype_rows(p):
    rows = []
    for m in load(p).get("matches", []) or []:
        a = m.get("artifact", {}); v = m.get("vulnerability", {})
        rel = m.get("relatedVulnerabilities", []) or []
        descs = [v.get("description") or ""] + [r.get("description") or "" for r in rel]
        cpes = sorted({c for md in m.get("matchDetails", []) or [] for c in ((md.get("found") or {}).get("cpes") or [])})
        rows.append({"scanner": "grype", "id": v.get("id"), "canon": canon(v.get("id"), [r.get("id") for r in rel]),
                     "name": a.get("name"), "nname": norm_name(a.get("name")), "version": a.get("version"),
                     "purl": a.get("purl"), "cpes": cpes, "desc": next((d for d in descs if d), ""),
                     "alldesc": " ".join(descs),
                     "match": sorted({md.get("type") for md in m.get("matchDetails", []) or []})})
    return rows


def osv_rows(p):
    rows = []
    for r in load(p).get("results", []) or []:
        for pk in r.get("packages", []) or []:
            pkg = pk.get("package", {})
            for v in pk.get("vulnerabilities", []) or []:
                rows.append({"scanner": "osv", "id": v.get("id"), "canon": canon(v.get("id"), v.get("aliases")),
                             "name": pkg.get("name"), "nname": norm_name(pkg.get("name")), "version": pkg.get("version"),
                             "purl": None, "ecosystem": pkg.get("ecosystem"), "cpes": [],
                             "desc": (v.get("summary") or v.get("details") or "")[:300], "alldesc": "", "match": ["osv"]})
    return rows


def classify(row, hk):
    """`hk` is the target's Haskell packages as a set of (lower-cased name, version)."""
    if row["scanner"] == "osv":
        return "right-product", "osv ecosystem advisory"
    haskell = ptype(row["purl"]) == "hackage" or ((row["name"] or "").lower(), row["version"]) in hk
    if not haskell:
        return "right-product", "not a Haskell package"
    if (row["name"] or "").lower() in BUNDLED_C:
        return "questionable", "Haskell package bundling C sources"
    if re.search(r"haskell|hackage", row["alldesc"], re.I):
        return "right-product", "description names Haskell/Hackage"
    return "collision", "Haskell package; CVE describes another product"


# (name, cpe vendor:product) -> verdict, decided by reading the CVE descriptions
# (stage 2 of the recall corpus; excerpts in ../stage2-classify.tsv). `zlib` is
# keyed by major version: the C library is 1.x, the Haskell binding 0.x.
REVIEWED = {
    ("openssl", "openssl:openssl"): "right-product",
    ("libxml2", "xmlsoft:libxml2"): "right-product",
    ("pcre2", "pcre:pcre2"): "right-product",
    ("zlib@1", "zlib:zlib"): "right-product",
    ("zlib@0", "zlib:zlib"): "collision",              # CVEs describe the C library's own versions
    ("warp", "cloudflare:warp"): "collision",          # Cloudflare WARP VPN client
    ("parallel", "gnu:parallel"): "collision",         # GNU Parallel
    ("void", "void_project:void"): "collision",        # a PHP blog
    ("cookie", "cookie_project:cookie"): "collision",  # the Rust cookie crate
    ("foundation", "foundation:foundation"): "collision",  # Zurb Foundation
    ("hedgehog", "dns-stats:hedgehog"): "collision",   # dns-stats hedgehog (C++)
    ("async", "async_project:async"): "collision",     # the npm Async library
    ("cmark-gfm", "github:cmark-gfm"): "questionable",  # vendors the C project
}


def reviewed_key(name, version):
    n = (name or "").lower()
    return f"zlib@{(version or '').split('.')[0]}" if n == "zlib" else n


def vendor_product(cpe):
    """Fields 4-5 of a CPE 2.3 name: cpe:2.3:a:<vendor>:<product>:..."""
    p = cpe.split(":")
    return f"{p[3]}:{p[4]}" if len(p) > 4 else None


# --- link closure -------------------------------------------------------------


def link_closure(p):
    """(link closure, Haskell universe, root) from link-closure.nix's `result`."""
    d = load(p)
    if not (d.get("root") or {}).get("has-cabal-deps"):
        print(f"classify.py: {p}: the root has no getCabalDeps, so the link closure is empty "
              "rather than measured. Refusing to build a truth file that would silently omit every HSEC.",
              file=sys.stderr)
        sys.exit(2)
    link = [(x["pname"], x["version"]) for x in d.get("packages", [])]
    universe = link + [(x["pname"], x["version"]) for x in d.get("universe", [])]
    return link, universe, d["root"]


def link_cdx(args):
    pkgs, _, root = link_closure(args.link_closure)
    cs = [{"type": "library", "name": n, "version": v, "bom-ref": f"pkg:hackage/{n}@{v}", "purl": f"pkg:hackage/{n}@{v}"}
          for n, v in sorted(set(pkgs))]
    doc = {"bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1, "components": cs}
    with open(args.out, "w") as f:
        json.dump(doc, f, indent=1)
    print(f"link-cdx: {len(cs)} pkg:hackage components for {root.get('pname')} {root.get('version')}")


# --- truth --------------------------------------------------------------------


def truth(args):
    _, universe, root = link_closure(args.link_closure)
    hk = {(n.lower(), v) for n, v in universe}
    if root.get("pname") and root.get("version"):
        hk.add((root["pname"].lower(), root["version"]))

    findings = collections.OrderedDict()
    excluded = {}
    no_cpe = []
    disagree = []

    def add(row, cpe=None, purl=None):
        key = (row["canon"], row["nname"], row["version"] or "")
        f = findings.setdefault(key, {"id": row["canon"], "name": row["name"], "version": row["version"] or "",
                                      "cpe": set(), "purl": set()})
        if cpe:
            f["cpe"].add(cpe)
        if purl:
            f["purl"].add(purl)

    for r in grype_rows(args.grype):
        verdict, why = classify(r, hk)
        for vp in sorted({vp for vp in (vendor_product(c) for c in r["cpes"]) if vp}):
            want = REVIEWED.get((reviewed_key(r["name"], r["version"]), vp))
            if want and want != verdict:
                disagree.append((r["id"], r["name"], r["version"], vp, verdict, want))
        if verdict == "right-product":
            vps = sorted({vp for vp in (vendor_product(c) for c in r["cpes"]) if vp})
            if not vps:
                no_cpe.append((r["id"], r["name"], r["version"], r["match"]))
                continue
            for vp in vps:
                add(r, cpe=vp)
        else:
            reason = "name-collision" if verdict == "collision" else "bundled-c-questionable"
            excluded[(r["canon"], r["nname"], r["version"] or "")] = {
                "id": r["canon"], "name": r["name"], "version": r["version"] or "", "reason": reason,
                "why": why, "cpe": sorted({vp for vp in (vendor_product(c) for c in r["cpes"]) if vp}),
                "excerpt": r["desc"].replace("\n", " ")[:200]}

    for r in osv_rows(args.osv):
        if (r["id"] or "").startswith("HSEC"):
            add(r, purl=f"pkg:hackage/{r['name']}@{r['version']}")

    if disagree:
        print("classify.py: the rule disagrees with a reviewed verdict, so its Haskell set or the rule "
              "itself is wrong for this target (id, name, version, cpe, rule, reviewed):", file=sys.stderr)
        for x in sorted(set(disagree)):
            print(f"  {x}", file=sys.stderr)
        sys.exit(2)
    if no_cpe:
        print("classify.py: right-product grype findings with no CPE in matchDetails; the proxy "
              "cannot match them, and dropping them would shrink the truth silently:", file=sys.stderr)
        for x in sorted(no_cpe, key=lambda t: tuple(str(y) for y in t)):
            print(f"  {x}", file=sys.stderr)
        sys.exit(2)

    out_findings = []
    for key in sorted(findings, key=lambda k: (k[0], k[1], k[2])):
        f = findings[key]
        matcher = {}
        if f["cpe"]:
            cpes = sorted(f["cpe"])
            matcher["cpe"] = cpes[0] if len(cpes) == 1 else cpes
        if f["purl"]:
            purls = sorted(f["purl"])
            matcher["purl"] = purls[0] if len(purls) == 1 else purls
        out_findings.append({"id": f["id"], "name": f["name"], "version": f["version"], "matcher": matcher})

    tools = dict(t.split("=", 1) for t in args.tool or [])
    doc = {
        "schema": 1,
        "produced": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d"),
        "tools": dict(sorted(tools.items())),
        "target-pin": args.target_pin,
        "findings": out_findings,
        "excluded": [excluded[k] for k in sorted(excluded)],
    }
    with open(args.out, "w") as f:
        json.dump(doc, f, indent=1, sort_keys=False)
        f.write("\n")
    by_reason = collections.Counter(e["reason"] for e in doc["excluded"])
    hsec = sum(1 for f in out_findings if "purl" in f["matcher"])
    print(f"truth: {len(out_findings)} findings ({hsec} with a purl matcher), "
          f"excluded {dict(sorted(by_reason.items()))}, Haskell set {len(hk)} (name, version) pairs")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("truth")
    t.add_argument("--grype", required=True)
    t.add_argument("--osv", required=True)
    t.add_argument("--link-closure", required=True)
    t.add_argument("--target-pin", required=True)
    t.add_argument("--tool", action="append")
    t.add_argument("--out", required=True)
    lc = sub.add_parser("link-cdx")
    lc.add_argument("--link-closure", required=True)
    lc.add_argument("--out", required=True)
    args = ap.parse_args()
    {"truth": truth, "link-cdx": link_cdx}[args.cmd](args)


if __name__ == "__main__":
    main()
