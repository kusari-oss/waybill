Stub-`nix` fixture for issue #1444 (`tests/closure_overlapping_edges.rs`), served by
`tests/common/nix_stub.rs`. Never evaluated by a real nix.

- `project/` — a Cargo project. `Cargo.lock` records `app -> lib`, `app -> devlib` (a
  `[dev-dependencies]` entry) and `lib -> leaf`. `flake.lock` pins the revision the stub
  seeds an empty package set for.
- `closure.json` — the same four packages at the same `(name, version)`, with the same three
  edges in `inputs.drvs`. Each member merges into the Cargo component of that identity, so
  the closure's build-graph edges land on PURL pairs the Cargo reader already contributed.

Every name is `waybill-fixture-*`; the `Cargo.lock` checksums are placeholders.
