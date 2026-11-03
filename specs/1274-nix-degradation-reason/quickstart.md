# Quickstart: the nix-closure tier says why it degraded

**Milestone**: 1274 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Prerequisites

```sh
cargo build -p waybill --bin waybill
```

`nix` on `PATH`. The probe fetches the pinned nixpkgs on first run, so the
first invocation needs network; later ones use the store.

## Reproduce the defect

```sh
./specs/1274-nix-degradation-reason/measurements/reproduce-1283.sh
```

Builds two otherwise-identical flakes for one `.cabal` and scans both. While
the defect stands:

| | handwritten (control) | `callCabal2nix` |
|---|---|---|
| nix itself | evaluates | refuses, naming the setting |
| document reason | **absent** | `evaluation-failed` |
| log detail | — | `warning: Git tree '…' has uncommitted changes` |
| components | 304 | 2 |

**The control is load-bearing.** A `callCabal2nix` flake that degrades proves
nothing by itself — a broken fixture degrades too. The handwritten flake
classifying 1,170 derivations in the same run is what makes the subject's
degradation attributable to the refusal.

The probe **exits 1** if nix does not refuse, rather than carrying on to
report findings about something else.

## What to check after a change

```sh
# C2/C3 — the document names the refusal and carries the cause.
jq -r '.metadata.properties[] | select(.name=="waybill:nix-closure-degraded") | .value' out.cdx.json
```

Expected shape:

```json
{"reason":"import-from-derivation-required",
 "detail":"error: cannot build '…cabal2nix-….drv' during evaluation because the option 'allow-import-from-derivation' is disabled"}
```

Assert on the **clause**, not on "it isn't the git warning". Nix emits the
stack trace between the first error line and the cause, so a weaker assertion
passes on 1,155 characters of trace.

## The two traps this milestone exists because of

**1. The salient line is the LAST error line, not the first.** Nix prints the
trace first and the cause last. On the measured refusal:

```
[ 0] warning: Git tree '…' has uncommitted changes
[ 1] error:                                   <- what `position()` finds
[ 2..24] … stack trace, file positions, code excerpts …
[25]        error: cannot build '…' because the option '…' is disabled   <- the cause
```

`position` → `rposition` is the whole fix, and it is strictly better: with one
matching line the two agree, with a trace `rposition` picks the cause.

**2. The classifier must not key on the bare setting name.** Measured:

```sh
# A flake requesting the setting, failing for an unrelated reason:
nix --option allow-import-from-derivation false eval --raw '.#default.drvPath'
#   warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
#   error: attribute 'this-attribute-does-not-exist' missing
```

The setting's name is in stderr and the failure has nothing to do with it.
`slack-web` sets this option, so the false positive is ordinary. Match the
clause `because the option 'allow-import-from-derivation' is disabled`, on an
error line.

## Affected tests

```sh
cargo test -p waybill --test nix_config_closure
```

Six assertions across five tests read C190's bare string. They are
**corrected** to read `reason` — not pinned (FR-016) — and that suite is where
FR-013 is enforced, so the corrected assertions are what prove the existing
wire strings survived.

## Golden churn

**Expected: none.** No committed corpus golden carries C190; only a target
scanned with `--nix-closure` can, which is `nix-closure-moat` alone, and its
evaluation succeeds. Verify rather than assume:

```sh
grep -rl "nix-closure-degraded" waybill-cli/tests/fixtures/public_corpus/ || echo "none, as expected"
```

A golden that *does* change means something reached further than intended.

## The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189). Run `cargo clippy --workspace --all-targets
-- -D warnings` if checking clippy separately — **with** the flag, which is
what the gate runs; without it, dead-code and argument-count lints are
warnings you will not see.
