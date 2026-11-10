Stub-`nix` fixtures for milestone 1377 (`tests/closure_subject.rs`, `tests/closure_declared_cpe.rs`),
served by `tests/common/nix_stub.rs`. Never evaluated by a real nix.

- `closure.json` — the root `waybill-fixture-app 2.1` is the one derivation no member consumes, so it is `graph.root`.
- `closure-mainmodule.json` — the root is `waybill-fixture-roleproj 0.1.0`, the same `(name, version)` as the cabal package in `closure_role_scope/project` (the moat shape).
- `closure-two-roots.json` — `closure.json` plus a second unconsumed derivation, so the closure does not say which one was evaluated.
- `closure-nixos.json` — the root is a NixOS toplevel: no `pname`, no `version`.
- `rows.json` — meta-evaluation rows carrying the 1377 fields: `cpe` present (app), `cpe` null (lib), both absent (tool).
