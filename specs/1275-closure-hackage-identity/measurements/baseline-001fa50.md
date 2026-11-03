# Baseline: what the closure tier emits before milestone 1275

**Task**: T001 | **Commit**: `001fa50` (= this branch's code; see below)
**Date**: 2026-11-03 | **Host**: aarch64-darwin
**nixpkgs**: `a799d3e3886da994fa307f817a6bc705ae538eeb`
**Project**: a flake depending on `aeson` and `text`
(`waybill-fixture-ifd`, 1,290 derivations → 341 components)

## The baseline is this branch's, and that is verified rather than assumed

`git diff --stat 001fa50..HEAD -- '*.rs' '*.toml'` is **empty**: every commit
on this branch so far is documentation. So the artifacts captured at `001fa50`
are what this branch's binary produces, and re-scanning could not have
produced a different number. The empty diff is the evidence; a re-run would
only have agreed with it.

## Probe output, reproduced

```
pinned set: 19437 (pname,version) pairs, 19058 names
closure-appended pkg:generic components: 276

bucket        n  ghc-built  not-ghc  unknown
exact        60         60        0        0
name-only    26          0       26        0
absent      190          1      175       14

the two signals agree on 60 of 60 exact-pair rows
ghc-built but not an exact pair: 1  ['ghc-settings-edit']
```

Every figure `quickstart.md` records is reproduced exactly. **SC-009 / C21
holds for `size-closure-members.py`.**

## What this branch must change

| | measured now | target |
|---|---|---|
| closure members with `pkg:hackage` identity | **0** of 60 | 59 (SC-001; the 60th is the alias gap) |
| those carrying a license | **0** | ≥ the resolved set's 47 of 65 rate (SC-002) |
| `pkg:generic` closure members carrying a reason | **0** of 276 | 276 (SC-004) |
| out-edges from closure-appended components | **0** | ≥265 of 278 declared (SC-005a) |
| `pkg:hackage` PURLs whose version is absent from the set | 0 | **0** — must stay 0 (SC-003) |

The last row is the one that can only get worse. It is 0 today because the
tier mints no Hackage PURLs at all; it stays 0 only if the collision rule
works.

## The collision population, verbatim from the probe

A name match alone would mint these. Each is a real Hackage package name
attached to software that has nothing to do with Haskell:

```
asciidoc      closure=10.2.1    hackage=0.1.0.2
bash          closure=5.3p9     hackage=0.1.8
brotli        closure=1.2.0     hackage=0.0.0.3
curl          closure=8.20.0    hackage=1.3.8
filelock      closure=3.20.3    hackage=0.1.1.9
ghc           closure=9.4.8     hackage=9.14.1
idna          closure=3.13      hackage=0.3.0
libarchive    closure=3.8.7     hackage=3.0.5.0
```

26 in total. `ghc 9.4.8` is the one worth staring at: the compiler itself
collides with a Hackage package called `ghc`, at a version 5 major releases
ahead of what the closure holds.

## Reproducing

```sh
python3 -I specs/1275-closure-hackage-identity/measurements/size-closure-members.py \
    <cdx.json> <closure.json> \
    ~/.cache/waybill/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb/hackage-packages.nix
```

Producing the two inputs: see `quickstart.md`. The closure JSON is 6.4 MB and
the emitted CDX 358 KB.
