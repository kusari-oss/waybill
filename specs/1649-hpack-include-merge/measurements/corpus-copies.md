# T003: corpus copies for measurement (local, public repositories)

Each repository was cloned at its #1677 pin into the session scratchpad: `git init`, `git fetch --depth 1 origin <sha>`, then `git checkout FETCH_HEAD`.

- **The committed copy** (`hpack/<repo>`) is the clone itself.
- **The no-generated-cabal copy** (`hpack/strip-<repo>`) is a `cp -R` with `.git` removed, and every `*.cabal` whose first four lines contain `hpack` deleted (#1677's rule).

| Repository | Pin | Commit | package.yaml | .cabal (committed) | .cabal (no-generated) |
|---|---|---|---|---|---|
| hs-opentelemetry | 25f5b2d6 | 25f5b2d65744af384e06f61040e5c037d7cb8db9 | 24 | 37 | 13 |
| lootbox | 2cd7b1e0 | 2cd7b1e0fde3888593404d1e1842d45e1b62bb6c | 7 | 7 | 0 |
| hs-temporal-sdk | d24313e3 | d24313e37a009cc4121012e509e16a3e749811d3 | 7 | 7 | 0 |
| ogmios | 435189ae | 435189aea2d9a047dc395a6f1cdef737d52f1cbe | 8 | 8 | 0 |
