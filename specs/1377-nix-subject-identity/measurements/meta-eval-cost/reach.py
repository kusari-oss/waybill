#!/usr/bin/env python3
"""Independent reach count for SC-009.

The POPULATION comes from waybill's own reader, not from a model of it:
`waybill debug closure-dump --closure-json <nix derivation show -r output>`
reports every derivation with `emits_as_component`, `pname_with_fallback` (the
name milestone 1281 evaluates), `version` and `drv`. Re-deriving which
derivations waybill emits would be modelling the reader.

The JOIN is computed here, independently: the guarded meta expression
(probe.py's `guarded` variant) is evaluated for those names, and each emitted
member is matched by `.drv` basename, then by pname + version. waybill's
C184 `identifiers` must report the same counts.

Usage:
  CLOSURE_DUMP=<closure-dump json> [NIX_REV=...] [NIX_SYSTEM=...] python3 -I -B reach.py
"""
import json, os, re, subprocess, sys

dump_path = os.environ.get("CLOSURE_DUMP") or sys.exit("set CLOSURE_DUMP (see quickstart.md)")
REV = os.environ.get("NIX_REV", "a799d3e3886da994fa307f817a6bc705ae538eeb")
SYS = os.environ.get("NIX_SYSTEM", "aarch64-darwin")
SAFE = re.compile(r"^[A-Za-z0-9._+-]+$")

rows = json.load(open(dump_path))["rows"]
# The population milestone 1281 counts as `licenses.wanted`
# (closure/emit.rs::emitted_drv_keys): emitted, carrying a pname, and either
# not a milestone-1282 member or one whose member role is an edge endpoint
# (`source` / `package`). Checked: 670 on openssh, equal to C184's `wanted`.
members = [r for r in rows if r.get("emits_as_component") and r.get("pname_with_fallback")
           and (r.get("member_role") is None or r["member_role"] in ("source", "package"))]
names = sorted({m["pname_with_fallback"] for m in members if SAFE.match(m["pname_with_fallback"] or "")})

lst = " ".join(f'"{n}"' for n in names)
# `haslic` uses milestone 1281's own license parse (`one`/`lic` from
# eval/licenses.rs), not mere presence of `meta.license`: a license that
# parses to nothing usable is not counted by 1281 either.
expr = f'''let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{REV}").legacyPackages.{SYS};
  roots = [ {{ id = "pkgs"; set = pkgs; }} {{ id = "haskellPackages"; set = pkgs.haskellPackages; }} ];
  one = l:
    if builtins.isAttrs l then {{ spdx = l.spdxId or null; name = l.shortName or l.fullName or null; }}
    else if builtins.isString l then {{ spdx = null; name = l; }}
    else {{ spdx = null; name = null; }};
  lic = l: if builtins.isList l then map one l else if l == null then [ ] else [ (one l) ];
  usable = ls: builtins.any (x: x.spdx != null || x.name != null) ls;
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = {{ attr = name; drv = baseNameOf p.drvPath; version = p.version or null;
              cpe = let c = builtins.tryEval (p.meta.identifiers.cpe or null); in if c.success then c.value else null;
              haslic = usable (lic (p.meta.license or null)); }};
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) [ {lst} ]'''
out = subprocess.run(["nix", "eval", "--json", "--option", "allow-import-from-derivation", "false",
                      "--expr", expr], capture_output=True, text=True, timeout=900)
if out.returncode:
    sys.exit(f"eval failed: {out.stderr[:600]}")
meta = json.loads(out.stdout)
by_name = {}
for r in meta:
    by_name.setdefault(r["attr"], []).append(r)

by_drv = by_nv = declared = drv_with_license = ambiguous = 0
for m in members:
    cands = by_name.get(m["pname_with_fallback"], [])
    hit = next((r for r in cands if r["drv"] == m["drv"]), None)
    if hit:
        by_drv += 1
        drv_with_license += bool(hit.get("haslic"))
    else:
        nv = [r for r in cands if r["version"] is not None and r["version"] == m.get("version")]
        if not nv:
            continue                                  # unreached
        by_nv += 1
        if len({r["cpe"] for r in nv}) > 1:
            ambiguous += 1                            # reached, CPE withheld
            continue
        hit = nv[0]
    declared += bool(hit.get("cpe"))

received = by_drv + by_nv
print(json.dumps({"wanted": len(members), "received": received, "by-drv": by_drv,
                  "by-name-version": by_nv, "declared": declared, "ambiguous": ambiguous,
                  "control-by-drv-with-license (must equal C184 licenses.received)": drv_with_license,
                  "unreached": len(members) - received}, indent=1))
