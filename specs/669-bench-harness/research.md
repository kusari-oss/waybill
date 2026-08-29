# Phase 0 Research: Persisted reproducible benchmark suite

**Feature**: `669-bench-harness` | **Date**: 2026-08-29

Seven decisions with rationale + rejected alternatives.

## R1: Peak-RSS measurement — `sysinfo` crate

**Decision**: Add `sysinfo = "0.39.6"` (verified 2026-08-29 via `cargo search sysinfo`; the initial research pass had v0.32 as a knowledge-cutoff artifact — same drift-caught-at-T002 pattern as m668 R1) as a **dev-only** dependency under `xtask/Cargo.toml`. Poll the child `waybill` process at ~10 Hz for peak resident-memory usage; record the max observed value as the fixture-run's `max_rss_kb`.

**Rationale**: Cross-platform peak-RSS on Linux/macOS/Windows is the design constraint (SC-002 reproducibility target + m100 Windows-host stance). Alternatives evaluated:
- `libc::getrusage(RUSAGE_CHILDREN, ...)` — POSIX-only; needs different code path for Windows via `GetProcessMemoryInfo`. Adds Windows/POSIX split in `measure.rs`.
- `/usr/bin/time -v` shell-out — POSIX-only; requires shell parsing; brittle.
- `sysinfo` — pure-Rust, well-tested (~100M downloads on crates.io), cross-platform, single API. Costs 10-15ms overhead per poll on the observer thread; at 10 Hz over a 5-second scan that's 100 polls × 15ms = 1.5s CPU on the observer. Not on the timed thread; doesn't perturb the `waybill` child's wall-clock. Acceptable.

The 10 Hz poll rate is a compromise: high enough to catch spike allocation patterns during SBOM emission, low enough to keep observer CPU cost under 3% of a single core.

**Alternatives considered**:
- Skip RSS measurement in v1; wall-clock-only benchmarks. Rejected — FR-004 explicitly requires RSS; SC-003 regression detection on RSS is a documented user-story-2 acceptance scenario.
- `procfs` crate (Linux-only, richer memory data). Rejected on cross-platform grounds; Linux-only defeats FR-017's runner-metadata generality.
- Wrap the `waybill` binary in a launcher that self-reports RSS via `getrusage(RUSAGE_SELF)` at exit. Rejected — invasive; requires waybill CLI change; violates FR-011 (waybill runtime unchanged).

## R2: JSON schema versioning — root `schema_version: 1` field

**Decision**: Emit `"schema_version": 1` at the root of every `run.json` and `baseline.json` file. Future schema evolution is additive-only for at least 12 months per FR-005; if a breaking change is ever required, it bumps `schema_version` to 2 and consumers with schema-version-aware readers fail closed on unknown versions.

**Rationale**: The FR-005 "additive-only for 12 months" guarantee needs an explicit hook for consumers to detect when they're reading an unknown schema. A root-level `schema_version` field is the industry-standard pattern (Sigstore bundle v0.3, SLSA Provenance v1, CycloneDX 1.6 all use it). Absent a version field, downstream tooling can't distinguish "no matching field because reader is old" from "field genuinely absent because value is empty."

**Alternatives considered**:
- No version field; rely on serde's `deny_unknown_fields` disabled + additive-only discipline. Rejected — provides no forward-migration path if the additive-only guarantee is ever broken.
- URI-based `predicateType`-style versioning à la in-toto. Rejected — over-engineered for a dev-tool internal schema.

## R3: Regression-comment mechanism — `actions/github-script` with magic marker

**Decision**: CI workflow uses `actions/github-script` to post a PR comment via the GitHub API's `issues/comments` endpoint. The comment body starts with a magic marker `<!-- bench-regression-comment-v1 -->`. On subsequent runs, the script queries existing PR comments for this marker; if found, `PATCH`es the existing comment; if not found, creates a new one. Ensures FR-018's "at most one diff comment per PR" invariant.

**Rationale**: `gh pr comment` from the `gh` CLI supports `--edit-last` but only when the last comment on the PR was authored by the current bot user AND was the most recent comment — brittle if any human comments after the bot. The magic-marker + `actions/github-script` API path is robust to interleaved comments.

**Alternatives considered**:
- `marocchino/sticky-pull-request-comment` action — mature, supports magic-marker semantics natively. Rejected on Kusari Inspector grounds: adds a third-party action to the workflow (needs SHA-pinning + dependabot cadence). GitHub's own `actions/github-script` is already a Kusari-blessed dep; extending its usage is lower-friction.
- Comment-per-run without deduplication. Rejected — violates FR-018.

## R4: Fixture-manifest shape — `manifest.json` in test-fixtures repo

**Decision**: Fixtures live at `waybill-test-fixtures/benchmark/<fixture-name>/` with a top-level `waybill-test-fixtures/benchmark/manifest.json` declaring the registry:

```json
{
  "fixtures": [
    {
      "name": "cargo-workspace-medium",
      "path": "source-tier/cargo-workspace-medium",
      "kind": "source-tree",
      "ecosystem": "cargo",
      "supported_modes": ["default", "no-deep-hash", "triple-format", "no-deep-hash+triple-format"],
      "expected_scan_class": "medium"
    },
    ...
  ]
}
```

xtask reads this manifest at bench-time to enumerate the matrix. Adding a new fixture = adding one entry + one directory in the fixtures repo; no waybill main-repo change.

**Rationale**: Data-driven matrix enumeration keeps fixture-set changes decoupled from waybill code changes. Follows the m090 split-repo discipline (FR-019).

**Alternatives considered**:
- Hardcoded fixture list in `xtask/src/bench/matrix.rs` — every fixture add requires a waybill main-repo commit. Rejected as friction.
- Directory-only enumeration (walk `benchmark/` for subdirs). Rejected — no way to declare per-fixture mode support without a manifest.

## R5: Median-of-5 posture reuse from dual_format_perf.rs

**Decision**: `xtask/src/bench/run.rs` implements the same median-of-5 + one-warmup pattern established in `waybill-cli/tests/dual_format_perf.rs::median_of_5` (m045 posture; verified 2026-08-29 at `dual_format_perf.rs:237-247`). Structure lift-and-shift; adapted for the xtask context (spawn `waybill` binary rather than in-process API call).

**Rationale**: Reuses a documented internal convention users of the perf infrastructure already understand. Preserves the 25% noise budget (SC-002 anchor) with an existing pedigree.

**Alternatives considered**:
- Median-of-N with N configurable via flag. Rejected — adds surface without clear value; FR-003 mandates "at least 5" so a fixed 5 is compliant.
- Statistical mean or trimmed mean. Rejected — median is more robust to single-run outliers per m045's rationale (documented in dual_format_perf.rs:227-232).

## R6: Corpus-mode integration

**Decision**: xtask reads `~/.cache/waybill/fingerprints/<pinned-sha>/` at bench-time using the same env-var + fallback path pattern m108 established. The corpus-mode axis on a fixture-run passes `--fingerprints-corpus <path>` to the waybill child; corpus-off runs omit the flag. If the pinned corpus is unreachable at bench-time (network flake, missing cache), the corpus-mode Result records `exit_status = "corpus-unreachable"` per spec Assumption 9 — not silently omitted.

**Rationale**: Reuses existing infrastructure; zero new corpus-side code needed.

**Alternatives considered**:
- Bundle a snapshot corpus in the bench-fixtures repo. Rejected — fragments the corpus into two SoT locations (m108's location + bench-only snapshot). Reuse the canonical corpus.

## R7: Release-prep pre-flight staleness check (Q2)

**Decision**: xtask exposes a `bench --preflight-check` mode that:
1. Reads `docs/perf/baseline.json`
2. Extracts `waybill_commit_sha` from the baseline metadata
3. Runs `git diff --stat <baseline-sha>..HEAD -- 'waybill-cli/**' 'waybill-common/**' 'waybill-ebpf/**' Cargo.lock` (matches SC-006 scope)
4. Exits **non-zero with a diagnostic** if the diff is non-empty (baseline is stale relative to HEAD's waybill-runtime scope)
5. Exits zero if the diff is empty (baseline still matches HEAD; safe to release)

The release-prep flow calls this pre-flight check before opening the release PR; failures print `Run 'cargo run -p xtask -- bench --update-baseline' to refresh the baseline, then re-run release-prep.` and exit non-zero.

**Rationale**: Codifies Q2's "fail loudly" branch as a concrete algorithm. The `git diff` scope matches SC-006 (waybill-runtime code paths) exactly — a fixture-only or docs-only change between baseline and HEAD does NOT trigger staleness; only code changes to the crates that affect scan performance do.

**Alternatives considered**:
- Timestamp-based staleness ("baseline older than 30 days"). Rejected — over-triggers on quiet weeks; under-triggers if a major perf-relevant PR lands the day before release.
- Commit-count-based ("N commits since baseline"). Rejected — doesn't distinguish perf-relevant commits from docs/CI commits.
- Content-hash of Cargo.lock only. Rejected — misses source-code perf regressions with unchanged deps (the common case).

## Empirical claims to re-verify at implementation time

Per memory `feedback_verify_research_empirical_claims`, each of these is a re-check at task-execution time:

- **`sysinfo` current version + Rust MSRV compatibility**: verify via `cargo add sysinfo --dry-run` at task T002 that the current published version supports the workspace MSRV. If v0.32 is stale, adopt the current major.
- **Existing `dual_format_perf.rs::median_of_5` pattern**: verified inline — `waybill-cli/tests/dual_format_perf.rs:237-247` uses 5-sample median with pre-warmup at :273. Lift-and-shift is safe.
- **Existing `.github/workflows/perf.yml`**: confirmed exists via `ls`. m669's new `bench.yml` is disjoint (different trigger + different scope); no conflict.
- **xtask crate structure**: verified — `xtask/src/main.rs` is 40 lines with one enum-variant subcommand (`Ebpf`). Extension pattern is trivial.
- **`sysinfo` cross-platform peak-RSS API**: v0.32 supports `Process::memory()` on Linux/macOS/Windows. Verify empirically via a scratch benchmark on the reference-arch runner during T005 (the driver-implementation task).

## Deferred decisions (out of scope for this plan)

- **Per-dimension regression thresholds**: v1 uses uniform 25% per SC-003. Per-dimension tuning (e.g., stricter for output-bytes, looser for RSS) is a v2 follow-up if empirical evidence shows uniform doesn't fit.
- **Trace-mode benchmarking**: deferred per Q1 clarification. Follow-up feature when eBPF-observed trace measurement discipline is defined.
- **Cross-host comparability**: deferred per spec Assumption 1. v1 pins docs citations to Linux x86_64; cross-host baseline projection is a v2 or v3 problem.
- **Baseline history / trend visualization**: deferred. v1 ships one committed baseline per release; longitudinal analysis via `git log docs/perf/baseline.json` is left to operator tooling.
