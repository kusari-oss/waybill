"""List the GHC units (packages) that have code defined in a Haskell executable.

usage: nm <binary> | python3 -I -B q6-linked-units.py > <out>

GHC symbol names are z-encoded `<unit-id>_<Module>_<name>_<kind>`, e.g.
`_aesonzm2zi2zi4zi1zmNQ4HYX6Wd2H2KWSK5cg0l_DataziAesonziTypesziInternal_Object_con_info`.
Only *defined* symbols are counted (nm types other than U), so a unit listed here had
code or data linked into the binary and survived dead-stripping. Units without a version
(base, ghc-prim, ...) are compiler-supplied boot units. Output: `<name-version>\t<symbols>`.
"""
import re
import sys
from collections import Counter

Z = {"zm": "-", "zi": ".", "zu": "_", "zz": "z", "ZZ": "Z", "zd": "$", "zp": "+",
     "zh": "#", "zl": "<", "zg": ">", "zc": ":", "zs": "/", "zv": "|", "zt": "*",
     "zq": "'", "zr": "%", "zn": "^", "za": "&", "ze": "=", "zx": "!", "zb": "\\",
     "ZL": "(", "ZR": ")", "ZM": "[", "ZN": "]", "ZC": ":"}


def zdecode(s):
    out, i = [], 0
    while i < len(s):
        two = s[i:i + 2]
        if two in Z:
            out.append(Z[two]); i += 2
        else:
            out.append(s[i]); i += 1
    return "".join(out)


units = Counter()
for line in sys.stdin:
    parts = line.split()
    if len(parts) < 3 or parts[-2] in ("U", "u"):
        continue
    sym = parts[-1].lstrip("_")
    m = re.match(r"([A-Za-z0-9]+?)_[A-Z]", sym)
    if not m:
        continue
    unit = zdecode(m.group(1))
    # name-version-hash -> name-version ; boot units have no version
    mv = re.match(r"^(.*?-\d+(?:\.\d+)*)(?:-[A-Za-z0-9]+)?$", unit)
    if mv:
        units[mv.group(1)] += 1
    elif re.fullmatch(r"[a-z][a-z0-9-]*", unit):
        units[unit + " (boot, unversioned)"] += 1
for u, n in sorted(units.items(), key=lambda kv: kv[0].lower()):
    print(f"{u}\t{n}")
