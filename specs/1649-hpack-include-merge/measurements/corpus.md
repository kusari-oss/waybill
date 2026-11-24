# T019: corpus goldens, regenerated in CI

- **Regeneration dispatch:** `public-corpus.yml -f branch=1649-hpack-include-merge -f regen_goldens=true`, run 45298296496.
- **Scanned commit:** the log names `111b45143df2f742235b13e23add0737a17a8437`.
- **Result:** success. Corpus suite `ok. 174 passed; 0 failed`, plus a 3-test suite.

## What changed

Of 87 regenerated goldens, 63 are byte-identical to the committed ones and 24 differ:

- **21 `hpack-*` goldens**, all 7 targets in 3 formats.
- **3 `nix-closure-slacklinker` goldens.** slacklinker's `package.yaml` declares `HUnit`, which `main` minted as `pkg:hackage/hunit`, a name that resolves to nothing. With FR-009 the declaration keeps `HUnit` and merges into the closure tier's `pkg:hackage/HUnit@1.6.2.0`. Everything else follows from that one identity:
  - one fewer component (2948 → 2947);
  - nixpkgs resolution `resolved` 64 → 65 and `absent-from-package-set` 1 → 0;
  - one fewer transitive and one fewer `addressable_without_cpe`.

  No other difference: the hackage reference sets in both SPDX documents differ by exactly `pkg:hackage/hunit`.

All 24 were installed.

## The hpack goldens match T018's local measurement

```
hpack-hs-opentelemetry                     old      24     8     0    16     0     0      127
hpack-hs-opentelemetry                     new      24    24     0     0    16     0      127
hpack-hs-opentelemetry-no-generated-cabal  old      24     8     0    16     0     0       89
hpack-hs-opentelemetry-no-generated-cabal  new      24    24     0     0    16     0      127
hpack-hs-temporal-sdk                      old       7     7     6     0     0     0       73
hpack-hs-temporal-sdk                      new       7     7     0     0     6     0       73
hpack-lootbox                              old       7     1     0     6     0     0       62
hpack-lootbox                              new       7     7     0     0    11     0       62
hpack-lootbox-no-generated-cabal           old       7     1     0     6     0     0        8
hpack-lootbox-no-generated-cabal           new       7     7     0     0    11     0       62
hpack-ogmios                               old       8     0     0     8     0     0      102
hpack-ogmios                               new       8     8     0     0    16     0      103
hpack-ogmios-no-generated-cabal            old       8     0     0     8     0     0        0
hpack-ogmios-no-generated-cabal            new       8     8     0     0    16     0      103
```

Every count is the one `corpus-counts.md` measured locally, CI's 49 resolved includes among them.

The identity changes on the committed targets are exactly `corpus-counts.md`'s traced list:
- hs-temporal-sdk: `hspec-discover` and the 6 versions;
- ogmios: `json-via-show` (#1685).

hs-opentelemetry and lootbox have none.

Within each repository, the committed and no-generated-cabal goldens now hold the same components. They differ only in the root component's name, which is the target's own.

## Invariants

The run's invariant checks passed. The hpack targets' existing declarations, I2 on ogmios (#1208, the TypeScript `package.json` files) and I7 on hs-opentelemetry (#1678, a URL-pinned Stack snapshot), concern files this spec does not read, and still hold. No new failure appeared, and none went stale.
