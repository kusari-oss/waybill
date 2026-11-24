# Spec 1652 baseline (FR-001)

**Every figure here was executed.** Binary: `main` `317fbae` (the branch carries spec commits only). Every scan is `--offline`, with a `HOME` isolated by `scan.sh`. Predictions are labelled as predictions and are kept out of the data tables.

**Reproducing:**

```sh
./make-fixtures.sh /tmp/fx
for f in /tmp/fx/*; do
  for m in all root-only strict; do
    WAYBILL=target/debug/waybill SCAN_HOME=/tmp/h ./scan.sh "$f" "$m" "/tmp/out/$(basename "$f").$m.cdx.json"
  done
done
python3 -I -B count.py /tmp/out
```

**Reading the tables:**
- The "log" column is the milestone-220 INFO line. `FALLBACK` means the run printed `found zero root-level manifests; falling back to full-scope emission`, which is the #1045 path. On that path no C140 is emitted.
- Main modules are counted in `components[]`, recursively, with "nested" meaning `waybill:workspace-member` is other than `["."]`.

## Synthetic fixtures (`make-fixtures.sh`)

| target | mode | components | main modules (nested) | hackage | completeness | log |
|---|---|---|---|---|---|---|
| f1-json-root | all | 6 | 3 (2) | 4 | partial | - |
| f1-json-root | root-only | 1 | 0 (0) | 0 | **complete** | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=2 |
| f1-json-root | strict | 1 | 0 (0) | 0 | **complete** | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=2 |
| f2-hpack-root-chain | all | 5 | 4 (3) | 5 | partial | - |
| f2-hpack-root-chain | root-only | 5 | 4 (3) | 5 | partial | **FALLBACK** |
| f2-hpack-root-chain | strict | 5 | 4 (3) | 5 | partial | **FALLBACK** |
| f3-cabal-project | all | 5 | 3 (3) | 5 | partial | - |
| f3-cabal-project | root-only | 5 | 3 (3) | 5 | partial | **FALLBACK** |
| f3-cabal-project | strict | 5 | 3 (3) | 5 | partial | **FALLBACK** |
| f4-cabal-project-json | all | 7 | 4 (3) | 5 | partial | - |
| f4-cabal-project-json | root-only | 1 | 0 (0) | 0 | **complete** | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=3 |
| f4-cabal-project-json | strict | 1 | 0 (0) | 0 | **complete** | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=3 |
| f5-stack | all | 6 | 3 (3) | 5 | partial | - |
| f5-stack | root-only | 6 | 3 (3) | 5 | partial | **FALLBACK** |
| f5-stack | strict | 6 | 3 (3) | 5 | partial | **FALLBACK** |
| f6-npm-path | all | 4 | 2 (1) | 0 | partial | - |
| f6-npm-path | root-only | 4 | 2 (1) | 0 | partial | root_main_modules=1 workspace_members_followed=1 nested_projects_ignored=**1** |
| f6-npm-path | strict | 4 | 2 (1) | 0 | partial | root_main_modules=1 workspace_members_followed=1 nested_projects_ignored=**1** |

**What the table shows:**
- **f1 and f4** reproduce #1676: every Haskell component is dropped, and the result reads `complete`. C200 still lists both manifests under `manifests-read`.
- **f2, f3 and f5** never filter at all. A root Haskell package is not recognised as root-level (R1), and a root `cabal.project` or `stack.yaml` is not a main module, so `root-only` and `strict` fall back to the full scan.
- **f6:** the root's edge to the nested npm package survives the filter, so the package is kept, yet the log reports it as ignored (R3).

## The one-variable experiment (R1)

`package.yaml` was added to `is_manifest_basename` in `generate/split.rs`. The binary was rebuilt and f2 scanned again. The file was then restored with `cp` and the binary rebuilt.

| mode | log | hackage components kept |
|---|---|---|
| root-only | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=3 | root, `a`, `c`, `base` (`d` dropped) |
| strict | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=3 | root, `a`, `c`, `base` (`d` dropped) |

**What this shows:**
- Once the root is recognised, the projection keeps `a` (a direct dependency) and `c` (a dependency through `a`), and drops the independent `d`. Reachability already crosses main modules, under both modes.
- The log says 3 nested projects were ignored when 1 was dropped.

## Corpus targets

**Sources:**
- Nine are local corpus caches at their manifest pins.
- `hpack-hs-opentelemetry`, `hpack-hs-temporal-sdk` and `hpack-ogmios` were fetched at their manifest pins (`25f5b2d`, `d24313e`, `435189a`).
- `hpack-lootbox-no-generated-cabal` is the harness's prepared tree.

| target | mode | components | main modules (nested) | hackage | completeness | log |
|---|---|---|---|---|---|---|
| go-cobra | root-only | 7 | 0 (0) | 0 | partial | root_main_modules=1 workspace_members_followed=2 nested_projects_ignored=0 |
| go-cobra | strict | 7 | 0 (0) | 0 | partial | root_main_modules=1 workspace_members_followed=2 nested_projects_ignored=0 |
| go-opentelemetry | all | 334 | 29 (28) | 0 | partial | - |
| go-opentelemetry | root-only | 14 | 4 (3) | 0 | partial | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=28 |
| go-opentelemetry | strict | 14 | 4 (3) | 0 | partial | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=28 |
| haskell-aeson | all / root-only / strict | 63 | 6 (5) | 61 | partial | root-only and strict: **FALLBACK** |
| haskell-language-server | all / root-only / strict | 167 | 6 (5) | 128 | partial | root-only and strict: **FALLBACK** |
| haskell-security-advisories | all / root-only / strict | 62 | 6 (6) | 59 | partial | root-only and strict: **FALLBACK** |
| hpack-hs-opentelemetry | all / root-only / strict | 155 | 37 (37) | 127 | partial | root-only and strict: **FALLBACK** |
| hpack-hs-temporal-sdk | all / root-only / strict | 445 | 8 (8) | 73 | partial | root-only and strict: **FALLBACK** |
| hpack-lootbox | all / root-only / strict | 63 | 7 (7) | 62 | partial | root-only and strict: **FALLBACK** |
| hpack-lootbox-no-generated-cabal | all / root-only / strict | 63 | 7 (7) | 62 | partial | root-only and strict: **FALLBACK** |
| hpack-ogmios | all / root-only / strict | 1149 | 13 (13) | 103 | partial | root-only and strict: **FALLBACK** |
| npm-express | all / root-only / strict | 369 | 0 (0) | 0 | complete | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=0 |
| rust-ripgrep | root-only | 68 | 10 (0) | 0 | complete | root_main_modules=10 workspace_members_followed=0 nested_projects_ignored=0 |
| rust-ripgrep | strict | 68 | 10 (0) | 0 | complete | root_main_modules=1 workspace_members_followed=0 nested_projects_ignored=**9** |

**What the table shows:**
- **All 8 Haskell targets fall back** under both modes, so their output is identical to `all`. Today `root-only` does nothing on a Haskell repository, and says so only in a WARN.
- **go-opentelemetry:** 3 nested main modules are kept through edges, while the log counts all 28 as ignored.
- **rust-ripgrep `strict`:** 9 workspace members are kept, through the root's edges, while the log counts them as ignored.

## Root declarations in the Haskell targets

Read from each tree's root:

| target | root package | root `cabal.project` `packages:` | root `stack.yaml` `packages:` |
|---|---|---|---|
| haskell-aeson | `aeson.cabal` | `.` | none |
| haskell-language-server | `haskell-language-server.cabal` | `./ ./shake-bench ./hls-graph ./ghcide ./hls-plugin-api ./hls-test-utils` | `. ./hls-graph ./ghcide/ ./hls-plugin-api ./hls-test-utils` |
| haskell-security-advisories | none | `code/*/*.cabal` | none |
| hpack-lootbox (both) | none | none | `code/base code/config code/crypto code/log code/network code/prelude code/demo` |
| hpack-hs-opentelemetry | none | leading-comma list: `api-types`, `, api`, `, sdk`, … | `- api-types`, `- api`, … |
| hpack-hs-temporal-sdk | none | `*/*.cabal` | none |
| hpack-ogmios | none | none at the root | none at the root |

**Prediction (not measured; the T-task after implementation measures it):**
- `hpack-ogmios` keeps falling back.
- The other seven stop falling back. Packages that are not declared, and that no root package reaches, are dropped and recorded.
