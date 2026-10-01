#!/usr/bin/env python3
"""Classify a Nix build or runtime closure by whether a PURL can be formed.

Issue #1034 asks how deep waybill should follow a Nix closure. Its tension #2
assumes admitting the closure would add "~1,300 components largely without
PURLs". This measures that instead of assuming it, for both graphs.

Usage:
    classify-closure.py build   <derivation-show-json>
    classify-closure.py runtime <path-info-r-output>

Two schema traps, both of which produced confidently wrong numbers first time:

1. `nix derivation show` schema version 4 puts `name` at the TOP level of each
   derivation, and for derivations using `structuredAttrs` the `pname` and
   `version` live in `structuredAttrs`, NOT in `env` (which holds only the
   output names). Reading `env` alone reports ~49% of derivations as nameless.
   Validate any change here against a known figure before trusting it.

2. Nix multi-output store paths end in the OUTPUT name, not the version:
   `ncurses-6.6-dev`, `llvm-21.1.8-lib`, `shake-0.19.9-data`. A version regex
   anchored at end-of-string scores these as version-less and understates
   runtime-closure identity by ~48 points (50.7% vs the true 98.5%).
"""
import collections
import json
import re
import sys

# Nix multi-output suffixes. A store path ending in one of these has its
# version BEFORE the suffix; see trap 2 above.
OUTPUT_SUFFIXES = (
    "dev", "lib", "man", "doc", "devdoc", "bin", "out", "info", "static",
    "debug", "data", "libtool", "xcrun", "include",
)

VERSION = re.compile(r"^(?P<name>.+?)-(?P<version>\d[\w.+]*(?:-unstable-[\d-]+)?)$")


def strip_output_suffix(name: str) -> str:
    parts = name.split("-")
    while len(parts) > 1 and parts[-1] in OUTPUT_SUFFIXES:
        parts.pop()
    return "-".join(parts)


def classify_build(path: str) -> dict:
    """pname/version from a `nix derivation show -r --json` dump."""
    doc = json.load(open(path))
    drvs = doc["derivations"] if "derivations" in doc else doc
    rows = []
    for drv in drvs.values():
        structured = drv.get("structuredAttrs") or {}
        env = drv.get("env") or {}
        rows.append(
            {
                "name": drv.get("name") or "?",
                "pname": structured.get("pname") or env.get("pname"),
                "version": structured.get("version") or env.get("version"),
            }
        )
    return {"kind": "build", "unit": "derivations", "rows": rows}


def classify_runtime(path: str) -> dict:
    """name/version parsed out of store paths from `nix path-info -r`."""
    rows = []
    for line in open(path):
        line = line.strip()
        if not line:
            continue
        base = line.rsplit("/", 1)[-1]
        # strip the 32-char store hash
        name = base.split("-", 1)[1] if "-" in base else base
        match = VERSION.match(strip_output_suffix(name))
        rows.append(
            {
                "name": name,
                "pname": match.group("name") if match else None,
                "version": match.group("version") if match else None,
            }
        )
    return {"kind": "runtime", "unit": "store paths", "rows": rows}


def report(result: dict) -> None:
    rows = result["rows"]
    total = len(rows)
    identified = [r for r in rows if r["pname"] and r["version"]]
    opaque = [r for r in rows if not (r["pname"] and r["version"])]

    print(f"=== {result['kind']} closure: {total:,} {result['unit']} ===")
    print(f"  distinct names            : {len({r['name'] for r in rows}):,}")
    pct = (len(identified) / total * 100) if total else 0
    print(f"  name + version (PURL-able): {len(identified):,}  {pct:.1f}%")
    print(f"  opaque                    : {len(opaque):,}  {100 - pct:.1f}%")

    if opaque:
        print("\n  opaque, most common:")
        for name, count in collections.Counter(r["name"] for r in opaque).most_common(12):
            print(f"    {count:5,}  {name[:58]}")

    print("\n  build machinery present:")
    for pattern in ("bootstrap", "expand-response-params", "clang-wrapper",
                    "cctools", "apple-sdk", "die-hook", "stdenv", "source"):
        hits = sum(1 for r in rows if pattern in r["name"])
        print(f"    {pattern:24} {hits:5,}")


def main() -> int:
    if len(sys.argv) != 3 or sys.argv[1] not in ("build", "runtime"):
        print(__doc__)
        return 2
    kind, path = sys.argv[1], sys.argv[2]
    report(classify_build(path) if kind == "build" else classify_runtime(path))
    return 0


if __name__ == "__main__":
    sys.exit(main())
