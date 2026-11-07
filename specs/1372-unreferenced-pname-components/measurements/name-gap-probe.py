#!/usr/bin/env python3 -I
"""The residual sbomnix name gap, with nix's language prefixes normalised.

Milestone 1276 established that the raw sbomnix-only name count is a naming
artifact: nix calls a Python derivation `python3.13-urllib3` while waybill
names the component by its `pname`, `urllib3`. Comparing raw name sets counts
every such package as missing. That milestone normalised by hand and reported
32; this file makes the rule reproducible so the next re-run measures the same
thing rather than a differently-normalised one.

  python3 -I name-gap-probe.py <sbomnix.cdx.json> <waybill.cdx.json>
"""
import json
import re
import sys

# nix's language-ecosystem derivation-name prefixes. Each is the *derivation*
# naming convention, not a package-name component, so stripping it recovers the
# pname waybill emits.
PREFIXES = [
    re.compile(r"^python3\.\d+-"),
    re.compile(r"^python2\.\d+-"),
    re.compile(r"^perl5\.\d+\.\d+-"),
    re.compile(r"^ghc\d+(\.\d+)*-"),
    re.compile(r"^ruby\d+\.\d+(\.\d+)?-"),
    re.compile(r"^lua\d+\.\d+-"),
    re.compile(r"^php\d+\.\d+-"),
]


def normalise(name: str) -> str:
    for p in PREFIXES:
        stripped = p.sub("", name, count=1)
        if stripped != name:
            return stripped
    return name


def names(path: str) -> set:
    doc = json.load(open(path))
    return {c.get("name") for c in doc.get("components", []) if c.get("name")}


def main() -> None:
    a, b = names(sys.argv[1]), names(sys.argv[2])
    raw_gap = a - b
    norm_gap = {n for n in raw_gap if normalise(n) not in b}
    print(f"sbomnix names          {len(a)}")
    print(f"waybill names          {len(b)}")
    print(f"shared, raw            {len(a & b)}")
    print(f"sbomnix-only, raw      {len(raw_gap)}")
    print(f"sbomnix-only, norm     {len(norm_gap)}")
    print(f"waybill-only           {len(b - a)}")
    print("\nthe residual gap:")
    for n in sorted(norm_gap):
        print(f"  {n}")


if __name__ == "__main__":
    main()
