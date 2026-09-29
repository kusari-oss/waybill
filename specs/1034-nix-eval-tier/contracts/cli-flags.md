# Contract: CLI flag surface

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)
**Command**: `waybill sbom scan` — `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs`.
Note `waybill-cli/src/cli/scan.rs` is a different struct of the same name,
belonging to `waybill trace`; these flags do not go there.

## Flags

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--nix-eval` | boolean | **off** | Enable the evaluation tier. |
| `--nix-eval-system <SYSTEM>` | string | the host's | Platform to evaluate for. Requires `--nix-eval`. |
| `--nix-eval-timeout-secs <N>` | integer | *set by task T-R6* | Wall-clock budget for evaluation. Requires `--nix-eval`. |

Shape follows `--gradle-resolve` (m235) and `--helm-render` (m203): one boolean
that turns the tier on, companions that are rejected without it.

## Deliberate departures from precedent

**No environment variable.** `--helm-render` has a `WAYBILL_HELM_RENDER=1`
equivalent; this feature has none. Issue **#1042** records that the CLI carries
146 long flags — 99 on `sbom scan` alone — with two mechanisms for several
capabilities and no rule for which applies. Adding a second mechanism here would
widen exactly what is under investigation. Spec Assumption A-2; overridable.

**No default-on.** Decided, though on narrower grounds than first written: the
tier evaluates nixpkgs, not the project's flake, so the original
"it runs the repo's code" argument does not apply to what was built. What
remains — an external `nix` dependency, host-dependent output, 300–335 MB of
store against 16 MB of fetches, and evaluation of unaudited nixpkgs code — is
still enough. Spec Out of Scope, and US3's scope correction.

## Help text

The `--nix-eval` help MUST state that enabling it runs `nix` and evaluates Nix
code, and MUST say precisely which: nixpkgs at the revision the project's
`flake.lock` pins, not the project's own flake (spec FR-003). This is the only
flag on `sbom scan` that makes waybill run an evaluator at all, and a reader
must neither have to infer that nor be left with an inflated idea of the
exposure.

## Precedence

1. Flag off → no `nix` process is started at all (spec FR-002). Not "started and
   ignored" — **not started**, which is what makes byte-identity testable.
2. Flag on, tier succeeds → evaluated versions win; file-parsed values retained
   as metadata (C177); origin marked on every touched component (C181).
3. Flag on, tier degrades → file-parsing output unchanged, reason in C180, exit
   status success.

## Validation

- `--nix-eval-system` without `--nix-eval`: argument error.
- `--nix-eval-timeout-secs` without `--nix-eval`: argument error.
- `--nix-eval-system` not matching `<arch>-<os>`: argument error at parse time.
- `--nix-eval-timeout-secs 0`: argument error. There is no "unbounded" setting,
  because research R4 established that nothing else would bound it.

Argument errors fail at parse time and are **not** degradations — the operator
made a mistake in the invocation, which is different from the environment being
unable to satisfy a valid one.
