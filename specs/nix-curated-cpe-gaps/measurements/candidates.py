#!/usr/bin/env python3
"""Which derivation(s) in each target's build closure carry a candidate name?

For each candidate, prints every derivation whose `name` is `<cand>-<version>`
or whose pname (from env or structuredAttrs) equals the candidate, with its
pname, version, src URL(s) and whether it looks like a Haskell package
(`haskell` in builder/args/setup hooks, or a hackage src URL).

Usage: DRV_DIR=<drv-dump.sh output> candidates.py <cand>...
"""
import json, os, re, sys

D = os.environ.get("DRV_DIR") or sys.exit("set DRV_DIR")
TARGETS = "curl openssh httpie gh ripgrep moat".split()
cands = sys.argv[1:]


def attrs(drv):
    env = drv.get("env", {})
    sa = env.get("__json")
    if sa:
        try:
            return {**env, **json.loads(sa)}
        except Exception:
            pass
    return env


def srcs(a):
    s = a.get("src") or ""
    urls = a.get("urls") or a.get("url") or ""
    return s, urls


for c in cands:
    print(f"=== {c}")
    seen = {}
    for t in TARGETS:
        with open(f"{D}/{t}.json") as f:
            data = json.load(f)
        drvs = data.get("derivations", data)
        for path, drv in drvs.items():
            name = drv.get("name", "")
            a = attrs(drv)
            pname = a.get("pname") or ""
            if pname != c and not re.match(rf"^{re.escape(c)}-\d", name):
                continue
            key = path
            if key in seen:
                seen[key][1].append(t)
                continue
            blob = json.dumps(drv)
            haskell = ("hackage" in blob) or ("ghc-" in blob and "Setup" in blob) or ("haskell" in blob.lower())
            src, urls = srcs(a)
            seen[key] = ((name, pname, a.get("version", ""), haskell, src, urls), [t])
    for path, ((name, pname, ver, hs, src, urls), ts) in seen.items():
        print(f"  {name}  pname={pname!r} version={ver!r} haskell-ish={hs}")
        print(f"    drv={path.split('/')[-1]} targets={ts}")
        if urls:
            print(f"    urls={str(urls)[:200]}")
