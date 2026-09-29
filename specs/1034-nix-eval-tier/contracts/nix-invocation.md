# Contract: how waybill invokes `nix`

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)

Every element of this contract traces to a probe in
[`../measurements/probe-nix-behaviour.sh`](../measurements/probe-nix-behaviour.sh).
Re-run it when the `nix` on the bench changes.

## 1. Pre-flight — verify the safety control is in effect

**Runs once per scan, before any evaluation.**

```
nix config show --option allow-import-from-derivation false
```

**Accept** iff stdout contains the exact line `allow-import-from-derivation = false`.
**Otherwise** degrade with `IfdRefusalUnverified` and evaluate nothing.

**Why this gate exists** (research R3, MEASURED): `nix` accepts an unknown
setting with a warning and **exit code 0**:

```
$ nix eval --expr '1' --impure --option definitely-not-a-real-option true
warning: unknown setting 'definitely-not-a-real-option'
1                                                          # rc=0
```

So a `nix` that does not support the option would take the flag, ignore it, and
evaluate with import-from-derivation **enabled** — while exit code and output
look exactly like success. Checking that the option *applied* rather than that
it was *accepted* is the whole point. This is the project's standing lesson:
*a check that did not run looks identical to a check that passed.*

**Do not** substitute a `nix --version` floor. Determinate Nix 3.20.0 reports as
`nix 2.34.6`; a version parser already has two numbering schemes to reconcile,
and forks and vendor builds add more.

## 2. Host system detection — the one impure call

```
nix eval --impure --raw --expr 'builtins.currentSystem'
```

Used **only** when the operator did not name a system. It evaluates a builtin
and nothing repository-controlled.

**Why it must be separate** (research R2, MEASURED): `builtins.currentSystem` is
unavailable in pure mode —

```
$ nix eval --raw --expr 'builtins.currentSystem'
error: attribute 'currentSystem' missing
```

— so folding system detection into the resolving evaluation would force that
evaluation impure. Keeping it separate is what lets step 3 stay pure.

## 2a. What is evaluated, and what is not

The tier evaluates **nixpkgs** at the revision the project's `flake.lock` pins.
It does **not** evaluate the project's own `flake.nix`. The attribute path is
`(builtins.getFlake "github:NixOS/nixpkgs/<rev>").legacyPackages.<system>`,
with the repository name fixed in the expression and only the revision taken
from the lockfile.

Two consequences the rest of this contract depends on:

- Repository-authored Nix expressions do not run. The import-from-derivation
  refusal is not protecting against them today; it protects evaluation of
  nixpkgs, which waybill neither authors nor audits, and it is what makes
  project-flake evaluation (research §R8) an additive step later rather than a
  new risk.
- The revision is the one repository-controlled value that reaches a Nix
  expression, so §3 validates it.

## 3. Resolution — pure, IFD-refused, budgeted

```
nix eval --json \
         --option allow-import-from-derivation false \
         --expr '<expression pinned to <revision> and <system>>'
```

**No `--impure`.** MEASURED: a pinned 40-character revision evaluates in pure
mode, with IFD refused, in 0.49–0.64 s warm:

```
$ nix eval --json --option allow-import-from-derivation false \
    --expr 'let p = (builtins.getFlake "github:NixOS/nixpkgs/cbb5cf35…").legacyPackages.aarch64-darwin;
            in p.haskellPackages.aeson.version'
"2.2.4.1"
```

**The expression MUST**:
- pin the revision to the 40-character SHA from the project's `flake.lock`;
- name the system as a literal (never `builtins.currentSystem` — see §2);
- wrap per-attribute lookups in `builtins.tryEval … or null`, so one broken
  attribute cannot abort the whole evaluation. The existing oracle already does
  this (`xtask/src/nix_oracle/mod.rs:161-169`) and that part is worth copying.

**The expression MUST NOT** be built by string-concatenating unvalidated values
into Nix source. Two enter it, and both are checked rather than escaped:

- **Component names** — refused unless `[A-Za-z0-9._-]`, which is Hackage's own
  rule, so nothing legitimate is lost and there is no escaping bug to get wrong.
- **The revision** — refused unless 40 lowercase hex characters. Measured: a
  crafted `rev` in `flake.lock` reaches the expression builder. It is not
  exploitable today only because the package-set fetch for the same revision
  runs first and fails on a non-revision, which is defence by accident resting
  on an unrelated call. This check makes it defence by design.

## 4. Budget

A wall-clock budget around the step-3 subprocess, using the existing pattern —
`std::process::Command` + `std::thread` + `std::sync::mpsc::recv_timeout`, as at
`waybill-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81` and
`golang/mod_why.rs:240`. On expiry: kill the child, degrade with
`BudgetExceeded`.

**Why waybill must do this** (research R4, MEASURED): `nix` bounds recursion but
not time.

```
$ nix config show | grep -E '^(max-call-depth|timeout) '
max-call-depth = 10000     # caught runaway recursion in 1s
timeout = 0                # the BUILD timeout — this tier never builds
```

A shallow, non-recursive evaluation ran **45 s and was still going** when the
probe killed it.

**One budget covers both the fetch and the evaluation.** Research R5 argued for
splitting them, on the grounds that a single budget makes the timeout partly a
bandwidth test — the same repository passes on a warm store and fails on a cold
one for reasons unrelated to the flake. That reasoning still holds, but the
split is not implementable as described: `getFlake` fetches *during* evaluation,
inside the one `nix eval` call, and there is no seam to bound separately short
of pre-fetching in an extra invocation.

The consequence is accepted rather than hidden: the default budget must be
generous enough to cover a cold-store fetch, and an operator whose first scan
degrades with `budget-exceeded` either raises it or warms the store. Splitting
it properly — a `nix flake prefetch` under its own budget, then a bounded
evaluation — is a reasonable follow-up if this tier's value is ever
demonstrated.

**Default values**: set by task T-R6 (project-flake evaluation cost) and T-R5
(cold-store acquisition cost). **Not quoted here** — no number describing
external behaviour enters this contract without a measurement behind it.

## 5. Parsing

`nix eval --json` output is parsed with `serde_json` into
`BTreeMap<String, Option<String>>`. A `null` means the attribute was absent or
`tryEval` caught it; it is **not** an error and **not** a version.

## 6. What this contract deliberately does not copy

`xtask/src/nix_oracle/mod.rs` (the #971 part B oracle) passes `--impure` at
lines 172 and 187 and does not disable IFD. That is acceptable for a
developer-run review tool pointed at known targets. It is **not** a template for
this tier, and the plan must not lift its invocation. Research R2 and R3 are the
reasons.
