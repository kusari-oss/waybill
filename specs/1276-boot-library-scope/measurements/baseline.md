# Baseline: what a compiler-supplied package is scoped as today

**Task**: T001 | **Date**: 2026-11-03 | **Host**: aarch64-darwin
**Parent commit**: the tip of `1275-closure-hackage-identity` (this branch is
stacked on milestone 1275; see spec.md Assumptions)

## `slack-web`, offline against its own pinned revision

Revision `5ae3b07d8d6527c42f17c876e404993199144b6a`, in the local
per-revision cache. Reproduced with
`specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh`:

```
  hackage components:                        190
  reachable from the root:                   190
  reachable without traversing the marked:   179
  reachable ONLY through the marked:          11
    the marked components themselves:  2 ['butcher', 'monad-loops']
    non-boot fan-out below them:       8 ['clock', 'deque', 'extra',
                                          'microlens', 'microlens-th',
                                          'multistate', 'strict-list',
                                          'unsafe']
    compiler-supplied (boot):          1 ['pretty']
```

**`pretty` is the defect.** The **8** are the control: SC-003 requires them to
still carry a non-runtime scope afterwards, because the correction is confined
to packages the compiler supplies.

## The corpus

`measurements/scope-always-present.py`, over the 17 committed goldens:

```
components carrying optional/excluded, all classes: 579

class         scope              n
file-tier     no-scope         175
haskell-boot  excluded          10
haskell-boot  no-scope          68
os-package    no-scope         142

the defect population, by target:
  haskell-security-advisories    {'haskell-boot': 8}
  nix-closure-moat               {'haskell-boot': 2}
```

Every figure the spec quotes is reproduced. **SC-008 / C15 holds for both
probes.**

## What must change, and what must not

| | measured now | target |
|---|---|---|
| boot-library components with a non-runtime scope | **10** of 78 | **0** (SC-002) |
| `pretty` on `slack-web` | `excluded` | no scope (SC-001) |
| the 8 non-boot controls on `slack-web` | non-runtime | **still non-runtime** (SC-003) |
| non-runtime scopes corpus-wide | **579** | **569** — exactly 10 move (SC-004) |
| exempted components recording why | 0 | all of them, via C170 (SC-005) |
| documents recording an exemption count | 0 | all (SC-005) |

The 569 figure is the headline control. This feature's plausible wrong
implementation **over**-exempts, which would take 579 down by more than 10 and
report genuinely test-only components as runtime — the same defect being
fixed, pointing the other way.

## The two always-present classes that are not participants

142 OS-package and 175 file-tier components, **every one of them scope-free**.
Reported with that positive control because a bare zero cannot distinguish
"never mis-scoped" from "never examined", and #1340's open question turns on
exactly that distinction.

## Reproducing

```sh
cargo build -p waybill --bin waybill
WAYBILL=./target/debug/waybill \
  bash specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh
python3 -I specs/1276-boot-library-scope/measurements/scope-always-present.py .
```

The first clones `slack-web` and scans offline against the cached revision;
the second reads only committed goldens and needs no network.
