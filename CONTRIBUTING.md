# Contributing to mikebom

Thanks for your interest in contributing! mikebom is pre-1.0 alpha; we
encourage a quick discussion on non-trivial changes before you open a
PR so we can align on direction.

## Workflow overview (the speckit lifecycle)

For non-trivial changes (new features, behavior changes, large
refactors, ecosystem additions), mikebom uses the spec-kit lifecycle:

1. `/speckit.specify` — write the feature spec (what + why)
2. `/speckit.clarify` (optional) — resolve open questions
3. `/speckit.plan` — produce `research.md`, `data-model.md`, `contracts/`
4. `/speckit.tasks` — break work into a checklist
5. `/speckit.analyze` (optional) — cross-check spec ↔ plan ↔ tasks
6. `/speckit.implement` — execute the task list

Each milestone lives at `specs/<NNN>-<short-name>/`. See an existing one
(e.g., [`specs/092-fix-maven-version-extract/`](specs/092-fix-maven-version-extract/))
for a complete example.

Per-skill references are under `.claude/skills/speckit-*/SKILL.md`.

**Small drive-by fixes** (typo corrections, single-line bug fixes,
doc tweaks) skip the lifecycle — just open a PR.

## Local development setup

```bash
git clone https://github.com/kusari-sandbox/mikebom.git
cd mikebom
cargo +stable build --release
```

The `sbom`, `policy`, `attestation`, and related subcommands build
under the **stable** toolchain. The eBPF-based `trace` subcommands
additionally need nightly + bpf-linker — see
[`docs/user-guide/installation.md`](docs/user-guide/installation.md)
for the full setup, including the `mikebom-dev` container and Lima VM
options for macOS.

Test fixtures live in a sibling repo (`kusari-sandbox/mikebom-test-fixtures`)
and are cloned automatically by `build.rs` on first build into a
per-host cache at `~/.cache/mikebom/fixtures/<pinned-sha>/`. The
pinned SHA lives in `tests/fixtures.rev`.

## Pre-PR gate (MANDATORY)

Before opening any PR, **both** of these MUST exit clean:

```bash
./scripts/pre-pr.sh
```

This single script runs, in order:

1. `cargo +stable clippy --workspace --all-targets -- -D warnings` — zero
   clippy warnings (warnings become errors).
2. `cargo +stable test --workspace` — every test suite must report
   `N passed; 0 failed`.

Both gates are what CI enforces; running the script locally first saves
a CI round-trip.

For PRs that touch SBOM emission or output formats, also opt-in to the
SPDX-3 conformance validator:

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
```

This requires the JPEWdev `spdx3-validate` Python package pinned in
`.venv/spdx3-validate/`. If the validator isn't installed locally,
the gate skips silently — but CI runs it strictly on release branches,
so test locally before release-bump PRs.

## Walker-audit CI gate

If your PR adds code under `mikebom-cli/src/scan_fs/`, read this section
**before** writing the new filesystem-walking logic. The walker-audit gate
fails fast (under one second) when an unauthorized `fn walk_*` shows up
outside the shared helper or the documented exception list.

### What the gate enforces

The post-milestone-114 invariant is that every ecosystem-discovery
filesystem walker under `mikebom-cli/src/scan_fs/` goes through the
shared `scan_fs::walk::safe_walk` helper. Hand-rolled `read_dir`
recursion bypasses the canonicalize-keyed visited-set + depth-bound +
exclusion-set + skip-debug-log machinery that lives in one place for
auditability.

The CI step (in `.github/workflows/ci.yml`'s `Lint + test (linux-x86_64)`
job, between `actions/checkout` and `Install stable Rust`) runs:

```bash
grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \
  | LC_ALL=C sort -u
```

and `diff`s the result against the committed allow-list at
`mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`. Either-direction
drift (new walker without an allow-list entry, OR stale entry pointing
at deleted code) fails the build.

### What to do when your PR turns red on `Walker-audit allow-list check`

The CI log shows you a unified-diff hunk. Two paths from there:

**Scenario A — you added a walker by accident.** You wrote a one-off
`read_dir` recursion when `safe_walk` would have worked. Refactor the
new code to call `safe_walk`:

```rust
use crate::scan_fs::walk::{safe_walk, WalkConfig};

let cfg = WalkConfig {
    max_depth: 6,
    should_skip: &|p, _| {
        // your skip predicate
        false
    },
    exclude_set,
};
safe_walk(rootfs, &cfg, |path| {
    // your per-path visit logic
});
```

See `mikebom-cli/src/scan_fs/walk.rs`'s module-level comment block for
the full API + the documented exceptions that already exist. The
five-minute walkthrough in
[`specs/114-safe-walk-migration/quickstart.md`](specs/114-safe-walk-migration/quickstart.md)
covers `max_depth` / `should_skip` / callback shape choices.

**Scenario B — your function name starts with `walk_` but doesn't
walk the filesystem.** Common false-positive class: in-memory
iterators (`walk_nested_archives_in_bytes` iterates ZIP entries
parsed from a `Vec<u8>`), tree-page walkers (`walk_schema_page`
walks SQLite B-tree pages), or `#[test]` functions whose name
shares the prefix of the unit under test. The grep gate can't
distinguish these from real filesystem walkers — it's a literal
`fn walk[_(]` match.

Opt out at the function definition site with a sigil comment on
the line IMMEDIATELY above the `fn` signature:

```rust
// walker-audit: false-positive — iterates in-memory zip entries, no filesystem traversal
fn walk_nested_archives_in_bytes(...) { ... }
```

The text after `// walker-audit:` is free-form developer audit
trail (kept short — one phrase explaining what the function does
instead of walking the filesystem). The CI gate's pre-filter
drops any match whose preceding line carries the
`// walker-audit:` sigil before the allow-list diff runs, so the
allow-list file stays minimal and only documents the genuine
filesystem-walking exceptions.

**Don't reach for the allow-list here**: an entry in
`walk.audit-allowlist.txt` says "this fn is a real walker we
accept"; a sigil says "this fn isn't a walker at all". The two
are different audit categories. Issue #378 (the milestone-133
follow-on) introduced the sigil escape hatch precisely so the
allow-list can shrink to only the real walkers.

**Scenario C — your walker legitimately can't fit `safe_walk`.** Rare,
but possible (per-descent stateful pruning, parent-name-aware recursion,
…). In the SAME PR, do two edits:

1. Append the new grep-output line to
   `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`, sorted with
   `LC_ALL=C sort -u`. The easiest path is to regenerate the whole
   file:
   ```bash
   grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \
     | sed 's/^\([^:]*\):[0-9]*:/\1:/' \
     | LC_ALL=C sort -u \
     > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt
   ```
   (The `sed` step strips the absolute line-number column so position
   drift from unrelated code insertions doesn't perturb the allow-list.
   Milestone 117 / issue #347.)
   `git diff` should show **exactly one** added line. More than one
   means either upstream drift (rebase first) or you added more than
   one walker.
2. Add a one-sentence reason in the comment block at the top of
   `mikebom-cli/src/scan_fs/walk.rs`'s "Documented known exceptions"
   subsection naming the new walker + why it can't delegate.

The gate enforces step 1 mechanically; step 2 is reviewer-policed. The
reviewer will see both the allow-list change AND the code change in
one diff and evaluate the new exception's justification at the same
time as the code change.

### When NOT to interact with the gate

- Refactoring inside an existing walker file (renames, helper
  extractions): as long as no NEW `fn walk_*` appears, the gate
  stays quiet.
- Adding a function named `fn walker_*` or `fn walking_*`: the regex
  `'fn walk[_(]'` only matches `fn walk_` or `fn walk(` exactly; longer
  prefixes don't match.
- Working in a different directory (`mikebom-common/`, `mikebom-ebpf/`):
  the gate is scoped to `mikebom-cli/src/scan_fs/` only.

### Why this is a CI gate and not a clippy lint

The gate is a single-line POSIX shell pipeline that runs in <500 ms on
the existing Linux CI runner. A clippy lint would be more "Rusty" but
would also require a Cargo build to fail. The shell gate fails before
any toolchain is installed, short-circuiting clippy + `cargo test` on
the way to maintainer review. The flat-text allow-list is human-
editable in any editor without schema knowledge.

For the design rationale, see
[`docs/design-notes.md` § "Filesystem walking pattern (milestone 114)"](docs/design-notes.md#filesystem-walking-pattern-milestone-114).

### Performance benchmarks (opt-in)

Wall-clock perf benchmarks (`triple_format_perf.rs`, `dual_format_perf.rs`)
do NOT run in the default pre-PR gate or in the per-PR CI lanes — they
inherit shared-CI-runner thermal/scheduler noise that false-fails
intermittently on macOS-latest at ~14–22% measured-reduction vs the
25% gate. Blocking PR merges on those flakes hurts more than the perf
signal helps (see [`specs/094-deflake-perf-tests/`](specs/094-deflake-perf-tests/)
for the architectural rationale).

Instead, [`.github/workflows/perf.yml`](.github/workflows/perf.yml) runs
them:

- **Daily at 06:00 UTC on `main`** — catches background regressions
  within ~24h.
- **On manual `workflow_dispatch`** — `gh workflow run perf.yml`.
- **On PRs labeled `perf`** — opt-in for PRs that touch the scan
  pipeline, output dispatch, or per-format emission.

The perf lane uses `nick-fields/retry@v3` with 3 attempts per test to
absorb runner-noise spikes. It is NOT required for PR merge.

To run perf benchmarks locally:

```bash
cargo +stable test --workspace -- --ignored --test-threads=1
```

`./scripts/pre-pr.sh` and the default `cargo +stable test --workspace`
skip `#[ignore]`'d tests automatically, matching CI default-lane
behavior.

A deterministic structural-correctness sibling test
([`mikebom-cli/tests/triple_format_structural.rs`](mikebom-cli/tests/triple_format_structural.rs))
DOES run in the default lane. It catches single-pass dispatch
regressions binary pass/fail via stderr log-line counting of the
existing `"scan starting"` info-line, plus triple-vs-sequential output
byte-equivalence — no wall-clock semantics, no thresholds, no
flakiness.

## Project principles + where to find them

The canonical source-of-truth for project principles is
[`.specify/memory/constitution.md`](.specify/memory/constitution.md).
Twelve principles to be aware of:

- **I. Pure Rust, Zero C** — no FFI, no `libbpf` bindings, no C
  toolchains in the build pipeline. `aya` provides the eBPF stack.
- **II. eBPF-Only Observation** — eBPF tracing is the trust-rooted
  dependency-discovery path; external sources (lockfiles, registries)
  only ENRICH what was observed.
- **III. Fail Closed** — never gap-fill with heuristics when the
  trace loses data; exit non-zero and surface the gap.
- **IV. Type-Driven Correctness** — newtype wrappers for PURL,
  hashes, license expressions; no `.unwrap()` in production code
  (use `anyhow` / `thiserror`).
- **V. Specification Compliance** — CycloneDX 1.6 + SPDX 2.3 +
  SPDX 3.x conformance is non-negotiable. **Standards-native fields
  take precedence over `mikebom:*` properties** — every new
  `mikebom:*` field MUST first audit each target format for an
  existing native construct.
- **VI. Three-Crate Architecture** — `mikebom-ebpf/` (no_std kernel
  programs), `mikebom-common/` (shared structs), `mikebom-cli/`
  (user-space). Additional crates require a constitution amendment.
- **VII. Test Isolation** — privilege-dependent tests (eBPF) gated
  behind CAP_BPF; unprivileged unit tests run on every CI lane.
- **VIII. Completeness** — minimize false negatives; every observed
  fetch must appear in the SBOM.
- **IX. Accuracy** — minimize false positives; flag low-confidence
  matches via spec-native confidence/evidence fields.
- **X. Transparency** — surface every limitation (overflow events,
  inferred edges, heuristic matches) via spec-native mechanisms.
- **XI. Enrichment** — license, VEX, supplier metadata when
  available; never block SBOM emission on unavailable enrichment.
- **XII. External Data Source Enrichment** — lockfiles / registries /
  hash databases MAY enrich observed components but MUST NOT
  introduce components the eBPF trace didn't observe.

If your change touches any principle, link to it from your PR
description and explain how the change preserves the principle. The
mandatory pre-PR template (`.github/pull_request_template.md`) has
a checkbox for this.

## Pull request etiquette

- Open one PR per logical change. The PR title should match the
  format `<type>(<scope>): <subject>` (e.g., `fix(092): Maven pom.xml
  version-extraction`).
- Include a `## Test plan` section in the PR description with the
  commands you ran locally.
- Run `./scripts/pre-pr.sh` clean before requesting review.
- For changes that regenerate byte-identity goldens, mention the
  expected diff symmetry in the PR description (e.g., "+1521/-1521
  tool-version churn only"). Use `./scripts/regen-goldens.sh` to
  refresh every golden in one pass — it runs the workspace test
  suite under all three `MIKEBOM_UPDATE_*` env vars at once, which
  covers per-test pinned goldens outside the three main regression
  targets. Narrowing cargo to `--test cdx_regression --test
  spdx_regression --test spdx3_regression` silently skips those.

## Reporting issues + security

- Bugs / feature requests: use the structured templates at
  https://github.com/kusari-sandbox/mikebom/issues/new/choose.
- Vulnerabilities: see [`SECURITY.md`](SECURITY.md) — do **not**
  open a public issue.

## License

By contributing, you agree your contributions are licensed under
Apache-2.0 (the project's license — see [`LICENSE`](LICENSE)).
