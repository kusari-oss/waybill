#!/usr/bin/env python3
"""Size #1282: which pkg:generic closure members are recoverable, and how.

Takes a waybill CDX document produced with --nix-closure, the `nix derivation
show -r` JSON for the same attribute, and the cached hackage-packages.nix for
the pinned revision. Reports three buckets and two independent signals.

  size-1282.py <cdx.json> <closure.json> <hackage-packages.nix>

Real coordinates are deliberate: the subject is nixpkgs' own content.
"""
import json, re, sys

def main(cdx_path, closure_path, hackage_path):
    drvs = json.load(open(closure_path))["derivations"]
    doc = json.load(open(cdx_path))
    txt = open(hackage_path, encoding="utf-8", errors="replace").read()

    # Every (pname, version) the pinned Haskell package set declares.
    pairs = {(m.group(1), m.group(2)) for m in
             re.finditer(r'pname\s*=\s*"([^"]+)"\s*;\s*version\s*=\s*"([^"]+)"\s*;', txt)}
    names = {n for n, _ in pairs}

    # Signal 2, independent of the package set: was this derivation built by a
    # GHC? `-ghc-binary-` matters -- happy-lib bootstraps with ghc-binary-9.0.2
    # and a stricter `-ghc-\d` pattern reports it as not-Haskell. A pattern
    # that disagrees with signal 1 on exactly one row is worth checking before
    # it is believed.
    GHC = re.compile(r'/nix/store/[a-z0-9]+-ghc(-binary)?-\d')
    def ghc_built(v):
        e = v.get("env") or {}
        return any(isinstance(s, str) and GHC.search(s)
                   for f, s in e.items()
                   if f in ("nativeBuildInputs", "depsBuildBuild",
                            "setupCompilerEnvironmentPhase"))

    built = {}
    for v in drvs.values():
        if not isinstance(v, dict):
            continue
        e = v.get("env") or {}
        if e.get("pname") is None:
            continue
        built.setdefault((e["pname"], e.get("version") or ""), ghc_built(v))

    g = lambda c, k: c.get(k) or ""
    gen = [c for c in doc["components"] if g(c, "purl").startswith("pkg:generic/")]
    rows = []
    for c in gen:
        k = (g(c, "name"), g(c, "version"))
        bucket = "exact" if k in pairs else ("name-only" if k[0] in names else "absent")
        rows.append((bucket, k[0], k[1], built.get(k)))

    print(f"pinned set: {len(pairs)} (pname,version) pairs, {len(names)} names")
    print(f"closure-appended pkg:generic components: {len(gen)}\n")
    print(f"{'bucket':<10} {'n':>4}  {'ghc-built':>9} {'not-ghc':>8} {'unknown':>8}")
    for b in ("exact", "name-only", "absent"):
        rs = [r for r in rows if r[0] == b]
        print(f"{b:<10} {len(rs):>4}  {sum(1 for r in rs if r[3] is True):>9}"
              f" {sum(1 for r in rs if r[3] is False):>8}"
              f" {sum(1 for r in rs if r[3] is None):>8}")

    ex = [r for r in rows if r[0] == "exact"]
    agree = sum(1 for r in ex if r[3] is True)
    print(f"\nthe two signals agree on {agree} of {len(ex)} exact-pair rows")
    outside = [r for r in rows if r[3] is True and r[0] != "exact"]
    print(f"ghc-built but not an exact pair: {len(outside)}  "
          f"{[r[1] for r in outside][:6]}")

    # The collision the name-only bucket proves. A name match alone would mint
    # pkg:hackage/bash@5.3p9 for GNU bash.
    print("\nwhy a name-only match is unsafe (closure version vs set version):")
    for b, n, v, _ in [r for r in rows if r[0] == "name-only"][:8]:
        inset = sorted(vv for nn, vv in pairs if nn == n)
        print(f"   {n:<20} closure={v or '<none>':<14} hackage={inset[:2]}")

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(__doc__)
        sys.exit(2)
    main(*sys.argv[1:4])
