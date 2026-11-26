# Research: spec 1663

Measured on `main` at `03b8b525` (debug build). The corpus clones are at the pins in `manifest.rs`.

## R1. A resolver-pinned `stack.yaml.lock` is never read (added to scope)

Stack writes `original` as a plain string when a snapshot is pinned by name, and as a map when it is pinned by URL:

```yaml
- completed: {sha256: 6406d803…, size: 728959, url: …/lts/24/35.yaml}
  original: lts-24.35                      # by name
- completed: {…, url: …/lts/18/8.yaml}
  original: {url: …/lts/18/8.yaml}         # by URL
```

`StackSnapshotOriginal` is a struct with a `resolver` field, so the string form fails the typed deserialize. Scanning hs-opentelemetry logs:

```
haskell: failed to parse stack.yaml.lock; falling back per FR-009
path=…/stack.yaml.lock error=typed deserialize failed: invalid type: string "lts-24.35", expected struct StackSnapshotOriginal
```

The whole file is rejected, packages included. Of the `stack.yaml.lock` files discovery collects in the corpus clones:

| target | `stack.yaml.lock` discovered | parses | why |
|---|---|---|---|
| hpack-hs-opentelemetry | 2 (root, `instrumentation/yesod`) | yesod only | root has `original: lts-24.35` |
| hpack-lootbox | 1 | no | string `original` |
| haskell-language-server, hs-temporal-sdk, ogmios | 0 | — | |

Discovery collects only the file name `stack.yaml.lock`. hs-opentelemetry's six `stack-ghc-*.yaml.lock` files are alternative configurations, and are not read before or after this spec.

**Decision (maintainer):** fix this in spec 1663. `original` deserializes as a string (a resolver) or a map with `resolver` or `url`. FR-002, one identity for name and URL, cannot be shown on real lock files otherwise.

## R2. Today's snapshot identities, and why the URL one is unexplained

These are the snapshot components a scan emits. The `hpack-*` goldens keep only `pkg:hackage/` (`layer2_golden.rs:76`, #1677), so they never contain snapshots; the invariant layer sees the whole document.

| target | component | source files | linked | orphan reason |
|---|---|---|---|---|
| haskell-language-server | `pkg:generic/stackage-lts-23.28@unspecified` | `[""]` | no | `build-toolchain` |
| haskell-language-server | `pkg:generic/ghc-9.6.5@unspecified` | `[""]` | no | `build-toolchain` |
| hpack-hs-opentelemetry | `pkg:generic/stackage-lts-24.35@unspecified` (from `stack.yaml`, because the lock failed) | empty | no | `build-toolchain` |
| hpack-hs-opentelemetry | `pkg:generic/unknown@93a10755…` (yesod lock) | empty | no | **none** (I7) |

`build-toolchain` (`orphan_reason.rs`, mechanism E) requires all three of:
- `design` tier;
- empty source files;
- no workspace members.

A snapshot with no hash is `design`, but a lock-file snapshot carries its hash and is `source`, so the URL one fails the test. And FR-004, recording the lock file, would empty that rule for every snapshot.

**Decision:** a component whose `waybill:source-type` is `hackage-snapshot` (the type every snapshot and compiler pin already carries) is `build-toolchain` when unreachable, whatever its tier and source files. This is the existing reason for resolver-named snapshots, now given to URL-pinned ones as well (FR-005). Snapshots stay unlinked: a snapshot is a build input set, not a dependency of the project, which is what mechanism E already says.

## R3. The stackage-snapshots layout

Checked with `gh api repos/commercialhaskell/stackage-snapshots` (default branch `master`):
- `lts/18/` holds `0.yaml`, `1.yaml`, `10.yaml` and so on;
- `nightly/2024/` holds `1` … `12`;
- `nightly/2024/5/` holds `1.yaml`, `10.yaml` and so on.

So paths are **not** zero-padded, while Stack's resolver names are (`nightly-2026-04-04`, observed in `stack-ghc-9.12.yaml.lock` beside `…/nightly/2026/4/4.yaml`).

**Decision:** match the last path segments, independent of the host and branch:
- `…/lts/<M>/<m>.yaml` → `lts-<M>.<m>`;
- `…/nightly/<Y>/<M>/<D>.yaml` → `nightly-<Y>-<MM>-<DD>`, zero-padded.

Every part must be all-digits. Mirrors and other refs keep working, and a non-numeric part falls through to R4. The trade-off is that a non-stackage URL ending in `/lts/<M>/<m>.yaml` is named as that stackage snapshot. Its sha256 still identifies the exact content, so the wrong-name risk is limited to the label.

## R4. Any other URL

**Decision:** `pkg:generic/stack-snapshot@<sha256>?download_url=<url>`, or `@unspecified` when there's no hash. `download_url` is the purl-spec qualifier for generic packages. The sha256 keeps distinct snapshots distinct, the URL is kept, and nothing is ever `unknown`. `waybill:stackage-resolver` is absent for these.

## R5. Which field names the snapshot

1. `original` as a string: the resolver.
2. `original.resolver`: the resolver (an older map form; kept).
3. `original.url`: R3 or R4.
4. Neither: `completed.url` (R3 or R4).

`original` is what the project wrote, and `completed` is what Stack resolved. In every lock file measured, they name the same snapshot.

## R6. Source files and duplicates

- The snapshot records its lock file as `source_path`.
- When several lock files name the same snapshot, the first is `source_path` and the rest go in `extra_source_paths`, sorted and unique.
- Today a later duplicate is dropped by the `seen_purls` set; it is merged instead.
- The `stack.yaml` fallback already runs only when no sibling lock parsed (`haskell.rs:1033`). With R1, hs-opentelemetry's root gets `stackage-lts-24.35@6406d803…` from the lock in place of `@unspecified` from `stack.yaml`.

Lock-file *package* components also record an empty `source_path` (`haskell.rs:2407`). That is not in this spec's scope; filed as a side finding.
