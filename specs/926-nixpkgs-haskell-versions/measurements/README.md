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

## M2 — A nulled name is compiler-supplied; two narrower rules were wrong

`configuration-ghc-<series>.nix` binds boot libraries to `null`, meaning
"ships with the compiler, do not build from Hackage".

| GHC series | `= null;` bindings | also in the package set |
|---|---|---|
| 9.0.x | 35 | 34 |
| 9.4.x | 40 | 36 |
| 9.6.x | 40 | 35 |
| 9.8.x | 41 | 37 |
| 9.10.x | 41 | 37 |
| 9.12.x | 44 | 40 |
| 9.14.x | 46 | 42 |
| 9.16.x | 47 | 43 |

Across all eight: 50 names in union, 32 in intersection — the sets are **not
identical**, so a hardcoded list of "the seven boot libraries" would be wrong.
The nulled set must be read at the pinned revision.

**Two attempts to narrow the set were both rejected by measurement.**

*Attribute-set nesting depth* drops `editedCabalFile` from 9.6.x correctly,
and also drops `directory-ospath-streaming` from 9.4.x — a real package at
v0.3. Real boot libraries live at deeper nesting too.

*Package-set membership* drops `editedCabalFile` correctly, and also drops
four real packages. The nulled names absent from the package set are exactly:

| name | real package? |
|---|---|
| `editedCabalFile` | no — a derivation attribute |
| `rts` | **yes** — the GHC runtime system |
| `ghc-platform` | **yes** — GHC-bundled |
| `ghc-toolchain` | **yes** — GHC-bundled |
| `system-cxx-std-lib` | **yes** — GHC-bundled |

Those four are missing from `hackage-packages.nix` *because* they are never
built from Hackage. Excluding them reports `absent-from-package-set` where the
truth is `compiler-supplied`.

**The rule is therefore the simple one**: nulled means compiler-supplied.
`editedCabalFile` is left in and is inert — the set is only consulted for
names a project declared, and that is not a legal package name.

Over-including withholds a version; under-including invents one. Both
rejected rules failed toward under-inclusion.

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
