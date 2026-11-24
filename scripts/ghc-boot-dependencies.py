#!/usr/bin/env python3
"""Produce waybill's GHC boot-library dependency table (spec 1653, #1688).

Usage:
    python3 -I -B scripts/ghc-boot-dependencies.py \
        > waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_dependencies.tsv

Spec: specs/1653-boot-library-edges/ (research R1, data-model.md).

Source: each release's own x86_64 Linux bindist,
    https://downloads.haskell.org/~ghc/<v>/ghc-<v>-x86_64-deb10-linux.tar.xz
streamed once, never written to disk. The release list is the one milestone
966's table covers (ghc_boot_libraries.tsv), so the two tables cover exactly
the same releases.

Two bindist layouts:
  - 9.4 and later (Hadrian): lib/package.conf.d/*.conf, field `depends:`,
    whose unit ids (`ghc-prim-0.12.0-3d5d`) are reduced to package names.
  - 9.0 and 9.2 (make): libraries/<lib>/dist-install/package-data.mk,
    variable `_DEP_NAMES`; the `ghc` library's is
    compiler/stage2/package-data.mk; `rts` is rts/package.conf.in, whose
    `depends:` is empty.

Measured before this table was committed (specs/1653-boot-library-edges/
measurements/): the bindist's dependencies equal nixpkgs' installed package
database in 5 of 5 releases checked, and every release's library set and
versions equal milestone 966's.

The probe fails when a release cannot be read or yields no library, so a
release cannot silently go missing. Stdlib only.
"""

import concurrent.futures
import datetime
import lzma
import os
import re
import sys
import tarfile
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
VERSIONS_TSV = os.path.join(
    HERE, "..", "waybill-cli", "src", "scan_fs", "package_db", "nix",
    "haskell_packages", "ghc_boot_libraries.tsv",
)
URL = "https://downloads.haskell.org/~ghc/{v}/ghc-{v}-x86_64-deb10-linux.tar.xz"
WORKERS = 6


def field(text, name):
    """A ghc-pkg field's whitespace-separated words, continuation lines included."""
    m = re.search(r"^" + re.escape(name) + r":[ \t]*(.*(?:\n[ \t]+.*)*)", text, re.M)
    return m.group(1).split() if m else []


def unit_name(uid):
    """`ghc-prim-0.12.0-3d5d` -> `ghc-prim`; `rts` -> `rts`."""
    out = []
    for part in uid.split("-"):
        if re.fullmatch(r"\d+(\.\d+)*", part):
            break
        out.append(part)
    return "-".join(out) if out else uid


def releases():
    seen = []
    with open(VERSIONS_TSV) as f:
        for line in f:
            if line.startswith("#") or not line[:1].isdigit():
                continue
            rel = line.split("\t", 1)[0]
            if rel not in seen:
                seen.append(rel)
    return seen


def read_release(v):
    conf, mk, rts_in = {}, {}, None
    with urllib.request.urlopen(URL.format(v=v)) as r, lzma.open(r) as x, \
            tarfile.open(fileobj=x, mode="r|") as t:
        for m in t:
            if not m.isfile():
                continue
            n = m.name
            if "/package.conf.d/" in n and n.endswith(".conf"):
                text = t.extractfile(m).read().decode()
                name = (field(text, "name") or ["?"])[0]
                conf[name] = (
                    (field(text, "version") or [""])[0],
                    sorted({unit_name(d) for d in field(text, "depends")}),
                )
            elif (n.endswith("/dist-install/package-data.mk") and "/libraries/" in n) \
                    or n.endswith("/compiler/stage2/package-data.mk"):
                text = t.extractfile(m).read().decode()
                get = lambda k: (re.search(r"_" + k + r" = (.*)", text) or [None, ""])[1].split()
                lib = n.split("/libraries/")[1].split("/dist-install/")[0].split("/")[-1] \
                    if "/libraries/" in n else "ghc"
                pkg = (re.search(r"_PACKAGE = (\S+)", text) or [None, lib])[1]
                mk[pkg] = ((get("VERSION") or [""])[0], sorted(set(get("DEP_NAMES"))))
            elif n.endswith("/rts/package.conf.in"):
                rts_in = t.extractfile(m).read().decode()
    if conf:
        return "package.conf.d", conf
    if rts_in is not None and "rts" not in mk:
        deps = [d for d in field(rts_in, "depends") if re.fullmatch(r"[a-z][a-z0-9-]*", d)]
        mk["rts"] = ("", sorted(set(deps)))
    return "package-data.mk", mk


def main():
    rels = releases()
    with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as pool:
        results = dict(zip(rels, pool.map(read_release, rels)))
    rows = []
    for rel in rels:
        layout, libs = results[rel]
        if not libs:
            sys.exit(f"{rel}: no libraries read from {URL.format(v=rel)}")
        for lib in sorted(libs):
            version, deps = libs[lib]
            rows.append((rel, lib, version, " ".join(deps) or "-", layout))
    out = sys.stdout
    out.write(f"# observed: {datetime.date.today().isoformat()}\n")
    out.write(f"# source: {URL.format(v='<v>')}\n")
    out.write("# layouts: package.conf.d (9.4 and later, field depends), "
              "package-data.mk (9.0 and 9.2, variable _DEP_NAMES)\n")
    out.write(f"# releases: {len(rels)}; rows: {len(rows)}\n")
    out.write("# regenerate: python3 -I -B scripts/ghc-boot-dependencies.py > "
              "waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_dependencies.tsv\n")
    out.write("ghc\tlibrary\tversion\tdepends\tlayout\n")
    for r in rows:
        out.write("\t".join(r) + "\n")


if __name__ == "__main__":
    main()
