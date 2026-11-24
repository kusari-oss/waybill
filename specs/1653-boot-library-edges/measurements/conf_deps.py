# Parse ghc-pkg .conf text into (name, version, [dep names]); unit ids lose their -hash suffix.
import re, sys
def field(text, name):
    m = re.search(r'^' + re.escape(name) + r':[ \t]*(.*(?:\n[ \t]+.*)*)', text, re.M)
    return m.group(1).split() if m else []
def unit_name(uid):
    # ghc-prim-0.12.0-3d5d -> ghc-prim ; base-4.20.2.0 -> base ; rts -> rts
    parts = uid.split('-')
    out = []
    for p in parts:
        if re.fullmatch(r'\d+(\.\d+)*', p): break
        out.append(p)
    return '-'.join(out) if out else uid
def parse(text):
    name = (field(text, 'name') or ['?'])[0]
    version = (field(text, 'version') or [''])[0]
    deps = sorted({unit_name(d) for d in field(text, 'depends')})
    return name, version, deps
