# Contract: Corpus Harness Invocation Surface

**Date**: 2026-07-14
**Purpose**: The stable surface that developers and CI use to invoke the corpus. Changes to any of the shapes below require a spec update.

## Invocation Surface

The corpus is invoked via `cargo test`:

```bash
MIKEBOM_RUN_PUBLIC_CORPUS=1 \
  cargo test --test public_corpus --release -- --nocapture
```

Any of the following forms are equivalent to a maintainer:

| Invocation | Behavior |
|---|---|
| `cargo test --test public_corpus` (no env) | Every corpus test skips with `println!("skipping: MIKEBOM_RUN_PUBLIC_CORPUS not set")`; exit 0. |
| `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus` | All targets run. Exit 0 on pass, non-zero on any failure. |
| `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus corpus_go_cobra` | Only the `corpus_go_cobra` target runs (cargo's `--filter` behavior). |
| `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1 MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus` | Layer 2 replaces the diff with a write; goldens regen in-place. |

## Environment Variables

| Var | Type | Default | Effect |
|---|---|---|---|
| `MIKEBOM_RUN_PUBLIC_CORPUS` | string | `""` | When set to `"1"`, gates all corpus tests to actually run. Any other value = skip. |
| `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS` | string | `""` | When `"1"`, Layer 2 writes golden files instead of comparing. Layer 1 still runs and must pass first. Ignored if `MIKEBOM_RUN_PUBLIC_CORPUS != "1"`. |
| `MIKEBOM_CORPUS_CACHE_DIR` | path | `$XDG_CACHE_HOME/mikebom` or `$HOME/.cache/mikebom` | Override for the cache root (per FR-011). Useful in CI runners with non-default caching. |
| `MIKEBOM_CORPUS_SKIP_OCI` | string | `""` | When `"1"`, image-tier targets skip with a diagnostic — useful on developer machines without Docker. In CI this MUST NOT be set (setting it defeats FR-002 image coverage). |

*(Deferred: `MIKEBOM_CORPUS_ASSERTIONS_ONLY` — a "run Layer 1 only" fast-path was scoped out of MVP per analyze-phase U1 finding. If iteration on Layer 1 assertions becomes a hot path, add via a follow-up spec — until then, the standard `cargo test --test public_corpus corpus_<target>` filter is a sufficient scoping mechanism.)*

## Exit Codes

The harness is a cargo integration-test binary, so the OS-level exit code follows cargo's convention: `0` on all-pass, non-zero on any failure. Cargo does NOT distinguish between failure classes at the exit-code level. Failure attribution (mikebom-regression vs corpus-infra vs Layer-1 vs Layer-2 drift) is emitted in the **diagnostic block** (see next section) so CI log-parsers and humans can distinguish, but the exit-code alone signals only pass/fail.

| Exit | Meaning |
|---|---|
| 0 | All corpus targets passed (Layer 1 + Layer 2 where applicable). |
| Non-zero (usually `101` — cargo-test panic) | At least one target failed. Read the diagnostic block(s) in the test output to identify (a) which target(s), (b) which layer / invariant, (c) mikebom-regression vs corpus-infra class. |

**Rationale for the un-distinguished exit code**: matches the milestone-101 windows-smoke and every other `cargo test --test <name>` integration test in this workspace. Introducing a distinguished-exit-code harness would require a bespoke `xtask` binary target — deliberately rejected in research §R2 as heavier than the value delivered. Consumers who need programmatic failure-class distinction should parse the diagnostic block from stdout.

## Diagnostic Output Format

Per FR-009, on any failure the harness prints a diagnostic block:

```text
================================================================================
✗ corpus target FAILED: <target-name>
--------------------------------------------------------------------------------
class:    <mikebom-regression | corpus-infra>
invariant: <invariant_name>
format:   <cdx | spdx-2.3 | spdx-3 | all>
observed: <observed value or file path>
expected: <expected value or file path>
next:     <suggested_action>
================================================================================
```

For corpus-infra failures the block adds:

```text
underlying error: <stderr excerpt, capped at 500 chars>
```

For Layer 2 drift, the harness additionally writes an `.actual.json` file next to the golden and prints its path so `diff <golden> <actual>` works copy-pasteably (matches the existing `cdx_regression.rs` UX).

## Layer 1 Assertion Function Contract

Each per-target Layer 1 function in `layer1_assertions.rs` has this signature:

```rust
pub fn <target_name>_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure>;
```

Contract:
- MUST NOT mutate `sboms`.
- MUST run in under 100ms per target (assertions are cheap JSON lookups).
- MUST return the FIRST failure encountered (fail-fast within a target's assertion chain).
- MUST provide a `suggested_action` that names the mikebom milestone / module the maintainer should investigate (per R4).

## Layer 2 Golden File Contract

Location: `mikebom-cli/tests/fixtures/public_corpus/<target-name>/{cdx.json, spdx-2.3.json, spdx-3.json}`

Contract:
- One file per format per target. Missing files = fresh corpus, treat first run as regen (implicit `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1`).
- After masking (workspace paths, HOME, hashes, timestamps, serial numbers per `feedback_cross_host_goldens`), the file MUST be byte-identical to the freshly-emitted output.
- Files ARE committed to the mikebom repo (this is the drift-tripwire mechanism).

## CI Workflow Contract

`.github/workflows/public-corpus.yml`:

- `on: schedule: cron: '17 6 * * *'` — nightly UTC.
- `on: workflow_dispatch: inputs.branch: type: string, default: main` — manual.
- Job runs on `ubuntu-latest` (Docker preinstalled).
- Steps produce artifacts:
  - `corpus-run-summary.md` — pass/fail per target, always uploaded.
  - `corpus-emitted-sboms/` — the actual SBOMs the corpus emitted, uploaded only on failure (for post-mortem).
- Failure notification: workflow `failure()` sets the workflow status; downstream escalation (e.g., a mikebom-maintainers issue) is out of MVP scope.

## Refresh Helper Contract

`scripts/corpus/refresh-pins.sh`:

- Reads the current manifest from `mikebom-cli/tests/public_corpus/manifest.rs` (parses the const table literally).
- For each git target: `git ls-remote --tags <clone_url> | grep <expected_tag> | awk '{print $1}'` → prints proposed new SHA.
- For each OCI target: `docker manifest inspect <image>:<tag>` → extracts `.digest` → prints proposed new digest.
- Output shape: a unified diff of the manifest, ready for a maintainer to review and apply.
- Does NOT auto-commit (per FR-008: intentional invariant changes must land in-PR with any behavior change).

## What is NOT part of the contract

- The specific 6 target repos and their pinned SHAs (that's manifest content, changes freely under this contract shape).
- The internal shape of `layer1_assertions.rs` per-target functions beyond the signature above.
- The specific masking rules used by Layer 2 (reused from the existing golden-regression pattern; changing those is a separate concern).
