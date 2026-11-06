#!/usr/bin/env python3
"""Classify the `pname`-less derivations of a nix closure (Gap A, milestone TBD).

Usage:  python3 -I classify-nameless.py <closure.json>

`nix derivation show --recursive <flakeref>` produces the input. Every number
this prints is derived from that file alone; nothing is fetched.

Three traps this script exists to avoid, each of which produced a wrong figure
during exploration:

  * `builder` is a TOP-LEVEL key, not an `env` key. Testing `env["builder"]`
    invents a class of 421 "derivations with no builder" that does not exist.
  * A fixed-output derivation records its hash in `outputs[<name>]["hash"]`.
    `env["outputHash"]` is the older, unstructured spelling and is present on
    only a quarter of them -- 139 against 549 here.
  * The top-level `version` key is the DERIVATION FORMAT version ("4" for
    every node in this closure), not a package version. Reading it as one
    makes every bootstrap fragment look versioned.
"""
import json, re, sys, collections

def content_hash(drv):
    for out in drv.get("outputs", {}).values():
        if isinstance(out, dict) and "hash" in out:
            return out["hash"]
    return drv.get("env", {}).get("outputHash")

def source_url(drv):
    env = drv.get("env", {})
    return env.get("urls") or env.get("url")

FORGE = re.compile(r"(github|gitlab|codeberg)\.(com|org)")
VERSIONED = re.compile(r"-\d+(\.\d+)*(-|$)")

def classify(drv):
    """source | package | scaffolding | fragment"""
    if content_hash(drv):
        return "source"
    if "text" in drv.get("env", {}):
        return "scaffolding"
    if VERSIONED.search(drv.get("name", "")):
        return "package"
    return "fragment"

def main(path):
    doc = json.load(open(path))
    drvs = doc["derivations"] if "derivations" in doc else doc
    nameless = {k: v for k, v in drvs.items() if not v.get("env", {}).get("pname")}

    print(f"closure derivations : {len(drvs)}")
    print(f"  with pname        : {len(drvs) - len(nameless)}")
    print(f"  WITHOUT pname     : {len(nameless)}"
          f" ({100*len(nameless)/len(drvs):.0f}%)  <- Gap A")
    print()

    classes = collections.Counter(classify(v) for v in nameless.values())
    for name in ("source", "package", "scaffolding", "fragment"):
        print(f"  {name:12s} {classes[name]:5d}")
    print()

    urls = [v for v in nameless.values() if source_url(v)]
    forge = [v for v in urls if FORGE.search(str(source_url(v)))]
    print(f"carrying a source url : {len(urls)}")
    print(f"  forge-hosted        : {len(forge)}"
          f"  ({100*len(forge)/len(nameless):.0f}% of Gap A)")
    print()

    # The collision test. A name-keyed identity is only safe if one name never
    # covers two different contents; it does not hold here, which is why the
    # hash belongs IN the identity rather than beside it.
    by_name = collections.defaultdict(set)
    for v in nameless.values():
        h = content_hash(v)
        if h:
            by_name[v.get("name", "")].add(h)
    collisions = {n: h for n, h in by_name.items() if len(h) > 1}
    print(f"distinct content-hashed names : {len(by_name)}")
    print(f"  names covering >1 content   : {len(collisions)}")
    for n, h in sorted(collisions.items(), key=lambda kv: -len(kv[1])):
        print(f"      {n!r} -> {len(h)} distinct hashes")

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "closure.json")

# ---------------------------------------------------------------- identity
# Appended after the class counts settled: the question that decides the
# emission scheme is not "what are these" but "can each be told apart".
#
# Run with `--identity` to print the identity comparison.

def identity_report(nameless):
    print()
    print(f"=== identity over all {len(nameless)} Gap A derivations ===")

    names = collections.Counter(v.get("name", "") for v in nameless.values())
    collide = sum(c - 1 for c in names.values() if c > 1)
    print(f"  name alone          : {len(names):5d} distinct, {collide} collide")

    # The `.drv` basename's hash is nix's own content-address OF THE RECIPE,
    # so it separates derivations that no other field distinguishes --
    # including the scaffolding and bootstrap fragments, which carry no
    # content hash of their own.
    pairs = {(v.get("name", ""), k[:32]) for k, v in nameless.items()}
    print(f"  name + .drv hash    : {len(pairs):5d} distinct,"
          f" {len(nameless) - len(pairs)} collide")

    hashed = {k: v for k, v in nameless.items() if content_hash(v)}
    ci = {(v.get("name", ""), content_hash(v)) for v in hashed.values()}
    print(f"  name + content hash : {len(ci):5d} distinct over {len(hashed)}"
          f" content-hashed, {len(hashed) - len(ci)} merge")
    print("      (a 'merge' here is correct: the same content fetched by two")
    print("       derivations is one component, not two)")
    print()
    print("  worst name collisions, which is why a hash belongs in the identity:")
    for n, c in names.most_common(5):
        if c > 1:
            print(f"      {n!r:34s} x{c}")

if __name__ == "__main__" and "--identity" in sys.argv:
    _doc = json.load(open([a for a in sys.argv[1:] if not a.startswith("-")][0]))
    _drvs = _doc["derivations"] if "derivations" in _doc else _doc
    identity_report({k: v for k, v in _drvs.items()
                     if not v.get("env", {}).get("pname")})
