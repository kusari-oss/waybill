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

## After: branch, release build

| target | `pkg:hackage` main → branch | snapshot change | lock parse warnings |
|---|---|---|---|
| haskell-aeson, haskell-security-advisories, hs-temporal-sdk, ogmios (and its variant) | unchanged | none | 0 → 0 |
| haskell-language-server | 128 → 128 | `stackage-lts-23.28` gains `stack.yaml`; `ghc-9.6.5` gains `test/wrapper/testdata/stack-with-dist-newstyle/stack.yaml` | 0 → 0 |
| hpack-hs-opentelemetry, and its variant | 127 → 133 | `unknown@93a10755…` (no reason) → `stackage-lts-18.8@93a10755…`, from `instrumentation/yesod/stack.yaml.lock`, `build-toolchain`. `stackage-lts-24.35@unspecified` (from `stack.yaml`) → `@6406d803…`, from `stack.yaml.lock`. | 1 → 0 |
| hpack-lootbox, and its variant | 62 → 78 | `snapshot.yaml@unspecified` → `stackage-lts-19.21@fe8926aa…`, from `stack.yaml.lock` | 1 → 0 |

**Every added Hackage component is pinned in the lock file that now parses:**
- hs-opentelemetry adds 6 (`co-log@0.6.1.2`, `proto-lens@0.7.1.6`, `proto-lens-runtime@0.7.0.7`, `thread-utils-context@0.4.1.0`, `thread-utils-finalizers@0.1.1.0`, `unix-compat@0.7.1`); all 6 are in the root `stack.yaml.lock`.
- lootbox adds 16; all 16 are in its `stack.yaml.lock`.
- No component is removed anywhere.

**Snapshots:**
- Every one is now named, with a source file and `build-toolchain`.
- None is named `unknown`.
- Unexplained differences: 0.

haskell-language-server's `ghc-9.6.5` comes from a `stack.yaml` in test data (`test/wrapper/testdata/…`). It was emitted before this spec too; recording the file makes that visible. Whether test-data Stack files should be read at all is outside this spec.
