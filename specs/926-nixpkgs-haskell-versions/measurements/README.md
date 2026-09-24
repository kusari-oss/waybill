# Measurements — nixpkgs-resolved Haskell versions (#947)

Run: `python3 probe_nixpkgs_haskell.py [--rev REV] [--series 9.6.x] [--json]`

Every number in `../spec.md` that describes nixpkgs behaviour comes from
here. Re-run the probe when the nixpkgs layout moves; an undocumented
layout is one that can change without notice.

Observed 2026-09-24 against nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb` (the revision pinned by the
`flake.lock` of the #947 measurement target — a public cabal + hpack +
Nix Haskell library).

## M1 — A pinned revision does resolve name → (version, source hash)

| quantity | observed |
|---|---|
| `pkgs/development/haskell-modules/hackage-packages.nix` | **16,634,427 bytes** |
| derivations parsed | **19,058** |

Sample, verbatim from the probe:

```
aeson                  2.2.4.1      sha256=0q7s09y0nqnf2rb06j...
cmark-gfm              0.2.6        sha256=0sd8q42j51ba7ymyxk...
th-compat              0.1.7        sha256=1zym9yia0is8wxfd6d...
uuid-types             1.0.6.1      sha256=091h1ifc1srv803rrk...
```

Both the byte count and the derivation count reproduce #947's figures
exactly, and the sampled versions match the three the issue quoted. The
core premise of the issue is confirmed: **design tier → source tier, with
content hashes, for a repository shipping no lockfile.**

## M2 — A nulled name is a boot library only if it is also a package

`configuration-ghc-<series>.nix` binds boot libraries to `null`, meaning
"ships with the compiler, do not build from Hackage" — so those packages
have no `hackage-packages.nix` version for that package set.

| GHC series | `= null;` bindings | of which are packages |
|---|---|---|
| 9.4.x | 40 | **36** |
| 9.6.x | 40 | **35** |
| 9.10.x | 41 | **37** |

Across all eight series present at this revision: 50 names in union, 32 in
intersection — the sets are **not identical**. Symmetric difference,
9.4.x vs 9.10.x: only in 9.4.x `directory-ospath-streaming`, `libiserv`;
only in 9.10.x `os-string`, `semaphore-compat`, `xhtml`.

**Design consequence 1**: a hardcoded list of "the seven boot libraries"
would be wrong. The nulled set must be read from the per-compiler
configuration at the pinned revision.

**Design consequence 2 — and this one reversed an earlier decision.** The
naive extraction also reports `editedCabalFile`, an attribute of a
derivation override rather than a package. Scoping by attribute-set
nesting depth removes it, and that rule was implemented first. Measurement
then rejected it: the depth rule *also* removed
`directory-ospath-streaming` from GHC 9.4.x, which is a real package at
v0.3 that the compiler genuinely supplies. Real boot libraries live at
deeper nesting too.

The discriminator that works is package-set membership:

| name | nulled | in the package set | boot library? |
|---|---|---|---|
| `editedCabalFile` | yes | no | no |
| `directory-ospath-streaming` | yes (9.4.x) | yes, v0.3 | yes |
| `base` | yes | yes, v4.22.0.0 | yes |

The asymmetry decides it. Over-including a boot library withholds a
version; under-including one invents a version the build never uses. The
depth rule failed toward the dangerous direction.

## M3 — The 12-of-19 split could not be reproduced, and the direction is against it

#947 reports **12 of 19 resolved, 7 boot**. This probe gets **10 resolved,
9 boot** for all three GHC series.

Two reasons to treat the issue's split as unverified rather than to treat
this probe as authoritative:

1. **The declared-dependency list here is a reconstruction.** The issue
   does not enumerate the target's 19 dependencies, so
   `DEFAULT_PACKAGES` in the probe is inferred. A different list gives a
   different split. This number is therefore *not* established by either
   source.
2. **The boot half is established.** The delta is `deepseq` and
   `transformers`. Both are bound to `null` in
   `configuration-ghc-9.4.x.nix`, `-9.6.x.nix` and `-9.10.x.nix` at this
   revision — read directly from the config file, independent of any
   dependency list. #947's quoted snippet listed seven names and did not
   include them; it reads as abridged rather than wrong.

So the honest statement is: **more of the declared set are boot libraries
than the issue's snippet showed, and the exact resolved count depends on a
dependency list neither source has enumerated.**

`spec.md` therefore states no fixed "N of 19" success criterion. SC-001 is
expressed against the boot set *as discovered from the pinned revision*,
which is measurable without knowing the target's dependency list.
