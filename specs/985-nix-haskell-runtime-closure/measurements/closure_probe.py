#!/usr/bin/env python3
"""Measure the transitive-closure multiplier for #962.

Parses `hackage-packages.nix` the way production does since #970 -- keyed on
the ATTRIBUTE name, not `pname`, because 371 attributes share a pname and
keying on pname silently returns a version the build never uses.

Reports closure size per dependency class so the scope decision in #962's
open question 1 rests on numbers rather than intuition.

Usage: closure_probe.py <hackage-packages.nix> <ghc-config.nix> <names-file>
"""
import re
import sys
from collections import deque

# Matches production `attribute_re` in package_set.rs.
HEAD = re.compile(
    r'(?m)^  (?:"([^"]+)"|([A-Za-z0-9][A-Za-z0-9_.\'-]*))\s*=\s*callPackage'
)
VERSION = re.compile(r'version\s*=\s*"([^"]+)"\s*;')
DEPFIELD = {
    "library": re.compile(r'libraryHaskellDepends\s*=\s*\[([^\]]*)\]', re.S),
    "executable": re.compile(r'executableHaskellDepends\s*=\s*\[([^\]]*)\]', re.S),
    "test": re.compile(r'testHaskellDepends\s*=\s*\[([^\]]*)\]', re.S),
    "benchmark": re.compile(r'benchmarkHaskellDepends\s*=\s*\[([^\]]*)\]', re.S),
}
NULLED = re.compile(r"(?:^|[\s{;])([A-Za-z][A-Za-z0-9_'-]*)[ \t]*=[ \t]*null[ \t]*;")


def parse(text):
    heads = list(HEAD.finditer(text))
    out = {}
    for i, m in enumerate(heads):
        name = m.group(1) or m.group(2)
        body = text[m.end(): heads[i + 1].start() if i + 1 < len(heads) else len(text)]
        v = VERSION.search(body)
        deps = {}
        for cls, rx in DEPFIELD.items():
            hit = rx.search(body)
            deps[cls] = hit.group(1).split() if hit else []
        out[name] = {"version": v.group(1) if v else None, "deps": deps}
    return out


def closure(index, roots, boot, classes):
    """BFS over the chosen dependency classes.

    Cycles: `seen` is checked before enqueue, so a mutually recursive test
    dependency terminates (#962 open question 3). Boot libraries are not
    traversed -- they ship with the compiler and their nixpkgs entry is not
    what the build uses (#962 open question 2).
    """
    seen, q, missing = set(), deque(), set()
    for r in roots:
        if r not in seen:
            seen.add(r); q.append(r)
    while q:
        cur = q.popleft()
        if cur in boot:
            continue
        ent = index.get(cur)
        if ent is None:
            missing.add(cur); continue
        for cls in classes:
            for d in ent["deps"][cls]:
                if d not in seen:
                    seen.add(d); q.append(d)
    return seen, missing


def main():
    pkgs_path, cfg_path, names_path = sys.argv[1:4]
    text = open(pkgs_path, encoding="utf-8", errors="replace").read()
    index = parse(text)
    boot = set(NULLED.findall(open(cfg_path, encoding="utf-8", errors="replace").read()))
    roots = [n for n in open(names_path).read().split() if n]

    present = [r for r in roots if r in index]
    print(f"package set   : {len(index)} attributes")
    print(f"boot (nulled) : {len(boot)}")
    print(f"declared      : {len(roots)}  ({len(present)} present in the set)")
    print()
    combos = [
        ("library only", ["library"]),
        ("library+executable", ["library", "executable"]),
        ("library+exec+test", ["library", "executable", "test"]),
        ("all four", ["library", "executable", "test", "benchmark"]),
    ]
    base = len(roots)
    for label, classes in combos:
        seen, missing = closure(index, present, boot, classes)
        resolvable = {n for n in seen if n in index and n not in boot}
        print(f"{label:22} closure={len(seen):5}  resolvable={len(resolvable):5}  "
              f"x{len(resolvable)/max(base,1):.1f}  unknown-names={len(missing)}")


if __name__ == "__main__":
    sys.exit(main())
