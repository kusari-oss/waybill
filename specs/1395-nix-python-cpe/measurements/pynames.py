"""List derivations whose pname/name looks like a Python interpreter.

Prints target, pname, version, name for every derivation whose `env.pname`
(or name) starts with 'python' and does NOT contain a '-' after the python
prefix's version (i.e. interpreter-shaped, not python3.13-<pkg>).
"""
import json, os, re, sys

for t in sys.argv[1:]:
    d = json.load(open(f"{os.environ['DRV_DIR']}/{t}.json"))
    if "derivations" in d:
        d = d["derivations"]
    rows = set()
    for k, v in d.items():
        env = v.get("env", {})
        name = v.get("name") or env.get("name", "")
        pname = env.get("pname", "")
        ver = env.get("version", "")
        sa = env.get("__json")
        if sa and not pname:
            try:
                j = json.loads(sa); pname = j.get("pname", ""); ver = j.get("version", "")
            except Exception:
                pass
        if re.match(r"^python", pname or name, re.I):
            rows.add((pname, ver, name))
    interp = sorted(r for r in rows if re.match(r"^python[0-9.]*(-minimal)?$", r[0] or ""))
    other = sorted(r for r in rows if r not in interp)
    print(f"## {t}: {len(rows)} python-prefixed derivations, {len(interp)} interpreter-shaped")
    for r in interp:
        print("  INTERP", r)
    for r in other[:15]:
        print("  other ", r)
    if len(other) > 15:
        print(f"  ... {len(other) - 15} more")
