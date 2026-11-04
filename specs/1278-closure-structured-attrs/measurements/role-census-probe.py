#!/usr/bin/env python3
"""What does reading `structuredAttrs` do to the closure role census?

Re-implements waybill's classifier (closure/classify.rs::classify) twice:
once reading only the flat `env` map, as the shipped reader does, and once
also reading the same seven fields out of `structuredAttrs`.

Usage: python3 -I role-census-probe.py closure.json [...]
"""
import json, sys, collections

ARTIFACT = ['buildInputs', 'propagatedBuildInputs', 'depsHostHost']
TOOLING = ['nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
           'nativeCheckInputs']


def basename(p):
    return p.rsplit('/', 1)[-1]


def paths(drv, field, with_sa):
    """Mirror RawDerivation::paths_in, optionally widened to structuredAttrs."""
    env = drv.get('env') or {}
    if field in env:
        return env[field].split()
    if with_sa:
        v = (drv.get('structuredAttrs') or {}).get(field)
        if isinstance(v, list):
            return [x for x in v if isinstance(x, str)]
        if isinstance(v, str):
            return v.split()
    return []


def census(drvs, with_sa):
    index = {}
    for key, drv in drvs.items():
        for out in (drv.get('outputs') or {}).values():
            p = out.get('path')
            if p:
                index[basename(p)] = key
    artifact, tooling = set(), set()
    for drv in drvs.values():
        for fields, sink in ((ARTIFACT, artifact), (TOOLING, tooling)):
            for f in fields:
                for p in paths(drv, f, with_sa):
                    k = index.get(basename(p))
                    if k:
                        sink.add(k)
    roles = {}
    for key in drvs:
        a, t = key in artifact, key in tooling
        roles[key] = ('both' if a and t else 'artifact-input' if a
                      else 'build-tooling' if t else 'unreferenced')
    return roles


for path in sys.argv[1:]:
    drvs = json.load(open(path))['derivations']
    before, after = census(drvs, False), census(drvs, True)
    cb, ca = collections.Counter(before.values()), collections.Counter(after.values())
    print(f"\n{path}: {len(drvs)} derivations")
    print(f"  {'role':16s} {'env only':>9s} {'+structuredAttrs':>17s} {'delta':>7s}")
    for r in ('artifact-input', 'build-tooling', 'both', 'unreferenced'):
        print(f"  {r:16s} {cb[r]:9d} {ca[r]:17d} {ca[r]-cb[r]:+7d}")
    moved = [k for k in drvs if before[k] != after[k]]
    print(f"  derivations whose role changes: {len(moved)}")
    trans = collections.Counter((before[k], after[k]) for k in moved)
    for (b, a), c in trans.most_common():
        print(f"    {b} -> {a}: {c}")
    # Does any role change for a derivation that CARRIES AN IDENTITY, i.e.
    # one that becomes a component? A role change on a nameless derivation is
    # invisible in the output.
    def pname(k):
        d = drvs[k]
        return (d.get('env') or {}).get('pname') \
            or (d.get('structuredAttrs') or {}).get('pname')
    named = [k for k in moved if pname(k)]
    print(f"  ...of which declare a pname (so are emitted as components): {len(named)}")
    ids = {(pname(k), ((drvs[k].get('env') or {}).get('version')
           or (drvs[k].get('structuredAttrs') or {}).get('version'))) for k in named}
    print(f"  ...covering {len(ids)} distinct (pname, version) identities")
