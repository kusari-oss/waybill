#!/usr/bin/env python3
"""#1412 — per-member license accounting for one `--nix-closure` scan.

Population and emission are read from waybill, not modelled:
  * DUMP  — `waybill --offline debug closure-dump` (rows with
            `in_coverage_population`, `drv`, `pname_with_fallback`, `version`)
  * SBOM  — the CycloneDX document `waybill sbom scan --nix-closure` emitted
  * C184  — that document's `licenses.received` / `withheld-ambiguous`

The rows are produced by evaluating the production expression VERBATIM: the
`LICENSE_PROJECTION` and `licenses_expr` text is lifted out of
`waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs` at run time, so the
probe cannot drift from the reader's projection. The names evaluated are
production's: every derivation's `pname()` (`pname_with_fallback`) passing
`is_safe_attribute_name`.

The join is by `.drv` basename across all rows, as `closure/licenses.rs::index`
does. A row is "usable" when its projected declaration is non-empty and every
leaf of every declaration has an spdx id or a name — the render step's own
precondition. FR-015 disagreement (different declaration trees for one `.drv`)
is reported as its own class.

Usage:
  DUMP=dump.json SBOM=sbom.cdx.json SRC=<repo root> [NIX_REV=..] [NIX_SYSTEM=..] \
      python3 -I -B account.py
"""
import json, os, re, subprocess, sys

DUMP = os.environ.get("DUMP") or sys.exit("set DUMP")
SBOM = os.environ.get("SBOM") or sys.exit("set SBOM")
SRC = os.environ.get("SRC") or sys.exit("set SRC (repo root)")
REV = os.environ.get("NIX_REV", "a799d3e3886da994fa307f817a6bc705ae538eeb")
SYS = os.environ.get("NIX_SYSTEM", "aarch64-darwin")

# ---- the production expression, lifted verbatim from the Rust source -------
rs = open(os.path.join(SRC, "waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs")).read()
projection = re.search(r'const LICENSE_PROJECTION: &str = r#"(.*?)"#;', rs, re.S).group(1)
body = re.search(r'fn licenses_expr\(.*?format!\(\s*r#"(.*?)"#', rs, re.S).group(1)
roots = re.search(r'const ROOTS: &\[\(&str, &str\)\] = &\[(.*?)\];', rs, re.S).group(1)
roots = re.findall(r'\("([^"]+)", "([^"]+)"\)', roots)

dump = json.load(open(DUMP))
rows = dump["rows"]
safe = re.compile(r"^[A-Za-z0-9._-]+$")  # invoke.rs::is_safe_attribute_name
names = sorted({r["pname_with_fallback"] for r in rows
                if r.get("pname_with_fallback") and safe.match(r["pname_with_fallback"])})

expr = body
for k, v in {
    "{revision}": REV, "{system}": SYS, "{LICENSE_PROJECTION}": projection,
    "{roots}": " ".join(f'{{ id = "{i}"; set = {p}; }}' for i, p in roots),
    "{list}": " ".join(f'"{n}"' for n in names),
}.items():
    expr = expr.replace(k, v)
expr = expr.replace("{{", "{").replace("}}", "}")  # format! escapes

cache = os.environ.get("ROWS_CACHE")
if cache and os.path.exists(cache):
    meta = json.load(open(cache))
else:
    out = subprocess.run(["nix", "eval", "--json", "--option", "allow-import-from-derivation",
                          "false", "--expr", expr], capture_output=True, text=True, timeout=1800)
    if out.returncode:
        sys.exit(f"eval failed: {out.stderr[-1500:]}")
    meta = json.loads(out.stdout)
    if cache:
        json.dump(meta, open(cache, "w"))

def leaves_ok(t):
    if t.get("op") is None:
        return bool((t.get("spdx") or "").strip() or (t.get("name") or "").strip())
    return bool(t.get("terms")) and all(leaves_ok(x) for x in t["terms"])

by_drv = {}
for r in meta:
    by_drv.setdefault(r["drv"], []).append(r)

# ---- emission, read from the document --------------------------------------
doc = json.load(open(SBOM))
def walk(cs):
    for c in cs or []:
        yield c
        yield from walk(c.get("components"))
comps = list(walk(doc.get("components")))
meta_comp = (doc.get("metadata") or {}).get("component")
if meta_comp:
    comps.append(meta_comp)
emitted = {}
for c in comps:
    lic = []
    for l in c.get("licenses") or []:
        if "expression" in l:
            lic.append(l["expression"])
        elif "license" in l:
            lic.append(l["license"].get("id") or l["license"].get("name"))
    emitted.setdefault((c.get("name"), c.get("version") or ""), []).append(lic)

c184 = None
for p in (doc.get("metadata") or {}).get("properties") or []:
    if "nix-closure" in p.get("name", "") and "licenses" in (p.get("value") or ""):
        try:
            v = json.loads(p["value"])
        except Exception:
            continue
        c184 = v.get("licenses") or c184

def render(t, top=True):
    """`closure/licenses.rs::render`, for comparison only."""
    if t.get("op") is None:
        return (t.get("spdx") or t.get("name")).strip()
    op, terms = t["op"], t["terms"]
    br = lambda x: (f"({render(x, False)})" if (x.get("op") in ("AND", "OR") and len(x["terms"]) > 1)
                    or x.get("op") == "WITH" else render(x, False))
    if op in ("AND", "OR"):
        return render(terms[0]) if len(terms) == 1 else f" {op} ".join(br(x) for x in terms)
    if op == "WITH":
        return f"{br(terms[0])} WITH {br(terms[1])}"
    return f"{br(terms[0])}+"

members = [r for r in rows if r["in_coverage_population"]]
table, counts, value_mismatch = [], {}, []
for m in members:
    hits = by_drv.get(m["drv"], [])
    if not hits:
        cls = "no-row"
    else:
        shapes = {json.dumps(h["licenses"], sort_keys=True) for h in hits}
        if len(shapes) > 1:
            cls = "fr015-disagree"
        elif not hits[0]["licenses"]:
            cls = "row-null-license"
        elif not all(leaves_ok(t) for t in hits[0]["licenses"]):
            cls = "row-unrenderable"
        else:
            cls = "row-usable"
    key = (m["pname_with_fallback"], m.get("version") or "")
    em = emitted.get(key)
    em_lic = None if em is None else [x for l in em for x in l]
    counts[cls] = counts.get(cls, 0) + 1
    if cls == "row-usable":
        state = "absent" if em is None else ("licensed" if em_lic else "UNLICENSED")
        # Licensed with what? A component merged from build variants, or into
        # another reader's component, could carry a license that is not this
        # row's. Compare as text, case-folded (canonicalisation may re-case).
        if em_lic:
            want = [render(t) for t in hits[0]["licenses"]]
            got = {x.lower() for x in em_lic}
            if not all(w.lower() in got for w in want):
                value_mismatch.append({"member": f"{key[0]} {key[1]}", "row": want, "emitted": em_lic})
        counts["row-usable/" + state] = counts.get("row-usable/" + state, 0) + 1
        if state != "licensed":
            table.append({"member": f"{key[0]} {key[1]}", "drv": m["drv"],
                          "row": hits[0]["licenses"], "row_attr": [h["attr"] + "@" + h["root"] for h in hits],
                          "emitted": em_lic, "state": state})

# What a NAME join (the #1404-era probe's method) would have counted, and the
# members it counts that the store-path join does not reach.
by_attr = {}
for r in meta:
    by_attr.setdefault(r["attr"], []).append(r)
name_hits = [m for m in members if by_attr.get(m["pname_with_fallback"])]
name_only = [m for m in name_hits if not by_drv.get(m["drv"])]
nulls = [m for m in members if by_drv.get(m["drv"]) and not by_drv[m["drv"]][0]["licenses"]]

# The #1404 probe (`specs/1404-nix-compound-licenses/measurements/classify.py`)
# counted "members with a row" over EVERY closure derivation, not over the
# coverage population C184 counts. These are the derivations it counted that
# C184 does not.
outside = [{"derivation": f'{r["pname_with_fallback"]} {r.get("version") or ""}', "drv": r["drv"],
            "c182_role": r["c182_role"], "emits_as_component": r["emits_as_component"],
            "member_role": r["member_role"], "declared_pname": r["declared_pname"],
            "row_license": by_drv[r["drv"]][0]["licenses"],
            "emitted": emitted.get((r["pname_with_fallback"], r.get("version") or ""))}
           for r in rows if not r["in_coverage_population"] and by_drv.get(r["drv"])]
all_drv_hits = sum(1 for r in rows if by_drv.get(r["drv"]))

def describe(m):
    return {"member": f'{m["pname_with_fallback"]} {m.get("version") or ""}', "drv": m["drv"],
            "rows_under_name": [{"drv": r["drv"], "version": r["version"], "root": r["root"],
                                 "has_license": bool(r["licenses"])}
                                for r in by_attr.get(m["pname_with_fallback"], [])]}

print(json.dumps({"wanted": len(members), "classes": counts, "C184": c184,
                  "rows_evaluated": len(meta), "names_evaluated": len(names),
                  "usable_but_not_licensed": table,
                  "licensed_but_not_with_the_row_text": value_mismatch,
                  "probe_1404_population": {"derivations": len(rows),
                                            "derivations_with_a_row_by_drv": all_drv_hits,
                                            "outside_coverage_population": outside},
                  "name_join": {"members_with_a_row_by_name": len(name_hits),
                                "of_which_not_reached_by_drv": len(name_only)},
                  "reached_by_drv_with_null_license": [describe(m) for m in nulls]}, indent=1))
