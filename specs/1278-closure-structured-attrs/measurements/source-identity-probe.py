#!/usr/bin/env python3
"""Can a nix closure's `source`-named derivations be given a real identity?

Question: emitting 107 components all called `source` is useless to a
vulnerability feed. Is there anything in the derivation to key off instead?

Input: the output of the SAME invocation waybill uses --
  nix derivation show -r --option allow-import-from-derivation false \
      <repo>#packages.<system>.default > closure.json
(see waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs::closure_argv)

Usage: python3 -I source-identity-probe.py closure.json [closure.json ...]

Measured 2026-11-04 against MercuryTechnologies/moat @ d0690555 (the
`nix-closure-moat` corpus pin), nixpkgs a799d3e3, Determinate Nix 3.20.0
(derivation-show format version 4).
"""
import json, sys, re, collections
from urllib.parse import unquote

FIELDS = ['buildInputs', 'propagatedBuildInputs', 'depsHostHost',
          'nativeBuildInputs', 'depsBuildBuild', 'depsBuildHost',
          'nativeCheckInputs']


def attrs(drv):
    """Everything waybill's RawDerivation reads, plus what it does not.

    Format version 4 puts a derivation's attributes in `structuredAttrs`
    when nixpkgs set `__structuredAttrs = true`, leaving `env` with only
    `out`. waybill reads `env` only, so those are invisible to it today.
    """
    return (drv.get('structuredAttrs') or {}), (drv.get('env') or {})


def identity(drv):
    """The ladder: forge coordinates, then URL, then the content hash."""
    sa, env = attrs(drv)
    get = lambda k: sa.get(k) or env.get(k)
    owner, repo = get('owner'), get('repo')
    ref = get('tag') or get('rev')
    if owner and repo:
        if isinstance(ref, str):
            ref = re.sub(r'^refs/tags/', '', ref)
        return 1, f"pkg:github/{owner.lower()}/{repo.lower()}@{ref}"
    urls = sa.get('urls') or env.get('urls')
    u = (urls[0] if isinstance(urls, list) and urls else urls) or get('url')
    if u:
        m = re.search(r'/projects/([^/]+)/repository/archive[^?]*\?sha=(.+)$', u)
        if m:
            proj, sha = unquote(m.group(1)), unquote(m.group(2))
            name = proj.rsplit('/', 1)[-1]
            return 2, f"pkg:generic/{name.lower()}@{re.sub(r'^refs/tags/', '', sha)}"
        return 2, f"pkg:generic/source?download_url={u}"
    h = sa.get('hash') or sa.get('outputHash') \
        or (drv.get('outputs', {}).get('out', {}) or {}).get('hash')
    return (3, f"pkg:generic/source@{h}") if h else (4, None)


for path in sys.argv[1:]:
    drvs = json.load(open(path))['derivations']
    src = {k: v for k, v in drvs.items() if v.get('name') == 'source'}
    tiers = collections.Counter()
    groups = collections.defaultdict(list)
    for k, v in src.items():
        tier, ident = identity(v)
        tiers[tier] += 1
        groups[ident].append(v)

    print(f"\n{path}: {len(drvs)} derivations, {len(src)} named `source`")
    for t, label in ((1, 'forge owner/repo + tag|rev'), (2, 'fetch URL'),
                     (3, 'content hash only'), (4, 'nothing')):
        print(f"  tier {t} {label:28s} {tiers[t]:4d}")
    print(f"  distinct identities: {len(groups)}")

    # A collision is only safe if the colliding derivations are the same
    # content. Same owner/repo/rev with different output hashes would mean
    # the identity is under-determined and needs a discriminator.
    unsafe = 0
    for ident, members in groups.items():
        if len(members) < 2:
            continue
        hashes = {(m.get('outputs', {}).get('out', {}) or {}).get('hash')
                  for m in members}
        if len(hashes) > 1:
            unsafe += 1
            print(f"  !! {ident} collides across {len(hashes)} content hashes")
    print(f"  colliding identities: {sum(1 for m in groups.values() if len(m) > 1)}"
          f" (of which under-determined: {unsafe})")

    # The blind spot this probe found on the way.
    blind = [k for k, v in drvs.items()
             if (v.get('structuredAttrs') or {})
             and not any(f in (v.get('env') or {}) for f in FIELDS)
             and any((v['structuredAttrs'].get(f) or []) for f in FIELDS)]
    entries = sum(len(v['structuredAttrs'].get(f) or [])
                  for k in blind for v in [drvs[k]] for f in FIELDS)
    print(f"  derivations declaring dependencies ONLY in structuredAttrs: "
          f"{len(blind)} ({entries} dependency entries an env-only reader "
          f"cannot see)")
