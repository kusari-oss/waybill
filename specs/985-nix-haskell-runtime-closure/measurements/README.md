# Measurements — milestone 985 (issue #962)

The numbers in `../spec.md` Success Criteria come from here. Re-run these when
the package-set format changes or when a Success Criterion is questioned; an
undocumented upstream format is one that can move without notice.

Project names are omitted per the project's external-name policy. `haskell-language-server`
is named because it is a corpus target already pinned in-tree.

## The instruments

| file | answers |
|---|---|
| `closure_probe.py` | what waybill *would* compute, by parsing the package set |
| `nix_closure_oracle.sh` | what nix itself computes, via `nix eval` of the runtime inputs |
| `nix_version_oracle.sh` | what version nix resolves for each name |

The first is the instrument; the second and third are the **oracle**. Keeping
them separate is the point — a probe that shares code with the thing it
measures cannot disagree with it.

## Running

```bash
# what waybill would compute (needs a populated ~/.cache/waybill/nixpkgs)
python3 closure_probe.py <hackage-packages.nix> <configuration-ghc-N.nix> <names.txt>

# what nix computes, independently
./nix_closure_oracle.sh <nixpkgs-rev> <ghcNNN> <names.txt>
./nix_version_oracle.sh <nixpkgs-rev> <ghcNNN> <names.txt>
```

`names.txt` is one Haskell package name per line — the project's declared set,
obtainable from a waybill scan by taking every `pkg:hackage/*` component name.

## Findings, 2026-09-24

### Closure multiplier, by dependency class

| project | declared | library only | +executable | +test | +benchmark |
|---|---|---|---|---|---|
| A | 21 | 32 (1.5×) | 32 (1.5×) | 153 (7.3×) | 206 (9.8×) |
| B | 44 | 165 (3.8×) | 167 (3.8×) | 277 (6.3×) | 316 (7.2×) |
| `haskell-language-server` | 162 | 366 (2.3×) | 394 (2.4×) | 503 (3.1×) | 528 (3.3×) |

**The runtime closure is library + executable**, at 1.5–3.8×. This drives
SC-001.

### Agreement with the oracle

| project | probe (library+executable) | `nix eval` | |
|---|---|---|---|
| B | 167 | **167** | exact |
| A | 32 | 33 | one apart |

Two independent methods agreeing at 167 is what licenses SC-002. The single
disagreement on project A is `os-string`, whose bare name exists only through a
compiler-configuration alias — filed as issue #984 and fixed separately. It is
recorded here rather than smoothed over, because an unexplained off-by-one in a
measurement is how a wrong number enters a spec.

### Closure does not vary by GHC series

```
haskell-language-server   ghc9.6  ghc9.10  ghc9.14
  library+executable         425      425      425   (closure)
  resolvable                 394      394      392
project A                 ghc9.4   ghc9.6  ghc9.10
  library+executable          32       32       32
```

Only the *resolvable* count moves, by 1–2, from differing nulled boot sets.
This is the evidence behind the spec's assumption that the closure is not
per-package-set.

### Cycles

The walk marks `seen` before enqueue and terminated on every run — three
projects, seven series. FR-010 states the requirement rather than relying on
this observation holding for other package sets.

## A note on the instrument itself

`closure_probe.py` keys on the **attribute** name, not `pname`. That is
deliberate: 371 attributes at one measured revision share a `pname`, and keying
on `pname` returns a version the build never uses. The production parser was
fixed for exactly this in issue #970; the older probe in
`specs/926-nixpkgs-haskell-versions/measurements/` still keys on `pname` and its
closure figures should not be trusted.
