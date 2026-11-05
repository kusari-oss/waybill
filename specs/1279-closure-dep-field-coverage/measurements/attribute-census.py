#!/usr/bin/env python3
"""Every nixpkgs dependency attribute a closure declares, against the seven
the classifier consults.

The baseline for #1346. The issue said "7 of ~20"; this says which, how many
are actually populated, and how unevenly.

Input: the output of the SAME invocation the reader uses, so a finding cannot
be an artifact of a differently-shaped query --
  nix derivation show -r --option allow-import-from-derivation false \
      <repo>#packages.<system>.default > closure.json
(see waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs::closure_argv)

Usage: python3 -I attribute-census.py closure.json [...]

Run 2026-11-05 against the `nix-closure-moat` corpus target at its pin,
nixpkgs a799d3e3, aarch64-darwin, Determinate Nix 3.20.0 (format version 4).
"""
import collections
import json
import sys

# The seven classify.rs consults, kept here as DATA rather than imported, so
# this probe measures the shipped set rather than agreeing with itself.
ARTIFACT = ['buildInputs', 'propagatedBuildInputs', 'depsHostHost']
TOOLING = ['nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
           'nativeCheckInputs']
CONSULTED = set(ARTIFACT) | set(TOOLING)


def is_dependency_attribute(name: str) -> bool:
    """nixpkgs' naming convention, mirroring derivation.rs.

    A convention rather than a list, so an attribute nixpkgs adds later is
    still counted. A pure shape test -- "does the value hold a store path" --
    was tried in #1348 and reported `PATH`, `SHELL` and `mkdir`: nearly every
    attribute mentions a store path and almost none declares a dependency.
    """
    return (name.endswith('Inputs')
            or name.endswith('InputsPropagated')
            or (name.startswith('deps') and len(name) > 4))


def values(drv, key):
    """Resolve from either attribute carrier (#1348)."""
    env = drv.get('env') or {}
    if key in env:
        return env[key].split()
    sa = drv.get('structuredAttrs') or {}
    v = sa.get(key) if isinstance(sa, dict) else None
    if isinstance(v, list):
        return [x for x in v if isinstance(x, str)]
    if isinstance(v, str):
        return v.split()
    return []


for path in sys.argv[1:]:
    drvs = json.load(open(path))['derivations']
    declared = collections.Counter()
    paths = collections.Counter()
    for drv in drvs.values():
        env = drv.get('env') or {}
        sa = drv.get('structuredAttrs') or {}
        keys = set(env) | (set(sa) if isinstance(sa, dict) else set())
        for key in keys:
            if not is_dependency_attribute(key):
                continue
            declared[key] += 1
            paths[key] += sum(1 for v in values(drv, key)
                              if v.startswith('/nix/store/'))

    print(f"\n{path}: {len(drvs)} derivations")
    print(f"{'attribute':34}{'declared on':>12}{'store paths':>12}  consulted")
    for key in sorted(declared, key=lambda k: (-paths[k], k)):
        mark = 'yes' if key in CONSULTED else 'NO'
        print(f"{key:34}{declared[key]:>12}{paths[key]:>12}  {mark}")

    gap = {k: paths[k] for k in declared
           if k not in CONSULTED and paths[k] > 0}
    print(f"\n  populated and UNCONSULTED: {len(gap)} attributes, "
          f"{sum(gap.values())} store-path declarations")
    for k, n in sorted(gap.items(), key=lambda kv: -kv[1]):
        print(f"    {k:32}{n:>6}")
    # The unevenness is the design-relevant part: a flat total would hide
    # that two stdenv-internal attributes carry most of it.
    if gap:
        top = max(gap, key=gap.get)
        print(f"  largest single attribute: {top} "
              f"({gap[top]} of {sum(gap.values())})")

    empty = sorted(k for k in declared if paths[k] == 0)
    print(f"  declared but never populated here: {', '.join(empty) or 'none'}")
