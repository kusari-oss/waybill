# Milestone 1273 fixtures — hpack `package.yaml` reading

Every package name here is **synthetic** (`waybill-fixture-*`). Real
coordinates are never used in fixtures: they trip Kusari Inspector's advisory
scan, which bit PRs #285 and #640.

Real coordinates appear only in the measurement probes under
`specs/1273-haskell-nixpkgs-truthfulness/measurements/`, deliberately and
documented in each script's header, because a probe's subject *is* the
external behaviour of a specific real artifact.

## Cache isolation

No test in this milestone may read the host's `~/.cache/waybill/nixpkgs/`.
A test that does passes on a developer's machine and fails on CI, which has
no such cache, and Principle VII (Test Isolation) forbids it.

Tests needing a resolved nixpkgs revision seed a `tempfile::tempdir()` with a
synthetic package set and point `WAYBILL_NIXPKGS_CACHE` at it, per
`seed_cache()` in `waybill-cli/tests/nix_haskell_resolution_m926.rs`. For the
"no cache" case that file points the variable at `__no_cache__` rather than
unsetting it, because an unset value falls back to the real `$HOME`.

## Layout

One directory per behaviour, named after what it exercises rather than after
the requirement number, so a renumbered spec does not strand a fixture.
