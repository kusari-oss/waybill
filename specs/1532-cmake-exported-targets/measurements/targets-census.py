"""Census of CMake exported-targets files in the TheRock SDK fixture (spec 1532, research R1).

Usage: python3 -I -B targets-census.py <fixture-clone> <all-files.txt> <generic-purls.txt>

  <fixture-clone>      a clone of kusari-sandbox/waybill-fixture-therock-sdk
  <all-files.txt>      its listing/all-files.txt (type size path per line)
  <generic-purls.txt>  the `measure` run's branch-generic-purls.txt (the #1531 components)

This is a measuring script, not a model of the reader: it resolves
${_IMPORT_PREFIX} the way CMake's generated files define it, and reports what
the exported targets name. waybill's own output is the authority (SC-003).
"""

import collections
import json
import os
import re
import sys

clone, listing, purls = sys.argv[1], sys.argv[2], sys.argv[3]
root = os.path.join(clone, "sdk")

kinds, sizes = {}, {}
for line in open(listing):
    parts = line.rstrip("\n").split(" ", 2)
    if len(parts) == 3:
        path = parts[2].split(" -> ")[0].lstrip("./")
        kinds[path] = parts[0]
        sizes[path] = int(parts[1]) if parts[1].isdigit() else 0

components = {
    line.strip()[len("pkg:generic/"):].split("@")[0].lower() for line in open(purls) if line.strip()
}

pattern = re.compile(r"((Targets|Exports)|-targets)(-[A-Za-z]+)?\.cmake$")
files = [os.path.join(d, f) for d, _, fs in os.walk(root) for f in fs if pattern.search(f)]

raise_by_dir = collections.defaultdict(int)
for fp in files:
    text = open(fp, errors="replace").read()
    n = len(re.findall(r'get_filename_component\(_IMPORT_PREFIX "\$\{_IMPORT_PREFIX\}" PATH\)', text))
    raise_by_dir[os.path.dirname(fp)] = max(raise_by_dir[os.path.dirname(fp)], n)

owner, forms, configs = {}, collections.Counter(), collections.Counter()
for fp in files:
    m = re.search(r"-([A-Za-z]+)\.cmake$", fp)
    if m and m.group(1).lower() not in ("targets", "exports"):
        configs[m.group(1).lower()] += 1
    d = os.path.dirname(fp)
    prefix = os.path.relpath(d, root)
    for _ in range(raise_by_dir[d]):
        prefix = os.path.dirname(prefix)
    text = open(fp, errors="replace").read()
    for st in re.finditer(r"set_target_properties\((\S+) PROPERTIES(.*?)\n\s*\)", text, re.S):
        for loc in re.finditer(r'IMPORTED_LOCATION(?:_[A-Z]+)? "([^"]+)"', st.group(2)):
            raw = loc.group(1)
            forms[re.match(r"(\$\{[A-Za-z_]+\}|/|.)", raw).group(1)] += 1
            path = os.path.normpath(raw.replace("${_IMPORT_PREFIX}", prefix)).lstrip("/")
            owner[path] = os.path.relpath(d, root)


def version_component(d):
    for f in os.listdir(os.path.join(root, d)):
        m = re.match(r"(.+?)(ConfigVersion|-config-version)\.cmake$", f)
        if m and m.group(1).lower() in components:
            return m.group(1).lower()
    return None


shipped = {p: d for p, d in owner.items() if p in sizes}
via_version = sum(1 for d in shipped.values() if version_component(d))
via_name = sum(
    1 for d in shipped.values() if not version_component(d) and os.path.basename(d).lower() in components
)
regular = sum(sizes[p] for p in shipped if kinds.get(p) == "-")
print(json.dumps({
    "targets_files": len(files),
    "configurations": dict(configs),
    "location_forms": dict(forms),
    "locations": len(owner),
    "shipped": len(shipped),
    "not_shipped": len(owner) - len(shipped),
    "attached_via_version_file": via_version,
    "attached_via_same_name": via_name,
    "no_package_version": len(shipped) - via_version - via_name,
    "shipped_regular_bytes": regular,
}, indent=2, sort_keys=True))
