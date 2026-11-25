# Identity and version probes (2026-11-25)

## Hackage presence

`curl -s -o /dev/null -w '%{http_code}' https://hackage.haskell.org/package/<name>`

| name | status |
|---|---|
| rts | 404 |
| ghc-platform | 404 |
| ghc-internal, ghc-prim, ghc-bignum, ghc-boot, ghc-boot-th, ghci, hpc, os-string, semaphore-compat, stm | 200 |

## OSV by purl

`curl -s -X POST https://api.osv.dev/v1/query -d '{"package":{"purl":"<purl>"}}'`

| purl | advisories |
|---|---|
| pkg:hackage/rts@1.0.2 | 0 |
| pkg:generic/rts@1.0.2 | 0 |
| pkg:hackage/ghc-platform@0.1.0.0 | 0 |
| pkg:generic/ghc-platform@0.1.0.0 | 0 |
| pkg:hackage/base@4.17.0.0 (control) | 1: HSEC-2023-0007 |
| pkg:hackage/process@1.6.17.0 (control) | 1: HSEC-2024-0003 |
| pkg:generic/curl@7.60.0 (control, spec 1373) | 1: OSV-2017-79 |

By name and ecosystem `Hackage`, every name this feature adds returns 0
advisories. The controls `base` (2) and `process` (1) return advisories.

## OSV's GHC ecosystem

`https://osv-vulnerabilities.storage.googleapis.com/ecosystems.txt` lists both
`GHC` and `Hackage`. In the `GHC` ecosystem, name `ghc` has three advisories
(HSEC-2024-0007, HSEC-2024-0008, HSEC-2025-0001), each with
`affected[].package = {ecosystem: GHC, name: ghc}` and no purl. Name `rts` has
0 advisories. `pkg:hackage/ghc@9.6.6` returns none of the three. Compiler
defects, the runtime system's included, are published against `ghc` in that
ecosystem, which no Hackage purl reaches. This is a gap in the compiler's
identity, not in a boot library's, and is out of this feature's scope.

## The two version tables

Comparing `ghc_boot_libraries.tsv` (milestone 966) with
`ghc_boot_dependencies.tsv` (spec 1653), keyed on (release, library):

- milestone 966: 1,407 rows; spec 1653: 1,371 rows; shared: 1,371.
- Only in 966: 36 rows, all `Win32` (Windows-only, excluded from 1653's table).
- Only in 1653: none.
- Disagreements: 9 rows, all `rts` on 9.0.1 (`1.0`) and 9.0.2 and 9.2.2–9.2.8
  (`1.0.2`). In each, 1653's version is empty, because the `package-data.mk`
  layout those releases use records no `rts` version. No row has two
  different non-empty versions.

## Orphans

No boot library in any of the five closure goldens has a
`waybill:orphan-reason` property (moat 36, openai 40, shellcheck 39,
slack-web 40, slacklinker 39 boot libraries).
