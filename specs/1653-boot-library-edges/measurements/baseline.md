# Spec 1653: FR-001 measurements

Every figure here was executed on 2026-11-24. The scripts are beside this file.

## Where a release's package database can be read

| Source | Reachable for | Cost | Used |
|---|---|---|---|
| The store output, at scan time | 3 of 10 compiler instances in the goldens (C214: the project compiler of slack-web, openai, slacklinker); no bootstrap compiler | none, but depends on the machine | **no** (Clarification 1) |
| nixpkgs' binary cache, file by file (`nix store cat --store https://cache.nixos.org`) | only the releases some nixpkgs revision builds; current nixpkgs has `ghc967`, `ghc9103`, `ghc902Binary`, `ghc984Binary`, not `ghc90` | ≈2.6 s per `.conf` (range reads, not the 225 MB NAR); 1 min 54 s for 42 files | as the **check** (`nixcache.py`) |
| GHC's release bindist, `downloads.haskell.org/~ghc/<v>/ghc-<v>-x86_64-deb10-linux.tar.xz` | all 36 releases | 200–295 MiB each, streamed once; ≈3.4 s download for 9.10.3, a few minutes per release for Python's decompression | as the **source** (`bindist.py`) |

**Bindist layouts:**
- **9.4 and later (Hadrian)** ship `lib/package.conf.d/*.conf`, and dependencies are read from `depends:`.
- **9.0 and 9.2 (make)** ship no `.conf` files. Each library's `libraries/<lib>/dist-install/package-data.mk` carries `_DEP_NAMES`, and the `ghc` library's is `compiler/stage2/package-data.mk`. `rts` comes from `rts/package.conf.in`, whose `depends:` is empty.

## The bindist agrees with nixpkgs' own builds

`cmp.py`, the bindist against nixpkgs' installed package database read from the binary cache. The edges are each library's direct dependencies, by name:

```
bd-9.10.3.json libs 42 42 only-bindist [] only-nixpkgs [] edges 199 199 differ 0
bd-9.0.2.json libs 35 35 only-bindist [] only-nixpkgs [] edges 155 155 differ 0
bd-9.6.7.json libs 37 37 only-bindist [] only-nixpkgs [] edges 179 179 differ 0
bd-9.8.4.json libs 37 37 only-bindist [] only-nixpkgs [] edges 175 175 differ 0
```

The four releases cover:
- both layouts (9.0.2 is make-built, the rest Hadrian);
- a nixpkgs source build (`ghc967`, `ghc9103`);
- both bootstrap compilers the goldens use, or their series (`ghc902Binary`, `ghc984Binary`).

**There are 0 differences in 4 of 4.**

**The issue's premise was wrong for 9.10.** Its `base` depends on `ghc-internal` and `ghc-prim`, not `ghc-bignum`. 9.6 and earlier depend on `ghc-bignum`, `ghc-prim` and `rts`.

## All 36 releases, against milestone 966's version table
| release | layout | libraries | edges | m966 libraries | only in probe | only in m966 | version disagreements |
|---|---|---|---|---|---|---|---|
| 9.0.1 | package-data.mk | 35 | 155 | 35 | — | — | — |
| 9.0.2 | package-data.mk | 35 | 155 | 35 | — | — | — |
| 9.2.2 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.3 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.4 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.5 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.6 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.7 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.2.8 | package-data.mk | 35 | 157 | 35 | — | — | — |
| 9.4.1 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.2 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.3 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.4 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.5 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.6 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.7 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.4.8 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.6.1 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.2 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.3 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.4 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.5 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.6 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.6.7 | package.conf.d | 37 | 179 | 37 | — | — | — |
| 9.8.1 | package.conf.d | 37 | 176 | 37 | — | — | — |
| 9.8.2 | package.conf.d | 37 | 176 | 37 | — | — | — |
| 9.8.3 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.8.4 | package.conf.d | 37 | 175 | 37 | — | — | — |
| 9.10.1 | package.conf.d | 42 | 198 | 42 | — | — | — |
| 9.10.2 | package.conf.d | 42 | 198 | 42 | — | — | — |
| 9.10.3 | package.conf.d | 42 | 199 | 42 | — | — | — |
| 9.12.1 | package.conf.d | 45 | 227 | 45 | — | — | — |
| 9.12.2 | package.conf.d | 45 | 227 | 45 | — | — | — |
| 9.12.3 | package.conf.d | 45 | 229 | 45 | — | — | — |
| 9.12.4 | package.conf.d | 45 | 230 | 45 | — | — | — |
| 9.14.1 | package.conf.d | 47 | 230 | 47 | — | — | — |

## What the table yields on the five closure goldens (main `91d84d1`)
| target | compiler | boot libs | in table | with ≥1 edge after | edges after | deps not in document | known leaves |
|---|---|---|---|---|---|---|---|
| moat | ghc-9.10.3 | 25 | 25 | 24 | 121 | ghc-boot×1, ghc-boot-th×1, ghc-internal×2, ghci×1, hpc×1, rts×2, semaphore-compat×1 | 0 |
| moat | ghc-binary-9.8.4 | 11 | 11 | 10 | 27 | array×2, directory×1, ghc-bignum×1, ghc-boot-th×1, ghc-prim×4, mtl×1, pretty×1, rts×1, stm×1, time×1 | 0 |
| shellcheck | ghc-9.10.3 | 27 | 27 | 26 | 125 | ghc-boot-th×2, ghc-internal×3, ghc-platform×1, ghci×1, hpc×1, os-string×4, rts×2, semaphore-compat×1 | 0 |
| shellcheck | ghc-binary-9.8.4 | 12 | 12 | 11 | 32 | array×3, directory×1, ghc-bignum×1, ghc-boot-th×1, ghc-prim×5, mtl×1, pretty×1, rts×1, stm×1, time×1 | 0 |
| slack-web | ghc-9.10.3 | 27 | 27 | 26 | 125 | ghc-boot×1, ghc-boot-th×1, ghc-internal×3, ghci×1, hpc×1, rts×2, semaphore-compat×1 | 0 |
| slack-web | ghc-binary-9.8.4 | 13 | 13 | 12 | 44 | array×4, directory×3, ghc-bignum×1, ghc-boot-th×1, ghc-prim×4, mtl×3, parsec×2, pretty×3, rts×1, stm×1, text×2, time×3 | 0 |
| openai | ghc-9.6.6 | 26 | 26 | 25 | 119 | ghc-boot×1, ghc-boot-th×1, ghci×1, hpc×1, rts×3 | 0 |
| openai | ghc-binary-9.2.4 | 14 | 14 | 13 | 32 | ghc-bignum×1, ghc-boot-th×1, ghc-prim×3, pretty×1, rts×1 | 0 |
| slacklinker | ghc-9.10.3 | 28 | 28 | 27 | 134 | ghc-boot-th×2, ghc-internal×3, ghc-platform×1, ghci×1, hpc×1, rts×2, semaphore-compat×1 | 0 |
| slacklinker | ghc-binary-9.8.4 | 11 | 11 | 10 | 27 | array×2, directory×1, ghc-bignum×1, ghc-boot-th×1, ghc-prim×4, mtl×1, pretty×1, rts×1, stm×1, time×1 | 0 |

**Agreement with milestone 966:** every release has the same library set as milestone 966's version table (Windows-only rows excluded), with **0 version disagreements**, independently of how those versions were sourced.

## How many boot libraries the document can connect fully

A boot library is fully connected when every dependency the table records is a component in the document. Documents contain only the boot libraries something declared or the closure synthesised, so `rts`, `ghc-internal` and, for the bootstrap compilers, `ghc-prim` are often absent.

| target | compiler | boot libs | all deps in document | some deps absent | no edge at all |
|---|---|---|---|---|---|
| moat | ghc-9.10.3 | 25 | 20 | 5 | 1: ghc-prim -> rts |
| moat | ghc-binary-9.8.4 | 11 | 2 | 9 | 1: base -> ghc-bignum/ghc-prim/rts |
| shellcheck | ghc-9.10.3 | 27 | 16 | 11 | 1: ghc-prim -> rts |
| shellcheck | ghc-binary-9.8.4 | 12 | 2 | 10 | 1: base -> ghc-bignum/ghc-prim/rts |
| slack-web | ghc-9.10.3 | 27 | 21 | 6 | 1: ghc-prim -> rts |
| slack-web | ghc-binary-9.8.4 | 13 | 2 | 11 | 1: base -> ghc-bignum/ghc-prim/rts |
| openai | ghc-9.6.6 | 26 | 21 | 5 | 1: ghc-prim -> rts |
| openai | ghc-binary-9.2.4 | 14 | 11 | 3 | 1: base -> ghc-bignum/ghc-prim/rts |
| slacklinker | ghc-9.10.3 | 28 | 21 | 7 | 1: ghc-prim -> rts |
| slacklinker | ghc-binary-9.8.4 | 11 | 2 | 9 | 1: base -> ghc-bignum/ghc-prim/rts |
