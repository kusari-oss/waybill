# Spec 1663 measurements

Re-run with `measurements/measure.sh <binary> <label> <work-dir>`. It uses the harness's flags, empty `HOME` and `WAYBILL_NIXPKGS_CACHE` directories, and each target fetched at its corpus pin and checked. The nix-closure targets are scanned only if they contain a Stack file; none does.

## Baseline: `main` at `03b8b525` (waybill 0.18.0, release build)

| target | `pkg:hackage` | snapshot components | lock parse warnings |
|---|---|---|---|
| haskell-aeson | 61 | 0 | 0 |
| haskell-security-advisories | 59 | 0 | 0 |
| haskell-language-server | 128 | 2 | 0 |
| hpack-hs-opentelemetry, and its variant | 127 | 2 | 1 |
| hpack-lootbox, and its variant | 62 | 1 | 1 |
| hpack-hs-temporal-sdk | 70 | 0 | 0 |
| hpack-ogmios, and its variant | 102 | 0 | 0 |
| nix-closure-moat, -slack-web, -openai, -slacklinker | no Stack files | | |

Snapshot components (source files, reachability, orphan reason):

| target | component | source files | reachable | orphan reason |
|---|---|---|---|---|
| haskell-language-server | `pkg:generic/ghc-9.6.5@unspecified` | `[""]` | no | `build-toolchain` |
| haskell-language-server | `pkg:generic/stackage-lts-23.28@unspecified` | `[""]` | no | `build-toolchain` |
| hpack-hs-opentelemetry | `pkg:generic/stackage-lts-24.35@unspecified` | `[""]` | no | `build-toolchain` |
| hpack-hs-opentelemetry | `pkg:generic/unknown@93a10755…` | `[""]` | no | **none** (I7) |
| hpack-lootbox | `pkg:generic/snapshot.yaml@unspecified` | `[""]` | no | `build-toolchain` |

The parse warnings are research R1, `invalid type: string "lts-24.35"` and `"lts-19.21"`. In each case the reader fell back to `stack.yaml`:
- hs-opentelemetry's `lts-24.35@unspecified` comes from `stack.yaml`.
- lootbox's `stack.yaml` says `resolver: snapshot.yaml`, a custom snapshot built on `lts-19.21`, which gives `snapshot.yaml@unspecified`. The rejected lock records `lts-19.21` with sha256 `fe8926aa…`.

**Expected after the change** (a prediction, checked in T013):
- hs-opentelemetry: `stackage-lts-24.35@6406d803…` from the root lock; the yesod snapshot `stackage-lts-18.8@93a10755…` with `build-toolchain`; lock-pinned Hackage versions.
- lootbox: `stackage-lts-19.21@fe8926aa…` in place of `snapshot.yaml@unspecified`, and lock-pinned Hackage versions.
- haskell-language-server: both snapshots gain `stack.yaml` as a source file.
- No snapshot without a reason, and 0 parse warnings.

## First branch measurement, superseded

The first branch build read the string-form lock files, and the corpus run on #1737 failed I7 on four targets: 6 components on hs-opentelemetry and 16 on lootbox, in both cases plus the no-generated-cabal variant. My measurement had listed the added components but not checked their reachability.

The cause was two defects the unread lock files had been hiding:
1. **A lock's pins reached only packages in its own directory** (`governing_pins`, `haskell.rs`). Both repositories keep their packages in subdirectories under a root lock. Each pin therefore produced a versionless `pkg:hackage/co-log`, which the edges reached, *and* an unreachable `co-log@0.6.1.2`.
2. **No orphan reason fit a lock pin that no declared package names.** A Stack lock records no edges, so these are almost certainly transitive dependencies of something the scan can't see.

Decisions (maintainer): (1) a `stack.yaml.lock` governs the packages its sibling `stack.yaml` lists; (2) a new orphan reason, `edgeless-lock-pin`.

## After: branch, release build

`unreachable_unexplained` counts unreachable components with no orphan reason. Every target's single entry is its own root component, `pkg:generic/<target>@<pin>`, which I7 excludes.

| target | `pkg:hackage` main → branch | unexplained (excl. root) main → branch | lock parse warnings |
|---|---|---|---|
| haskell-aeson, security-advisories, hs-temporal-sdk, ogmios (and its variant) | unchanged | 0 → 0 | 0 → 0 |
| haskell-language-server | 128 → 128 | 0 → 0 | 0 → 0 |
| hpack-hs-opentelemetry, and its variant | 127 → 128 | 1 (`unknown@93a10755…`) → 0 | 1 → 0 |
| hpack-lootbox, and its variant | 62 → 73 | 0 → 0 | 1 → 0 |

**Hackage changes:**
- **hs-opentelemetry:** `co-log`, `proto-lens`, `proto-lens-runtime`, `thread-utils-context` and `unix-compat` become their lock-pinned versions, one component each. `thread-utils-finalizers@0.1.1.0`, pinned but named by no manifest, is added as `edgeless-lock-pin`.
- **lootbox:** `co-log`, `co-log-core`, `co-log-sys`, `componentm` and `serialise` become their pinned versions. 11 pins no manifest names are added as `edgeless-lock-pin`: `bytebuild`, `byteslice`, `bytesmith`, `cborg`, `chronos`, `contiguous`, `primitive`, `run-st`, `teardown`, `typerep-map` and `zigzag`.
- Every added version is the lock's pin, and nothing else is removed.

**Snapshots:**
- hs-opentelemetry: `unknown@93a10755…` → `stackage-lts-18.8@93a10755…`, from `instrumentation/yesod/stack.yaml.lock`, `build-toolchain`. `stackage-lts-24.35@unspecified` → `@6406d803…`, from `stack.yaml.lock`.
- lootbox: `snapshot.yaml@unspecified` → `stackage-lts-19.21@fe8926aa…`, from `stack.yaml.lock`.
- haskell-language-server: `stackage-lts-23.28` gains `stack.yaml`; `ghc-9.6.5` gains `test/wrapper/testdata/stack-with-dist-newstyle/stack.yaml`. That one comes from test data and was emitted before too; recording the file makes it visible.

Unexplained differences: 0.

## Corpus goldens (T015)

Run 45900795766 regenerated the goldens on `46baca62` and passed. It reports 16 golden files changed:
- **15 are the expected ones:** haskell-language-server, and hpack-hs-opentelemetry and hpack-lootbox with their variants, each in three formats.
- **Content checks out:** for the hpack targets, the sorted `purl` differences match the table above exactly. haskell-language-server's components are unchanged; only its snapshots' source files changed.
- **The 16th, `hpack-hs-temporal-sdk/spdx-3.json`, was not committed.** It adds one `Organization "Microsoft"` that nothing references. That target has no Stack files, and neither binary produces it locally (#1738).
