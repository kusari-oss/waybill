#!/usr/bin/env python3
"""What `inputs.drvs` contains, against what waybill reads instead.

The committed comparison in this directory explained waybill's edge deficit
as a difference of philosophy: "sbomnix treats a store reference as a
dependency, waybill emits only what the package set declares." That is wrong,
and this probe is what showed it. `nix derivation show` gives every derivation
an `inputs.drvs` map -- the derivations whose outputs the build consumed --
and the graph it describes has **exactly** as many edges as sbomnix reports.
sbomnix is reading the build DAG. waybill parses `buildInputs`-style
attributes and never looks at `inputs.drvs` at all.

Input: the output of the same invocation the reader uses --
  nix derivation show -r --option allow-import-from-derivation false \
      <repo>#packages.<system>.default > closure.json

Usage: python3 -I input-graph-probe.py closure.json

Run 2026-11-05 against the `nix-closure-moat` corpus target at its pin,
nixpkgs a799d3e3, aarch64-darwin, 1,275 derivations, format version 4.
"""
import collections
import json
import sys

# What classification reads today, post-milestone-1279. Kept as DATA rather
# than imported, so the probe measures the shipped set rather than agreeing
# with itself.
DECLARED = [
    'buildInputs', 'propagatedBuildInputs', 'depsHostHost',
    'depsTargetTarget', 'depsTargetTargetPropagated',
    'nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
    'nativeCheckInputs', 'depsBuildTarget', 'propagatedNativeBuildInputs',
]


def env(drv):
    """Both attribute carriers merged (milestone 1278)."""
    e = dict(drv.get('env') or {})
    e.update(drv.get('structuredAttrs') or {})
    return e


def paths(drv, field):
    v = env(drv).get(field)
    if isinstance(v, list):
        return [p for p in v if isinstance(p, str) and p.startswith('/nix/store/')]
    if isinstance(v, str):
        return [p for p in v.split() if p.startswith('/nix/store/')]
    return []


def basename(p):
    return p.rsplit('/', 1)[-1]


def main(path):
    closure = json.load(open(path))
    drvs = closure['derivations']

    # --- the graph waybill reads today: declared attributes, basename-joined
    index = {}
    for key, drv in drvs.items():
        for out in (drv.get('outputs') or {}).values():
            if out.get('path'):
                index[basename(out['path'])] = key
    declared_edges = 0
    explained = set()
    for drv in drvs.values():
        for field in DECLARED:
            for p in paths(drv, field):
                owner = index.get(basename(p))
                if owner:
                    declared_edges += 1
                    explained.add(owner)

    # --- the graph nix actually records
    adj = collections.defaultdict(set)
    rdeps = collections.defaultdict(set)
    input_edges = 0
    for key, drv in drvs.items():
        for dep in ((drv.get('inputs') or {}).get('drvs') or {}):
            if dep in drvs:
                adj[key].add(dep)
                rdeps[dep].add(key)
                input_edges += 1

    roots = [k for k in drvs if not rdeps.get(k)]
    seen, stack = set(), list(roots)
    while stack:
        k = stack.pop()
        if k in seen:
            continue
        seen.add(k)
        stack.extend(adj[k] - seen)

    print(f"{path}: {len(drvs)} derivations, format version {closure.get('version')}\n")
    print(f"  declared-attribute edges (what waybill reads)   {declared_edges}")
    print(f"  inputs.drvs edges (what nix records)            {input_edges}")
    print(f"  derivations explained, declared attributes      {len(explained)} of {len(drvs)}")
    print(f"  derivations explained, inputs.drvs              {len(drvs) - len(roots)} of {len(drvs)}")
    print(f"  roots under inputs.drvs                         {len(roots)}"
          f"  {[drvs[r].get('name') for r in roots][:4]}")
    print(f"  reachable from those roots                      {len(seen)} of {len(drvs)}")
    print(f"  -> `unreferenced` would fall from {len(drvs) - len(explained)} to {len(roots)}")

    # --- the population the declared view cannot name, and what inputs.drvs
    #     says about it
    no_pname = [k for k in drvs if not env(drvs[k]).get('pname')]
    named_consumers = collections.Counter()
    for k in no_pname:
        named = {(env(drvs[c]).get('pname'), env(drvs[c]).get('version'))
                 for c in rdeps.get(k, ()) if env(drvs[c]).get('pname')}
        named_consumers['one' if len(named) == 1 else
                        ('several' if len(named) > 1 else 'none')] += 1
    print(f"\n  derivations with no `pname` in either carrier   {len(no_pname)}")
    print(f"    with a `name`                                 "
          f"{sum(1 for k in no_pname if drvs[k].get('name'))}")
    for label in ('one', 'several', 'none'):
        print(f"    with {label:>7} named consumer(s)                "
              f"{named_consumers[label]}")

    ident = collections.Counter()
    for k in no_pname:
        e = env(drvs[k])
        if e.get('owner') and e.get('repo') and e.get('rev'):
            ident['forge coords -> pkg:github/<owner>/<repo>@<rev>'] += 1
        elif (e.get('urls') or e.get('url')) and e.get('outputHash'):
            ident['url + hash -> pkg:generic?download_url=&checksum='] += 1
        elif e.get('outputHash'):
            ident['hash only -> pkg:generic?checksum='] += 1
        else:
            ident['name and owning derivation only'] += 1
    print("\n  identity available for them:")
    for label, n in ident.most_common():
        print(f"    {n:>5}  {label}")


if __name__ == '__main__':
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
