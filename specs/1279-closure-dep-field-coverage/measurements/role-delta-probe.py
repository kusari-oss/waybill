#!/usr/bin/env python3
"""What do the four newly-consulted attributes actually do to the roles?

The spec declined to predict this (SC-004, SC-011): it is this milestone's
output, not its input. This probe measures it before the implementation, so
the implementation can be checked against a number rather than against a
hope.

Usage: python3 -I role-delta-probe.py closure.json [...]

Run 2026-11-05 against the `nix-closure-moat` corpus target at its pin,
nixpkgs a799d3e3, Determinate Nix 3.20.0 (derivation-show format 4).
"""
import collections
import json
import sys

# Today, per classify.rs.
ARTIFACT = ['buildInputs', 'propagatedBuildInputs', 'depsHostHost']
TOOLING = ['nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
           'nativeCheckInputs']

# This milestone. The clarifications put the target triples in ARTIFACT
# (FR-010: `artifact-input` asserts no scope, `build-tooling` would assert
# `excluded`, and a cross-compiler ships its target libc) and
# depsBuildTarget in TOOLING (FR-011: it runs on the build machine).
NEW_ARTIFACT = ['depsTargetTarget', 'depsTargetTargetPropagated']
NEW_TOOLING = ['depsBuildTarget', 'propagatedNativeBuildInputs']

# Deliberately NOT consulted (FR-012) -- 323 of the 353 unread declarations.
UNCLASSIFIED = ['defaultNativeBuildInputs', 'defaultBuildInputs']


def bn(p):
    return p.rsplit('/', 1)[-1]


def values(drv, key):
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


def census(drvs, artifact_fields, tooling_fields):
    index = {}
    for key, drv in drvs.items():
        for out in (drv.get('outputs') or {}).values():
            if out.get('path'):
                index[bn(out['path'])] = key
    art, tool = set(), set()
    for drv in drvs.values():
        for fields, sink in ((artifact_fields, art), (tooling_fields, tool)):
            for f in fields:
                for p in values(drv, f):
                    k = index.get(bn(p))
                    if k:
                        sink.add(k)
    roles = {}
    for key in drvs:
        a, t = key in art, key in tool
        roles[key] = ('both' if a and t else 'artifact-input' if a
                      else 'build-tooling' if t else 'unreferenced')
    return roles, index


def pname(drv):
    e = drv.get('env') or {}
    s = drv.get('structuredAttrs') or {}
    v = e.get('pname') or (s.get('pname') if isinstance(s, dict) else None)
    return v if isinstance(v, str) else None


for path in sys.argv[1:]:
    drvs = json.load(open(path))['derivations']
    before, index = census(drvs, ARTIFACT, TOOLING)
    after, _ = census(drvs, ARTIFACT + NEW_ARTIFACT, TOOLING + NEW_TOOLING)

    cb = collections.Counter(before.values())
    ca = collections.Counter(after.values())
    print(f"\n{path}: {len(drvs)} derivations")
    print(f"  {'role':16}{'before':>8}{'after':>8}{'delta':>8}")
    for r in ('artifact-input', 'build-tooling', 'both', 'unreferenced'):
        print(f"  {r:16}{cb[r]:>8}{ca[r]:>8}{ca[r]-cb[r]:>+8}")

    moved = [k for k in drvs if before[k] != after[k]]
    print(f"\n  derivations whose role changes: {len(moved)}")
    for (a, b), n in collections.Counter(
            (before[k], after[k]) for k in moved).most_common():
        print(f"    {a} -> {b}: {n}")
    named = [k for k in moved if pname(drvs[k])]
    print(f"  ...of which declare a pname (so are emitted): {len(named)}")
    if named:
        ids = sorted({f"{pname(drvs[k])}" for k in named})
        print(f"  identities: {', '.join(ids)}")

    # FR-012 control: the stdenv pair must contribute ZERO movement (SC-011).
    with_stdenv, _ = census(drvs, ARTIFACT + NEW_ARTIFACT + ['defaultBuildInputs'],
                            TOOLING + NEW_TOOLING + ['defaultNativeBuildInputs'])
    stdenv_moved = [k for k in drvs if after[k] != with_stdenv[k]]
    print(f"\n  CONTROL -- had the stdenv pair been consulted too, a further "
          f"{len(stdenv_moved)} derivations would move.")
    print(f"  SC-011 asserts this milestone does NOT move them; the figure is "
          f"the size of what FR-012 declines.")

    # FR-002: members named ONLY by an unclassified attribute, which must
    # become distinguishable from `unreferenced`.
    only_unclassified = set()
    for drv in drvs.values():
        for f in UNCLASSIFIED:
            for p in values(drv, f):
                k = index.get(bn(p))
                if k and after[k] == 'unreferenced':
                    only_unclassified.add(k)
    print(f"\n  FR-002 population -- `unreferenced` members named only by an "
          f"unclassified attribute: {len(only_unclassified)}")
    named_only = [k for k in only_unclassified if pname(drvs[k])]
    print(f"    of which emitted (declare a pname): {len(named_only)}")
