#!/usr/bin/env python3
"""Cost and safety of adding meta.identifiers to milestone 1281's meta eval.

Variants differ ONLY in the per-row attribute set `v`:
  original : licenses only (verbatim from eval/licenses.rs::licenses_expr)
  naive    : + version, + cpe = p.meta.identifiers.cpe or null
  guarded  : + version, + cpe inside its own tryEval
Each is run N times after one warm-up; reports min/median/max wall time,
row count, rows carrying a license, rows carrying a cpe.
"""
import json, subprocess, sys, time, statistics
REV = "a799d3e3886da994fa307f817a6bc705ae538eeb"; SYS = "aarch64-darwin"; N = 3
names = [l.strip() for l in open("names.txt") if l.strip()]
ROOTS = '{ id = "pkgs"; set = pkgs; } { id = "haskellPackages"; set = pkgs.haskellPackages; }'
V = {
 "original": 'licenses = lic (p.meta.license or null);',
 "naive":    'licenses = lic (p.meta.license or null); version = p.version or null; cpe = p.meta.identifiers.cpe or null;',
 "guarded":  'licenses = lic (p.meta.license or null); version = p.version or null; '
             'cpe = let c = builtins.tryEval (p.meta.identifiers.cpe or null); in if c.success then c.value else null;',
}
def expr(extra):
    lst = " ".join(f'"{n}"' for n in names)
    return f'''let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{REV}").legacyPackages.{SYS};
  roots = [ {ROOTS} ];
  one = l:
    if builtins.isAttrs l then {{ spdx = l.spdxId or null; name = l.shortName or l.fullName or null; }}
    else if builtins.isString l then {{ spdx = null; name = l; }}
    else {{ spdx = null; name = null; }};
  lic = l: if builtins.isList l then map one l else if l == null then [ ] else [ (one l) ];
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = {{ attr = name; root = root.id; drv = baseNameOf p.drvPath; out = "${{p}}"; {extra} }};
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) [ {lst} ]'''
def run(e):
    t = time.monotonic()
    out = subprocess.run(["nix","eval","--json","--option","allow-import-from-derivation","false","--expr",e],
                         capture_output=True, text=True, timeout=900)
    dt = time.monotonic() - t
    if out.returncode: sys.exit(f"eval failed: {out.stderr[-400:]}")
    return dt, json.loads(out.stdout)
print(f"{len(names)} names, N={N} after 1 warm-up per variant")
for k, extra in V.items():
    e = expr(extra); run(e)
    ts = []; rows = None
    for _ in range(N):
        dt, rows = run(e); ts.append(dt)
    lic = sum(1 for r in rows if r["licenses"]); cpe = sum(1 for r in rows if r.get("cpe"))
    print(f"{k:9} min={min(ts):.2f}s median={statistics.median(ts):.2f}s max={max(ts):.2f}s  rows={len(rows)} with-license={lic} with-cpe={cpe}")
    json.dump(rows, open(f"rows-{k}.json","w"), indent=0)
