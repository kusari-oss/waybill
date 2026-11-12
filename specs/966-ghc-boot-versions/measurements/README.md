# #966 findings: GHC boot-library versions

Measured 2026-11-12, aarch64-darwin, nix 2.34.6, waybill main@d2041e9d.
Probes are in this directory: `patchdiff.py`, `compare.py`, `notes_vs_hints.py`,
`coverage.py`. `osv-boot.txt` summarises the OSV responses (the per-package JSON
was not kept).

## Re-running

Every script runs with `python3 -I -B` and reads only the paths it is given.
`global-hints-2026-11-12.yaml` is the Stack snapshot every number below came
from. Use a fresh copy (`stack/global-hints.yaml` in
commercialhaskell/stackage-content) to measure today.

| question | command |
|---|---|
| Does a series pin boot-library versions? | `patchdiff.py global-hints-2026-11-12.yaml` |
| Do local GHC package databases agree with Stack? | `compare.py global-hints-2026-11-12.yaml /nix/store/<hash>-ghc-X.Y.Z …` |
| Do the release notes agree with Stack? | Fetch `https://downloads.haskell.org/ghc/<v>/docs/users_guide/<v>-notes.html` for every `<v>` in `notes-fetch.txt` into `<dir>`, then `notes_vs_hints.py global-hints-2026-11-12.yaml notes-fetch.txt <dir>` |
| How much would a table resolve on the goldens? | `coverage.py global-hints-2026-11-12.yaml <target>/cdx.json …` |
| Which boot libraries have OSV advisories? | `POST https://api.osv.dev/v1/query` with `{"package":{"name":<n>,"ecosystem":"Hackage"}}` per name (`osv-boot.txt`) |

Implementation-time records: `table-generation.txt` (T002), `db-check.txt`
(T029), `fr012.txt` (T031), `golden-review.txt` (T032), `sc002-osv.txt` (T034).


## Answer

The versions can be obtained, but only given the exact GHC patch version. That
version is known on the closure tier and is not reliably known on the static
(m926) tier.

## Lead 4: per-series or per-patch? Per-patch.

Using Stack's `global-hints.yaml`, every series that has more than one release
changes non-GHC-versioned boot libraries between patch releases. `base` and
`process` change in all of them:

| series | patch releases | libraries whose version varies |
|---|---|---|
| 9.4 | 8 | 8 (base, bytestring, containers, process, text, …) |
| 9.6 | 7 | 9 (base, bytestring, Cabal, directory, process, unix, …) |
| 9.8 | 4 | 12 |
| 9.10 | 3 | 18 |
| 9.12 | 4 | 21 |

A table keyed by series is therefore wrong. It has to be keyed by
`ghc-X.Y.Z`.

## Sources, against #966's criteria

| source | machine-readable | no host nix/ghc | pinned by lockfile | coverage | accuracy |
|---|---|---|---|---|---|
| GHC release notes, "Included libraries" table | HTML table | yes | by GHC version | 9.4.2–9.14.1. 9.0.x, 9.2.x and 9.4.1 return 404 or have no table. Omits rts, ghc-bignum, system-cxx-std-lib, and in 9.10 ghc-internal and os-string. | Agreed with GHC source on the one disputed entry |
| Stack `global-hints.yaml` | YAML | yes | by GHC version | 9.0.1–9.14.1, all libraries | **1 error**: 9.12.3 `process` 1.6.26.0. GHC's tag pins 1.6.26.1 |
| GHC git at `ghc-X.Y.Z-release` | `.cabal`, but several are `.cabal.in` templates | yes | by GHC version | all | authoritative, but costs one fetch per library per release |
| The GHC store path's `lib/ghc-X.Y.Z/lib/package.conf.d` | file names are `<name>-<version>.conf` | needs the path local, or `nix store ls --store <substituter>` | **by store hash** | whatever was built | authoritative for that exact artifact |
| nixpkgs | — | — | — | does not carry the table (as #966 already found) | — |

Agreement checks:
- **Release notes vs Stack:** 26 versions, 943 of 944 shared entries agree. The
  one disagreement is the 9.12.3 `process` error above.
- **Store vs Stack:** the local store holds ghc-9.6.6 (one path) and ghc-9.10.3
  (four paths). They agree on 37 of 37 and 42 of 42 libraries. The only
  difference is `Win32`, which a darwin GHC doesn't ship.
- **Binary cache vs local store:** the cache.nixos.org listing for the
  ghc-9.6.6 path is identical to the local database, 37 of 37 files.
  - `nix store ls --store https://cache.nixos.org <path>/lib/ghc-9.6.6/lib/package.conf.d`
    took 7.73, 7.76 and 9.18 s over three runs.
  - The raw `.ls` is brotli-encoded, and the lockfile has no brotli crate.

## Lead 2, reconsidered

#966 set this lead aside because reading the package database "means having
the compiler." The closure tier already has nix and already names the compiler.
Every Haskell closure golden records the exact GHC:
- moat, shellcheck, slack-web and slacklinker: `ghc@9.10.3`
- openai: `ghc@9.6.6`

Each also records a separate bootstrap `ghc-binary`.

## Coverage on the committed goldens (table lookup at the closure's GHC)

| target | GHC | versionless pkg:hackage | resolved | left versionless (correctly) |
|---|---|---|---|---|
| nix-closure-moat | 9.10.3 | 24 | 24 | — |
| nix-closure-openai | 9.6.6 | 26 | 25 | os-string |
| nix-closure-shellcheck | 9.10.3 | 26 | 26 | — |
| nix-closure-slack-web | 9.10.3 | 27 | 26 | hspec-discover |
| nix-closure-slacklinker | 9.10.3 | 29 | 27 | hunit, slack-web |
| haskell-language-server, haskell-security-advisories | none recorded (static tier) | 39, 27 | — | — |

**Side finding:** on openai, `os-string` is labelled compiler-supplied, but GHC
9.6.6 does not ship it. It was nulled only in the 9.10+ configs, and the m926
union rule took those configs in. With the exact GHC version, the boot set
itself becomes exact.

## What versions would surface (OSV, Hackage ecosystem, all 55 boot-library names)

Only `base`, `process` and `Cabal` have advisories:

| advisory | affected | on the corpus GHCs |
|---|---|---|
| HSEC-2023-0007 base `readFloat` | ≥ 3.0.3.1, no fix | every target (base 4.18.2.1, 4.20.2.0) |
| HSEC-2024-0006 base `fromIntegral` | 4.15.0.0 only (GHC 9.0.1) | none |
| HSEC-2024-0003 process (Windows only) | < 1.6.23.0 | openai (1.6.19.0). **On a Nix host this matches a Windows-only flaw, so it is noise that VEX would need to suppress.** |
| HSEC-2026-0006 Cabal | ≥ 2.2 | already matched through #1472's alias path, where present |

sbomnix emits only `ghc-9.10.3`, which matches no Hackage advisory.
*Inference, not measured:* sbomnix therefore cannot report any of these.

## Static tier (m926)

- `candidate_series` reads the flake's text and falls back to every series.
- At a pinned revision, `haskell-packages.nix` maps each series alias to one
  patch release (`ghc96 = sets.ghc967`, `ghc912 = sets.ghc9123`).
  `haskellPackages` is `ghc910`, except on static or ghcjs hosts.
- An overlay can override any of this, and the static tier can't see overlays.
- So the version is determined only when exactly one series is named and no
  overlay replaces the compiler. The second condition can't be checked
  statically.
