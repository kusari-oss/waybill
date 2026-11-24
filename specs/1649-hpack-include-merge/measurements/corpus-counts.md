# T018: corpus counts, `main` against the branch (FR-001)

Local scans of the copies in `corpus-copies.md`: `waybill --offline sbom scan --no-deep-hash`, CycloneDX, with `HOME` isolated. `main` is the `ea149065` binary (`control.md`). The branch is this spec with FR-009 and FR-010 (`probe_lowering.md`). Before scanning, the four no-generated-cabal copies had **0** hpack-generated `.cabal`.

`hackage` counts unique `pkg:hackage` PURLs across all components, nested ones included.

```
copy                     arm     seen  read  part  unrd  hackage
hs-opentelemetry         main      24     8     0    16      127
hs-opentelemetry         branch    24    24     0     0      127
strip-hs-opentelemetry   main      24     8     0    16       89
strip-hs-opentelemetry   branch    24    24     0     0      127
lootbox                  main       7     1     0     6       62
lootbox                  branch     7     7     0     0       62
strip-lootbox            main       7     1     0     6        8
strip-lootbox            branch     7     7     0     0       62
hs-temporal-sdk          main       7     7     6     0       73
hs-temporal-sdk          branch     7     7     0     0       73
strip-hs-temporal-sdk    main       7     7     6     0       73
strip-hs-temporal-sdk    branch     7     7     0     0       73
ogmios                   main       8     0     0     8      102
ogmios                   branch     8     8     0     0      103
strip-ogmios             main       8     0     0     8        0
strip-ogmios             branch     8     8     0     0      103
```

## Summary

- **Manifests.** All 46 are `read` on the branch. On `main`: 16 + 6 + 8 = 30 unread, and 6 partial in hs-temporal-sdk.
- **Includes.** 49 resolved across the four repositories (16 + 11 + 6 + 16), and none unresolved, matching research R2's 49 includes.
- **Committed and no-generated-cabal copies now agree.** Their hackage counts are equal in every repository: 127, 62, 73 and 103. On `main`, the no-generated-cabal copies lost 38, 54, 0 and 103 of them.
- **The largest alias count** in one manifest with its includes is 7 (lootbox `code/network/package.yaml`). T005's alias test uses 40.

## Every identity change on the committed copies, traced

| Repository | Change | Cause |
|---|---|---|
| hs-opentelemetry | none | the manifests now read give the same set the generated `.cabal` gave |
| lootbox | none | as above |
| hs-temporal-sdk | `hspec-discover:hspec-discover` → `hspec-discover` | FR-009: a qualified build tool names its package (#1684) |
| hs-temporal-sdk | 6 workspace packages gain `@2025.10.1.0` | `version:` comes from `<<: !include ../hpack-defaults.yaml`. On `main` the include was stripped, so the main modules were versionless. The generated `.cabal` agrees: `version: 2025.10.1.0` |
| ogmios | `+ pkg:hackage/json-via-show@2.0.0` | `server/cabal.project` does not list `modules/json-via-show`, and #1032 drops its `.cabal`. The hpack path never applies #1032, which predates this spec; the manifest only becomes reachable here. Filed as #1685 and left out of scope: the membership rule for a `package.yaml`-only directory needs its own decision, and SC-002's per-package agreement is unaffected |

The first branch measurement, before FR-009 and FR-010, had these differences instead, all now gone:
- 3 ogmios manifests `not-a-manifest` (`when:` as one mapping);
- `ouroboros-network:api`, `:framework` and `:protocols`, `ouroboros-consensus:cardano` and `:diffusion`, and `typed-protocols:cborg` and `:stateful` as package names;
- a `pkg:hackage/quickcheck` beside `QuickCheck` in hs-opentelemetry.

## Found and filed, outside this spec

- **#1685:** a `package.yaml` outside `cabal.project`'s `packages:` is emitted (above).
- **#1686:** main-module edges keep only one name of a case-differing pair, in both the `.cabal` and hpack readers. No corpus target declares such a pair; fixture L1 asserts the components instead.
