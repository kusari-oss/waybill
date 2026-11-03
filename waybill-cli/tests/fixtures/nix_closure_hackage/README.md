# Fixtures: closure members that are known Hackage packages (milestone 1275)

Two scan roots, both pinning nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`
in `flake.lock` — the revision the suite seeds into a temporary
`WAYBILL_NIXPKGS_CACHE`. **No test reads the host's
`~/.cache/waybill/nixpkgs/`** (Constitution Principle VII).

| directory | why it exists |
|---|---|
| `project/` | has a `.cabal` with a declared dependency, so `haskell_packages::enrich` engages and hands its parsed package set to the identity pass. The common path. |
| `closure_only/` | no `.cabal`. `is_enrichable` finds nothing, so `enrich` returns `None` before loading anything — the case research R2 says must still work, and the one the pass would skip if it inherited that gate. |

Every name is `waybill-fixture-*`. Real coordinates in fixtures trip Kusari
Inspector's advisory scan (#285, #640), so the collision case — which is
`bash 5.3p9` against Hackage's `bash 0.1.8` in the real world — is a synthetic
pair here.

The closure JSON, the seeded package set and the compiler configuration are
consts in `waybill-cli/tests/nix_closure_hackage_identity.rs`, beside the
assertions that read them, matching `nix_config_closure.rs` and
`nix_haskell_resolution_m926.rs`.
