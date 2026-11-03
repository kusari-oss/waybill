# Milestone 1274 — the pre-change baseline (T003 / FR-016)

Captured 2026-11-03T07:34:04Z on `arm64-darwin`.
nix: `nix (Determinate Nix 3.20.0) 2.34.6`

FR-016 requires each corrected assertion be justified against a measurement.
A baseline taken after the change has already lost the comparison.

## R4 — the corpus blast radius, measured before touching anything

```
$ grep -rl "nix-closure-degraded" waybill-cli/tests/fixtures/public_corpus/

(no output — no committed golden carries C190)
```

So the real FR-016 population is the six assertions in
`waybill-cli/tests/nix_config_closure.rs`, not goldens.

## The probe, before the fix

```
waybill: waybill 0.14.0   system: aarch64-darwin   nixpkgs: a799d3e3

════ what nix itself says about the cabal2nix flake ════
  nix: ...cannot build during evaluation because the option 'allow-import-from-derivation' is disabled

════ control: a handwritten flake (must NOT degrade) ════
  log: nix-closure: classified attribute=default derivations=1170 roles={
  log detail: (none)
  document: waybill:nix-closure-degraded = <absent>
  components: 304

════ subject: a callCabal2nix flake (degrades by policy) ════
  log: nix-closure: degrading; the manifest-derived set is unaffected reason=
  log detail: detail=`nix eval` failed: warning: Git tree '/private/var/folders/dz/l36x69hs28l50_whrqsz2dvh0000gn/T/tmp.lHHzzfbSms/cab
  document: waybill:nix-closure-degraded = evaluation-failed
  components: 2

════ false positive: nixConfig names the setting, fails elsewhere ════
  log: nix-closure: degrading; the manifest-derived set is unaffected reason=
  log detail: detail=`nix eval` failed: warning: Git tree '/private/var/folders/dz/l36x69hs28l50_whrqsz2dvh0000gn/T/tmp.lHHzzfbSms/nix
  document: waybill:nix-closure-degraded = evaluation-failed
  components: 0

════ the finding ════
  The refusal is deliberate (#1034, #1114). What an operator can read is
  only `evaluation-failed`, and the log's detail is whichever line came
  first -- a git warning -- rather than the line that explains it. A
  policy refusal is therefore indistinguishable from a broken flake.

  The third fixture is the guard: its stderr names the setting in a
  WARNING and then fails on a missing attribute. It must keep the
  generic reason. A classifier that reports it as a refusal has made
  the output worse, not better.
```
