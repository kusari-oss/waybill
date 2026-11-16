"""Census of embedded-path attribution under spec 1534's clarified rules (waybill #1532).

Usage: python3 -I -B attribution.py <extracted-root> <binary-list> <versioned-names> <out.json>

Rules measured (spec 1534 clarifications):
- only compiled-source paths count (.c .cc .cpp .cxx .c++ .hip .cu); headers never;
- a path names package N when a directory component equals N exactly
  (case-insensitive) and lies outside any vendoring subtree;
- a path under third_party/, third-party/, thirdparty/, 3rdparty/, external/,
  extern/, vendor/<name>/ or _deps/<name>-src/ names a bundled tree <name>
  instead, and never attributes;
- toolchain/system paths (/usr/, /builddir/, /opt/) never attribute.

Strings are NUL-terminated runs read from three regions, recorded separately:
`ro` = .rodata then .data.rel.ro, first 16 MiB (waybill's existing string
region and cap); `ro_full` = the same sections uncapped; `debug` =
.debug_str + .debug_line_str. Section offsets come from `readelf -S -W`.
"""

import collections
import json
import os
import re
import subprocess
import sys

root, listfile, namesfile, out_path = sys.argv[1:5]
NAMES = {l.strip() for l in open(namesfile) if l.strip()}
COMPILED = re.compile(r"\.(c|cc|cpp|cxx|c\+\+|hip|cu)$")
HEADER = re.compile(r"\.(h|hh|hpp|hxx|inl|inc|cuh|ipp|tcc)$")
VENDOR = {"third_party", "third-party", "thirdparty", "3rdparty", "external", "extern", "vendor"}
TOOLCHAIN = ("/usr/", "/builddir/", "/opt/")
PATH = re.compile(rb"/[A-Za-z0-9._+@~-]+(?:/[A-Za-z0-9._+@~-]+)+")
CAP = 16 * 1024 * 1024


def sections(path):
    out = {}
    txt = subprocess.run(["readelf", "-S", "-W", path], capture_output=True, text=True, errors="replace").stdout
    for line in txt.splitlines():
        m = re.match(r"\s*\[\s*\d+\]\s+(\S+)\s+\S+\s+[0-9a-f]+\s+([0-9a-f]+)\s+([0-9a-f]+)", line)
        if m:
            out[m.group(1)] = (int(m.group(2), 16), int(m.group(3), 16))
    return out


def region(f, secs, names, cap=None):
    buf = bytearray()
    for n in names:
        if n in secs:
            off, size = secs[n]
            f.seek(off)
            take = size if cap is None else max(0, min(size, cap - len(buf)))
            buf += f.read(take)
    return bytes(buf)


EXAMPLES = collections.defaultdict(list)


def analyse(blob):
    """(attributing names, header-only names, bundled names, toolchain-only?, outermost-only names) for one region."""
    names, header_names, bundled, outer = set(), set(), set(), set()
    any_src = False
    for s in blob.split(b"\0"):
        for m in PATH.finditer(s):
            p = m.group(0).decode("ascii", "replace")
            p = p.split(":", 1)[0]
            comp = bool(COMPILED.search(p))
            hdr = bool(HEADER.search(p))
            if not (comp or hdr):
                continue
            any_src = True
            if p.startswith(TOOLCHAIN):
                continue
            parts = p.strip("/").split("/")[:-1]
            vendored = False
            hits = []
            for i, seg in enumerate(parts):
                low = seg.lower()
                if low in VENDOR and i + 1 < len(parts):
                    bundled.add(parts[i + 1].lower())
                    vendored = True
                    break
                if low == "_deps":
                    continue
                mm = re.match(r"(.+)-src$", low)
                if i > 0 and parts[i - 1] == "_deps" and mm:
                    bundled.add(mm.group(1))
                    vendored = True
                    break
                if low in NAMES:
                    hits.append(low)
            if vendored:
                continue
            (names if comp else header_names).update(hits)
            if comp and hits:
                outer.add(hits[0])
                for h in hits:
                    if len(EXAMPLES[h]) < 4 and p not in EXAMPLES[h]:
                        EXAMPLES[h].append(p)
    return names, header_names - names, bundled, any_src, outer


files = [l.strip() for l in open(listfile) if l.strip()]
recs = []
for i, rel in enumerate(files):
    path = os.path.join(root, rel)
    if not os.path.isfile(path):
        continue
    secs = sections(path)
    rec = {"path": rel, "size": os.path.getsize(path)}
    with open(path, "rb") as f:
        regions = {
            "ro": region(f, secs, [".rodata", ".data.rel.ro"], CAP),
            "ro_full": region(f, secs, [".rodata", ".data.rel.ro"]),
            "debug": region(f, secs, [".debug_str", ".debug_line_str"]),
        }
    for k, blob in regions.items():
        n, h, b, src, o = analyse(blob)
        rec[k] = {"names": sorted(n), "header_only": sorted(h), "bundled": sorted(b), "any_source_path": src, "bytes": len(blob), "outermost": sorted(o)}
    recs.append(rec)
    if i % 50 == 0:
        print(f"{i}/{len(files)}", flush=True)
json.dump({"records": recs, "examples": EXAMPLES}, open(out_path, "w"), indent=1, sort_keys=True)

gb = lambda n: f"{n / 1e9:.3f} GB"
print(f"\n## Attribution census, {len(recs)} binaries ({gb(sum(r['size'] for r in recs))})\n")
for k in ["ro", "ro_full", "debug"]:
    one = [r for r in recs if len(r[k]["names"]) == 1]
    shared = [r for r in recs if len(r[k]["names"]) > 1]
    hdr = [r for r in recs if not r[k]["names"] and r[k]["header_only"]]
    print(f"- region `{k}`: one package {len(one)} ({gb(sum(r['size'] for r in one))}); shared {len(shared)} ({gb(sum(r['size'] for r in shared))}); header-only {len(hdr)} ({gb(sum(r['size'] for r in hdr))})")
union = lambda r, key: set(r["ro_full"][key]) | set(r["debug"][key])
att = collections.Counter(); attb = collections.Counter()
for r in recs:
    for n in union(r, "names"):
        att[n] += 1; attb[n] += r["size"]
print("\n### Packages attributed (ro_full or debug), files and bytes\n")
for n, c in att.most_common(60):
    print(f"- `{n}`: {c} files, {gb(attb[n])}")
bund = collections.Counter(n for r in recs for n in union(r, "bundled"))
print("\n### Bundled trees (files naming each)\n")
for n, c in bund.most_common(60):
    print(f"- `{n}`: {c}")
shared = [(r["path"], sorted(union(r, "names"))) for r in recs if len(union(r, "names")) > 1]
print(f"\n### Shared ({len(shared)})\n")
for p, n in shared[:30]:
    print(f"- `{p}`: {n}")
