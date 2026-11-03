# Quickstart: a package the compiler ships is never scoped as absent

**Milestone**: 1276

## Reproduce the defect

```sh
cargo build -p waybill --bin waybill
WAYBILL=./target/debug/waybill \
  bash specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh
```

`slack-web`, offline against the nixpkgs revision its own `flake.lock` pins
(`5ae3b07d…`, already in the local per-revision cache). Expect:

```
  reachable ONLY through the marked pair:  11
    the marked components themselves:  2  [butcher, monad-loops]
    non-boot fan-out below them:       8  [clock, deque, extra, microlens,
                                           microlens-th, multistate,
                                           strict-list, unsafe]
    compiler-supplied (boot):          1  [pretty]
```

`pretty` is the defect. The 8 are the control: they must still be non-runtime
afterwards.

## The corpus population

```sh
python3 -I specs/1276-boot-library-scope/measurements/scope-always-present.py .
```

```
boot libraries corpus-wide:  78, of which 10 carry a non-runtime scope
  haskell-security-advisories: 8     nix-closure-moat: 2
os-package:  142 components, every one no-scope
file-tier:   175 components, every one no-scope
```

The last two lines are the positive control for #1340's open question, and
the reason it is answered rather than deferred: those classes never carry a
scope, so this rule takes no position on them. A bare zero could not
distinguish "never mis-scoped" from "never examined".

## The four things to hold on to

1. **The schema settles the semantics.** #1340 presents two readings as
   equally defensible. CycloneDX 1.6's own `meta:enum` does not: `optional`
   means "not **installed** or otherwise accessible", and says an installed
   component prohibited from being called "must be scoped as `required`".
   Read it in `waybill-cli/tests/fixtures/schemas/cyclonedx-1.6.json` before
   reopening the question.

2. **I3 demands the marker this feature removes.** All 10 affected components
   become I3 violations the moment they are unmarked, I3 has zero
   declarations, and an undeclared violation **fails the lane**. The fix and
   I3's amendment are one change. Declaring the 10 to get a green
   intermediate is forbidden — that file is for defects awaiting a fix.

3. **Over-exempting is the same defect pointing the other way.** The control
   is 569 of the corpus's 579 non-runtime scopes surviving, and the 8
   components above.

4. **The narrowing already exists.** `candidate_series` scans `flake.nix` and
   returns only the series it names. `slack-web`'s cache holds only
   `configuration-ghc-9.10.x.nix`, which is the evidence. FR-004a needs a
   test, not an implementation.

## Running the suites

**`resolve` and `scan_fs` unit tests live in the BIN target**, not the lib:
`cargo test -p waybill --lib <filter>` reports `0 passed; … filtered out` for
a test that exists and runs fine under `--bin waybill`.

```sh
cargo test -p waybill --bin waybill scope_propagation
cargo test -p waybill --test public_corpus            # the invariant harness
cargo test -p waybill --test nix_closure_hackage_identity   # t057
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189). Never run `cargo fmt`.

## Fixtures

Unit and integration fixtures use `waybill-fixture-*` names only — real
coordinates in fixtures trip the advisory scan (#285, #640). Every test seeds
a `tempfile::tempdir()` and points `WAYBILL_NIXPKGS_CACHE` at it; **no test
reads the host's `~/.cache/waybill/nixpkgs/`** (Principle VII).

The measurement probes above deliberately use real coordinates, as the
project's convention allows, because the subject of the measurement is a
specific dependency graph.

Milestone 1275's `optional_scope` fixture
(`waybill-cli/tests/fixtures/nix_closure_hackage/optional_scope/`) is the
deterministic reproduction of a boot library inheriting a scope through
closure-derived edges. `t057` drives it and currently asserts the defect;
FR-010 requires inverting it rather than deleting it.
