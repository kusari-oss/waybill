# `nix_ifd` — nix's own stderr, captured verbatim

Milestone 1274 (#1283 part 1).

These are **not** flakes. They are the real stderr of two real `nix eval`
invocations, captured once and committed so the classifier can be tested
against what nix actually says rather than against a hand-written
approximation of it.

| file | what produced it | why it is here |
|---|---|---|
| `cabal2nix-refusal.stderr` | a flake whose default package is `haskellPackages.callCabal2nix "waybill-fixture-ifd" ./. { }`, evaluated with `--option allow-import-from-derivation false` | the subject. 29 lines; nix prints a git warning, a bare `error:` opener, 24 lines of stack trace, and the explanatory clause **last** |
| `nixconfig-unrelated.stderr` | a flake whose `nixConfig` requests `allow-import-from-derivation` and whose default package is a missing attribute | the **false positive**. Its stderr names the setting — in a *warning* — and then fails for an unrelated cause. A classifier keyed on the bare name reports a missing attribute as a policy refusal |
| `ifd-build-failed.stderr` | `import (derivation { … builder = "/bin/sh"; args = [ "-c" "…; exit 1" ]; })`, evaluated with `--option allow-import-from-derivation true` (Determinate Nix 3.20.0 / nix 2.34.6) | milestone 1283: IFD **allowed**, and the derivation built during evaluation failed. Distinct from the refusal: `error: Cannot build '….drv'.` then `Reason: builder failed …`. Captured by `specs/1283-nix-allow-ifd/measurements/ifd-eval/build-failed.sh`; copied verbatim from `build-failed.err` there |

Absolute temp paths are rewritten to `/FIXTURE` so the files are
host-independent. Nothing else is edited: the indentation, the blank lines
and nix's exact wording are what the selection rule and the classifier are
being tested against.

## Why not a committed flake

A real `callCabal2nix` flake needs the pinned nixpkgs fetched (300 MB+) and a
GHC. `nix_config_closure.rs` keeps its `real_nix_*` fixtures input-free to
avoid exactly that, so a flake here would be the first nix fixture in the
suite requiring network.

## Keeping them honest

A captured stderr goes stale when nix changes its wording, and a stale
capture would let the classifier pass against a message nix no longer emits.
The guard is the committed probe at
`specs/1274-nix-degradation-reason/measurements/reproduce-1283.sh`, which
runs real `nix` and exits non-zero if the refusal it depends on is not
reproduced. Re-capture with:

```sh
./specs/1274-nix-degradation-reason/measurements/reproduce-1283.sh --raw-stderr
```

which prints each fixture's line indices, predicate matches and the byte
offset of the clause.
