# Quickstart: exercising the `nix eval` tier

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)

## Prerequisites

- `nix` on `PATH` (any build; the tier detects the capability it needs rather
  than gating on a version — see contracts/nix-invocation.md §1).
- A checkout with a `flake.lock` pinning nixpkgs. The two corpus Haskell targets
  qualify.

## The happy path

```sh
waybill sbom scan --path <project> --nix-eval --output out.cdx.json
```

Then check the document says what it did:

```sh
jq '.metadata.properties[] | select(.name|startswith("waybill:nix-eval"))' out.cdx.json
```

Expect `waybill:nix-eval-tier` with a revision, a system, and counts.

## Verify it against the oracle (the SC-001 check)

The differential harness from #971 part B already exists:

```sh
cargo run -p xtask -- nix-oracle --sbom out.cdx.json
```

Expect **zero `Disagree` verdicts**. This is the check that matters, because it
compares waybill's output against `nix` itself rather than against waybill's own
expectations — every real defect in this area was found that way, and none was
found by a self-authored unit test.

## Exercise each degradation (the SC-006 check)

Each of the seven `DegradationReason` variants must be independently reachable.
In every case the scan **exits 0** and the output equals the flag-off output
plus a `waybill:nix-eval-degraded` value.

| Variant | How to provoke |
|---|---|
| `ToolAbsent` | run with `nix` removed from `PATH` |
| `ToolUnusable` | point `NIX_REMOTE` at a socket that does not exist |
| `IfdRefusalUnverified` | a `nix` that does not support the option — or a stub on `PATH` whose `config show` omits it |
| `RevisionUnfetchable` | a `flake.lock` pinning a revision that does not exist |
| `NoEvaluableAttribute` | scan haskell-language-server: its flake exposes no `default` package, only `docs` and devShells |
| `EvaluationFailed` | a `flake.nix` with a syntax error |
| `BudgetExceeded` | `--nix-eval-timeout-secs 1` against a project whose evaluation takes longer |

## Verify the safety property (the SC-004 check)

The import-from-derivation fixture is
[`measurements/probe-nix-behaviour.sh`](./measurements/probe-nix-behaviour.sh)
§R3a, promoted to `waybill-cli/tests/fixtures/nix_eval/`. Scanning it must
build **nothing**:

```sh
nix store gc --dry-run 2>/dev/null   # note the store state
waybill sbom scan --path tests/fixtures/nix_eval/ifd --nix-eval
# assert: no ifd-marker path in the store, and the tier degraded
```

**Assert on the store, not on the exit code.** A scan that never reached
evaluation also builds nothing, and its exit code is identical to one that
reached evaluation and correctly refused. Assert the positive signal too — that
the tier ran and reported `IfdRefusalUnverified` or completed — or the test
passes vacuously (memory: `feedback_a_passing_test_may_exercise_nothing`).

## Verify byte-identity when off (the SC-002 check)

```sh
waybill sbom scan --path <project> --output off.cdx.json
waybill sbom scan --path <project> --nix-eval --output on.cdx.json
diff <(jq -S . off.cdx.json) <(jq -S . on.cdx.json)
```

With the flag off the run must be byte-identical to pre-feature output, and no
`nix` process may be started at all.

## Pre-PR

```sh
./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"
```

Read `$?` from the script itself, never from a pipeline — a pipeline reports its
last command's status, and a successful `grep` over the output of a script that
died at clippy reports 0. Then assert the **positive** signals in the log: the
`>>> all pre-PR checks passed.` line and the per-target
`test result: ok. N passed; 0 failed` lines.
