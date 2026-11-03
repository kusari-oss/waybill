# Quickstart: closure members that are known Hackage packages

**Milestone**: 1275

## Reproduce the defect

```sh
cargo build -p waybill --bin waybill
python3 -I specs/1275-closure-hackage-identity/measurements/size-closure-members.py \
    <cdx.json> <closure.json> ~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix
```

Producing those two inputs, on any Nix-built Haskell project with a
`flake.lock`:

```sh
waybill sbom scan --path <project> --no-deep-hash --nix-closure \
    --format cyclonedx-json --output cyclonedx-json=cdx.json
(cd <project> && nix --extra-experimental-features 'nix-command flakes' \
    --option allow-import-from-derivation false \
    derivation show -r '.#default' > closure.json)
```

Measured at `001fa50` on a flake depending on `aeson` and `text`:

```
closure-appended pkg:generic components: 276

bucket        n   ghc-built  not-ghc  unknown
exact        60          60        0        0
name-only    26           0       26        0
absent      190           1      175       14

the two signals agree on 60 of 60 exact-pair rows
```

## The three things to hold on to

1. **The obvious fix has already been measured wrong.** Moving the closure
   merge before the Haskell enrichment downgraded 41 PURLs from `pkg:hackage`
   to `pkg:generic` (m1035, #1040). The comment is at the merge site. Read it
   before reaching for the reorder.

2. **A name match alone is a defect, not a shortcut.** 26 of 276 components
   have a name in the Hackage set and nothing to do with Haskell. `bash`,
   `curl`, `ghc` and `brotli` all have real Hackage packages. The negative
   test (C5) is the one that matters most.

3. **Edges bind by name, not by the set's version.** 165 of 278 targets are
   present at a version the set does not declare, because the closure carries
   what GHC shipped. Binding to the set's version points 59% of edges at
   nothing.

## Running the suites

**`scan_fs` unit tests live in the BIN target, not the lib.** `scan_fs` is
declared in `main.rs`, so `cargo test -p waybill --lib <filter>` reports
`0 passed; … 218 filtered out` for a test that exists and runs perfectly well
under `--bin waybill`. The filtered-out count looks like a filter typo rather
than the wrong target, which is why this note is here.

```sh
cargo test -p waybill --test nix_closure_hackage_identity
cargo test -p waybill --bin waybill closure_identity   # the 15 unit tests
cargo test -p waybill --test nix_config_closure        # must stay 14/14
cargo test -p waybill --test nix_haskell_resolution_m926
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189).

## The two committed probes

Both are `#[ignore]`d and env-gated, because they read a real 16 MB package
set and a real 6 MB closure, which no ordinary test may do (Principle VII).
They are instruments, not behaviour tests: FR-006 requires the signals'
agreement be *measured*, and a disagreement is a finding to explain rather
than a test failure.

```sh
# T005 — what a second parse of the package set costs (findings F2).
WAYBILL_T005_PACKAGE_SET=~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix \
  cargo test -p waybill --bin waybill m1275_t005 -- --ignored --nocapture

# T009 — FR-006's agreement, measured against THIS code rather than a Python
# re-implementation of its rules (findings F5).
WAYBILL_T009_CLOSURE=/path/to/closure.json \
WAYBILL_T009_PACKAGE_SET=~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix \
  cargo test -p waybill --bin waybill m1275_t009 -- --ignored --nocapture
```

`measurements/size-closure-members.py` reads the emitted document and reports
the **276 closure-appended** members; the T009 probe reads the closure and
reports the **323 the tier emits**. The 47-member difference is the components
that merge into design-tier twins, and every bucket reconciles once that is
accounted for — see findings F5 before concluding the two disagree.

## Fixtures

Tests drive the closure tier through the stub `nix` that
`nix_config_closure.rs` established, with a seeded package-set cache the way
`nix_haskell_resolution_m926.rs` does (`WAYBILL_NIXPKGS_CACHE` pointed at a
`tempfile::tempdir()`, synthetic `waybill-fixture-*` names only). **No test
reads the host's `~/.cache/waybill/nixpkgs/`** — Principle VII.

The collision case needs a synthetic fixture of its own: a package-set entry
whose name matches a non-Haskell closure member at a different version. That
is `bash` in the real world; in the fixture it is a `waybill-fixture-*` pair,
because real coordinates in fixtures trip the advisory scan (#285, #640).
