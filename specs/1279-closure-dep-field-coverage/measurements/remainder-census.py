#!/usr/bin/env python3
"""What is left unread after #1346's widening, and who carries the marker.

`attribute-census.py` answers "which attributes go unread" against the SEVEN
the classifier consulted before this milestone. This answers the two questions
that only exist once four of them are consulted:

  1. the C204 disclosure's before/after -- same measurement, partitioned by
     the new consulted set rather than the old one; and
  2. the C205 population: which closure members an unconsulted attribute
     names, and how many of them become components.

(2) is NOT derivable from (1). A member named by an unread attribute is only
observable if it is emitted at all, and an `unreferenced` member that applies
no patch is not (C182 / m1035 FR-004a). Reporting the declaration count as
though it were the marker count is the mistake this probe exists to prevent.

Input: the output of the same invocation the reader uses --
  nix derivation show -r --option allow-import-from-derivation false \
      <repo>#packages.<system>.default > closure.json

Usage: python3 -I remainder-census.py closure.json

Run 2026-11-05 against the `nix-closure-moat` corpus target at its pin,
nixpkgs a799d3e3, **x86_64-linux** (the closure the corpus goldens pin),
format version 4.
"""
import collections
import json
import sys

BEFORE_ARTIFACT = ['buildInputs', 'propagatedBuildInputs', 'depsHostHost']
BEFORE_TOOLING = ['nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
                  'nativeCheckInputs']
ADDED_ARTIFACT = ['depsTargetTarget', 'depsTargetTargetPropagated']
ADDED_TOOLING = ['depsBuildTarget', 'propagatedNativeBuildInputs']

BEFORE = set(BEFORE_ARTIFACT) | set(BEFORE_TOOLING)
AFTER = BEFORE | set(ADDED_ARTIFACT) | set(ADDED_TOOLING)
# `patches` is consulted by the patch collector, not by classification, and
# is what makes an otherwise-unreferenced member emit at all.
PATCHES = 'patches'


def is_dependency_attribute(name):
    """nixpkgs' naming convention, mirroring derivation.rs."""
    return (name.endswith('Inputs')
            or name.endswith('InputsPropagated')
            or name.startswith('deps'))


def paths(drv, field):
    env = drv.get('env') or {}
    if field in env:
        return [p for p in str(env[field]).split() if p.startswith('/nix/store/')]
    sa = drv.get('structuredAttrs') or {}
    v = sa.get(field)
    if isinstance(v, list):
        return [p for p in v if isinstance(p, str) and p.startswith('/nix/store/')]
    if isinstance(v, str):
        return [p for p in v.split() if p.startswith('/nix/store/')]
    return []


def attributes(drv):
    seen = []
    for k in list((drv.get('env') or {}).keys()) + list((drv.get('structuredAttrs') or {}).keys()):
        if k not in seen and is_dependency_attribute(k):
            seen.append(k)
    return seen


def basename(p):
    return p.rsplit('/', 1)[-1]


def attr(drv, name):
    env = drv.get('env') or {}
    if name in env:
        return env[name]
    return (drv.get('structuredAttrs') or {}).get(name)


def main(path):
    closure = json.load(open(path))
    drvs = closure['derivations']
    index = {}
    for key, drv in drvs.items():
        for out in (drv.get('outputs') or {}).values():
            if out.get('path'):
                index[basename(out['path'])] = key

    def disclosure(consulted):
        fields, decls, affected = collections.Counter(), 0, 0
        for drv in drvs.values():
            hit = False
            for field in attributes(drv):
                if field in consulted:
                    continue
                ps = paths(drv, field)
                if not ps:
                    continue
                fields[field] += len(ps)
                decls += len(ps)
                hit = True
            affected += 1 if hit else 0
        return fields, decls, affected

    print(f'{path}: {len(drvs)} derivations, format version {closure.get("version")}')
    for label, consulted in (('before', BEFORE), ('after', AFTER)):
        fields, decls, affected = disclosure(consulted)
        print(f'\nC204 {label}: {len(fields)} fields, {decls} declarations, '
              f'{affected} derivations affected')
        for f, n in sorted(fields.items(), key=lambda kv: (-kv[1], kv[0])):
            print(f'    {f:<34} {n:>5}')

    # C205: members an unconsulted attribute names, and which are emitted.
    named = collections.defaultdict(set)
    for drv in drvs.values():
        for field in attributes(drv):
            if field in AFTER:
                continue
            for p in paths(drv, field):
                key = index.get(basename(p))
                if key:
                    named[key].add(field)

    # Role, with the widened set, so "emitted" is the post-fix answer.
    artifact, tooling = set(), set()
    for drv in drvs.values():
        for fields, sink in ((set(BEFORE_ARTIFACT) | set(ADDED_ARTIFACT), artifact),
                             (set(BEFORE_TOOLING) | set(ADDED_TOOLING), tooling)):
            for field in fields:
                for p in paths(drv, field):
                    key = index.get(basename(p))
                    if key:
                        sink.add(key)

    emitted_members, components = [], collections.defaultdict(set)
    for key, fields in named.items():
        drv = drvs[key]
        has_role = key in artifact or key in tooling
        if not (has_role or paths(drv, PATCHES)):
            continue
        pname, version = attr(drv, 'pname'), attr(drv, 'version')
        if not pname:
            continue
        emitted_members.append((key, sorted(fields)))
        components[(pname, version or '')] |= fields

    print(f'\nC205: {len(named)} closure members are named by an unconsulted '
          f'attribute')
    print(f'      {len(emitted_members)} of them are emitted as components '
          f'(the rest are `unreferenced` and apply no patch, so they are not '
          f'emitted at all)')
    print(f'      collapsing build variants: {len(components)} components '
          f'carry the marker')
    for (pname, version), fields in sorted(components.items()):
        print(f'    {pname}-{version:<12} {sorted(fields)}')


if __name__ == '__main__':
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
