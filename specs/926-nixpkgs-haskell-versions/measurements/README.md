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

## M3 — #947's 12-of-19 split is CONFIRMED; this probe's 10-of-19 was wrong

An earlier version of this document reported that #947's "12 of 19 resolved,
7 boot" could not be reproduced, and that the probe's 10-of-19 put the
direction against it. **That was wrong, and the issue was right.**

Running the implemented feature against the real repository:

```
resolved=12  unresolved={"compiler-supplied": 7}
revision=a799d3e3886da994fa307f817a6bc705ae538eeb
```

12 + 7 = 19. Exactly the issue's figures, including the boot set — `base`,
`bytestring`, `containers`, `mtl`, `template-haskell`, `text`, `time` — which
matches the seven names #947 quoted, name for name.

### Why the probe disagreed

`DEFAULT_PACKAGES` in this probe is a **reconstruction**. The issue does not
enumerate the target's dependencies, so the list was inferred — and inferred
badly. Eight of its nineteen entries are packages the project does not declare
at all:

```
aeson  deepseq  errors  http-api-data  http-client  scientific
transformers  vector-algorithms
```

and it omitted seven the project does declare (`case-insensitive`,
`haddock-library`, `hspec`, `hspec-discover`, `hspec-golden`, `primitive`,
`th-abstraction`, `unordered-containers`). Two of the invented entries,
`deepseq` and `transformers`, are boot libraries in some GHC package sets —
which is exactly how the probe manufactured a 9-boot answer from a 7-boot
project.

### What this is a lesson in

The probe's nixpkgs-side numbers were right and reproduced #947 exactly: file
size, derivation count, sampled versions. The *only* wrong input was the one
that came from guessing rather than measuring, and it produced a confident
wrong answer that survived three rounds of analysis — including being written
into a spec as a reason not to trust the issue.

A measurement is only as good as its least-measured input. The fix was not a
better probe; it was running the implementation against the actual repository.

### Consequence for the spec

SC-001 remains expressed against the boot set as discovered from the pinned
revision rather than as a fixed "N of 19". That was the right call for a
different reason than originally given: not because the split is unverifiable,
but because it is a property of one repository and a success criterion should
not hard-code one project's dependency count.
