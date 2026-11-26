# Data model: spec 1663

- **SnapshotId**: a `Resolver(name)` (`lts-18.8`, `nightly-2024-05-07`, `ghc-9.6.5`), or a `Url(url)` that matched no stackage layout.
- **StackSnapshot**: `id`, `sha256` (optional), and `source`, the lock file or `stack.yaml` that named it.
- **Snapshot component**: one per PURL.
  - Resolver: `pkg:generic/stackage-<name>@<sha|unspecified>`, or `pkg:generic/ghc-…`.
  - URL: `pkg:generic/stack-snapshot@<sha|unspecified>?download_url=<url>`.
  - `source_path` is the first source; `extra_source_paths` holds the rest, sorted.
  - `waybill:source-type = hackage-snapshot`.
  - When unreachable, the orphan reason is `build-toolchain`.
- **Measurement row**: target, revision (`main` or branch), snapshot PURL, source files, reachable, orphan reason, and lock parse warnings.
