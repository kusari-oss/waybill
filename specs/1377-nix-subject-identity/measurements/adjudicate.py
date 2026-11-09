#!/usr/bin/env python3
"""Adjudication pre-pass over grype's sbomnix-only findings.

For each distinct (component, version, CVE) that grype reports on sbomnix's
buildtime SBOM but not on waybill's, record:
  why-waybill-missed : no-component | component-no-cpe | component-has-cpe
                       (and whether versions agree)
  patched-by-nix     : the derivation carries a patch whose name or content
                       names the CVE -> the finding is a false positive
  junk-cpe           : sbomnix's CPE has an empty version or is a file
                       (patch / tarball) -> heuristic over-reach
  runtime            : the component is in Nix's runtime closure of some target
Patch content is grepped only where the patch file is present in the store.
"""
import json, os, re, subprocess, collections

W = "/Users/mlieberman/Projects/mikebom/target/nix-vuln-spike"
TARGETS = "curl openssh httpie gh ripgrep moat".split()
r = json.load(open(f"{W}/findings.json"))
PY = re.compile(r"^python3(\.\d+)?-")
norm = lambda n: PY.sub("", (n or "").lower())


def comps(t, tool):
    return json.load(open(f"{W}/out/{t}/{tool}.cdx.json")).get("components", [])


drv_cache = {}
def drv_patches(drv):
    if drv in drv_cache:
        return drv_cache[drv]
    out = []
    try:
        j = json.loads(subprocess.run(["nix", "derivation", "show", drv], capture_output=True,
                                      text=True, timeout=60).stdout or "{}")
        # Determinate Nix derivation-show format v4 nests under "derivations";
        # reading the wrapper as a derivation silently yields no patches.
        ds = j.get("derivations", j) if isinstance(j, dict) else {}
        for d in ds.values():
            env = d.get("env", {}) or {}
            sa = d.get("structuredAttrs") or {}
            raw = env.get("patches") or sa.get("patches") or ""
            if isinstance(raw, list):
                raw = " ".join(map(str, raw))
            out += [p for p in str(raw).split() if p]
    except Exception:
        pass
    drv_cache[drv] = out
    return out


def patch_names_cve(patches, cve):
    c = cve.lower()
    for p in patches:
        if c in p.lower():
            return "name"
        if os.path.isfile(p):
            try:
                if c in open(p, errors="ignore").read().lower():
                    return "content"
            except Exception:
                pass
    return None


rows = {}
for t in TARGETS:
    sb = comps(t, "sbomnix-bt"); wb = comps(t, "waybill")
    sb_by = collections.defaultdict(list)
    for c in sb:
        sb_by[(norm(c["name"]), c.get("version"))].append(c)
    wb_by = collections.defaultdict(list)
    for c in wb:
        wb_by[norm(c["name"])].append(c)
    rt = set(r[t]["grype"].get("runtime_names") or [])
    for name, ver, cve in r[t]["grype"]["sbomnix_only"]:
        key = (name, ver, cve)
        row = rows.setdefault(key, {"targets": set(), "runtime_in": set()})
        row["targets"].add(t)
        if name in rt:
            row["runtime_in"].add(t)
        if "why" not in row:
            w = wb_by.get(name, [])
            if not w:
                row["why"] = "no-component"
            else:
                same_v = [c for c in w if c.get("version") == ver]
                pool = same_v or w
                has_cpe = any(c.get("cpe") for c in pool)
                row["why"] = ("component-has-cpe" if has_cpe else "component-no-cpe") + \
                             ("" if same_v else "/version-differs")
            sc = sb_by.get((name, ver), [])
            cpe = (sc[0].get("cpe") if sc else "") or ""
            parts = cpe.split(":")
            row["junk_cpe"] = bool(sc and (len(parts) > 5 and parts[5] in ("", "*") or
                                           re.search(r"\.(patch|diff|tar|gz|bz2|xz|zip)$", name)))
            drvs = [p["value"] for c in sc for p in c.get("properties", []) if p["name"] == "nix:drv_path"]
            hit = None
            for d in drvs:
                hit = patch_names_cve(drv_patches(d), cve) or hit
            row["patched"] = hit

summary = collections.Counter()
for (n, v, c), row in rows.items():
    fp = "patched-by-nix" if row["patched"] else ("junk-cpe" if row["junk_cpe"] else "unresolved")
    summary[(row["why"], fp, "runtime" if row["runtime_in"] else "build-only")] += 1

print(f"distinct sbomnix-only grype findings: {len(rows)}\n")
print(f"{'why waybill missed it':34} {'verdict pre-pass':16} {'ships?':11} count")
for k, n in sorted(summary.items(), key=lambda kv: -kv[1]):
    print(f"{k[0]:34} {k[1]:16} {k[2]:11} {n}")

json.dump({f"{n}|{v}|{c}": {**row, "targets": sorted(row["targets"]),
                             "runtime_in": sorted(row["runtime_in"])}
           for (n, v, c), row in rows.items()},
          open(f"{W}/adjudication.json", "w"), indent=1)
print("\nby component (runtime-relevant only):")
byc = collections.Counter((n, v, rows[(n, v, c)]["why"]) for (n, v, c) in rows if rows[(n, v, c)]["runtime_in"])
for (n, v, why), k in byc.most_common(25):
    print(f"  {n:22} {v or '':14} {why:34} {k} CVEs")
