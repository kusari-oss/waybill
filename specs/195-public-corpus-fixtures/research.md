# Research: Public SBOM Regression Corpus

**Date**: 2026-07-14
**Purpose**: Resolve the 8 open design decisions the plan needs before task decomposition: (a) 6 target-repo selections one per ecosystem + one image, (b) harness architecture, (c) how coarse assertions integrate with the golden-diff layer, (d) how upstream repo cloning integrates with the milestone-090 fixture cache.

## R1 — Target selection (per ecosystem)

**Constraint**: (a) publicly-reachable no-auth source (FR-003 / FR-004); (b) exercises the corresponding mikebom reader path; (c) tree size fits inside SC-005's 30-min cold-cache budget budget-share (~5 min per source target, ~10 min for the image target); (d) upstream is stable enough to pin without expecting deletion in the next 12 months; (e) prefers repos likely to trip class-of-bug regressions (`workspace-peer` edges, `main-module` synthesis, `stdlib` link, transitive-graph reachability).

### R1.1 — Go source target: `spf13/cobra`

**Decision**: `github.com/spf13/cobra` pinned to a released tag SHA (initial pin: `v1.9.1` → the SHA it resolves to at authoring time).

**Rationale**:
- Widely-used, actively-maintained Go CLI library. High-signal for the Go reader.
- Small (~2 MB source), fits the per-target scan budget with headroom.
- Non-trivial `go.mod` (~20 direct deps) + full `go.sum` — exercises milestone-091 go.sum fallback + milestone-055 transitive-edges + milestone-053 main-module version-resolution ladder + milestone-194 US1 stdlib-edge synthesis.
- No `internal/` or `test/`-only complexity that would inflate cache footprint.
- Pinned by tag SHA (not tag) — guards against upstream re-tagging (rare but happens).

**Alternatives considered**:
- `kubernetes/kubernetes` — too big (~200 MB source, 1400+ Go modules). Exceeds SC-005 budget alone.
- `moby/moby` — large + complex build. Similar to kubernetes; overkill.
- `spf13/viper` — considered, but cobra has broader dep-graph shapes.

### R1.2 — Rust source target: `BurntSushi/ripgrep`

**Decision**: `github.com/BurntSushi/ripgrep` pinned to `14.1.1` released-tag SHA.

**Rationale**:
- Widely-known Rust CLI, active maintenance, MIT/Unlicense.
- Cargo workspace with multiple member crates (`crates/*`) — exercises milestone-064 cargo main-module emission + milestone-088 procmacro edges + milestone-087 workspace-version-resolution.
- ~10 MB source; ~200 transitive crates; fits budget.
- Cargo.lock committed — exercises the source-tier lockfile path.

**Alternatives considered**:
- `sharkdp/bat` — smaller but similar signal; ripgrep is more heavily depended-on so more likely to be canonical for external comparators.
- `rust-lang/cargo` — cargo scanning cargo is an interesting self-referential case but too complex for corpus MVP.

### R1.3 — npm source target: `expressjs/express`

**Decision**: `github.com/expressjs/express` pinned to `5.1.0` released-tag SHA.

**Rationale**:
- Universally-known npm library. Small (`package.json` + `package-lock.json` + light source tree).
- Non-trivial lockfile (v3) with nested `node_modules` shape — exercises milestone-147 peer-edges + milestone-159 pnpm/yarn alias + milestone-163 phantom-edges + milestone-180 optional-dep classification + milestone-194 US2 nested-nameless-workspace (if present).
- Well-formed `package.json` with `name` — main-module emits cleanly per m066.
- No workspaces / monorepo complexity; a good "canonical happy path" for the npm reader.

**Alternatives considered**:
- `sindresorhus/is-plain-obj` — too tiny; wouldn't exercise transitive-edge paths.
- `facebook/react` — a monorepo (yarn workspaces); useful but heavier. Consider for a future corpus expansion.
- `babel/babel` — big yarn workspaces monorepo; too heavy for MVP.

### R1.4 — Python source target: `pallets/flask`

**Decision**: `github.com/pallets/flask` pinned to `3.1.2` released-tag SHA.

**Rationale**:
- Widely-known Python web framework. `pyproject.toml` + optional `requirements/*.txt` — exercises milestone-183 pip extras/optional + milestone-068 pip main-module + venv scanning path.
- ~5 MB source; ~15 direct + transitive deps.
- Uses `pyproject.toml` (PEP 621) — the modern shape mikebom's readers exercise most.

**Alternatives considered**:
- `psf/requests` — considered, but flask has a richer `[project.optional-dependencies]` block (`extras_require` equivalent) which specifically exercises milestone-183.
- `django/django` — heavier; keeps MVP lean.
- `pytest-dev/pytest` — complex plugin architecture; pytest-as-corpus-target is meta and confusing.

### R1.5 — Java/Maven source target: `google/guice`

**Decision**: `github.com/google/guice` pinned to `7.0.0` released-tag SHA.

**Rationale**:
- Widely-known Java DI framework. Maven multi-module (`core/`, `extensions/*`) — exercises milestone-070 Maven main-module + milestone-085 Maven SPDX dep edges + milestone-092 Maven version extract + milestone-184 optional deps + milestone-009 shade-plugin deps.
- Moderate size (~15 MB source).
- Non-trivial `pom.xml` hierarchy with parent-pom + module boms.

**Alternatives considered**:
- `apache/logging-log4j2` — heavier + more polyglot.
- `google/gson` — smaller but single-module; less signal for the multi-module code path.
- `spring-projects/spring-boot` — massively too big.

### R1.6 — Polyglot container-image target: `postgres:16`

**Decision**: `docker.io/library/postgres:16` pinned by digest (initial pin: whatever digest resolves at authoring time; e.g., `sha256:<64-hex>`).

**Rationale**:
- Publicly-available on Docker Hub; official image; long-lived tag.
- Polyglot: Debian base (deb packages) + PostgreSQL binaries + `gosu` Go binary embedded in the image — exercises deb reader + binary-tier reader + Go BuildInfo extractor + milestone-177 `TransitiveEdgesUnresolvable` classifier (as observed in the m194 session postgres:16 scan).
- Moderate size (~150 MB compressed) — fits the ~10-min per-image budget-share.
- Deterministic digest pin: once pinned, the same digest resolves to byte-identical bytes across all registries.
- Already validated end-to-end during the m194 session; known output shape.

**Alternatives considered**:
- `nginx:latest` — very small (~30 MB) but less polyglot; only the alpine/debian base contributes.
- `python:3.12-slim` — heavy CPython source contributions, but no Go/other-lang bins; less polyglot.
- `wolfi-base` / `chainguard/static` — great for testing minimal-image emission, but too "clean" to exercise interesting orphan / partial classes.
- `k8s.gcr.io/pause:3.10` — too minimal; would produce a nearly-empty SBOM.

## R2 — Harness architecture

**Decision**: A single cargo integration test target (`mikebom-cli/tests/public_corpus.rs`) with sub-module code (`tests/public_corpus/*`), gated behind `MIKEBOM_RUN_PUBLIC_CORPUS=1`. Each corpus target is one `#[test]` function that (a) ensures the cache directory contains the pinned artifact (clone or pull if missing), (b) invokes the released `mikebom` binary via `env!("CARGO_BIN_EXE_mikebom")` with the target's scan command, (c) applies Layer 1 assertions, (d) if Layer 1 passes, applies Layer 2 golden diff.

**Rationale**:
- Cargo integration tests are the mikebom idiom for this kind of thing (matches `cdx_regression`, `spdx_regression`, `spdx3_regression`, `graph_completeness_operator_root`, `transitive_parity_go`, and the milestone-101 Windows smoke).
- `env!("CARGO_BIN_EXE_mikebom")` matches the milestone-101 pattern for spawning the actual production binary from an integration test — no compilation shortcuts.
- One `#[test]` per target lets cargo's parallel test runner run 5-6 targets concurrently (bound by CI runner cores + network) — helps hit SC-005's 30-min budget on fresh checkouts.
- Gate mechanism: `MIKEBOM_RUN_PUBLIC_CORPUS=1`. When unset, each test does an early `println!("skipping: MIKEBOM_RUN_PUBLIC_CORPUS not set")` + returns Ok — matches the milestone-101 skip idiom.
- Alternative rejected — a separate binary crate (`corpus-runner`) — adds compile-time cost + a new bin target; a cargo test target is strictly lighter.

**References**:
- `mikebom-cli/tests/cdx_regression.rs` — golden-regression byte-identity precedent.
- `mikebom-cli/tests/graph_completeness_operator_root.rs` — pattern for spawning `mikebom` with `--root-name`.
- `mikebom-cli/tests/windows_smoke.rs` (m101) — env-gated integration test with graceful skip.

## R3 — Corpus cache layout

**Decision**: `~/.cache/mikebom/corpus/<source-id>/<pin>/` where `<source-id>` is `hex(sha256(source_url_bytes))[0..16]` (16 hex chars) and `<pin>` is the raw commit SHA (40 hex) or image digest (`sha256-<64-hex>`). Cache-key layout mirrors milestone 090 (fixture cache) verbatim; the new sub-tree lives alongside `~/.cache/mikebom/fixtures/`.

**Rationale**:
- Same cache-hierarchy pattern as milestone 090 — operators already know this shape, `du -sh ~/.cache/mikebom/` gives them a coherent picture of what mikebom eats on disk.
- Content-addressed by pin: if two corpus refresh cycles both need the same `spf13/cobra@v1.9.1`, they share the same cache dir; if a subsequent pin refresh moves to `v1.10.0`, the `v1.9.1` cache stays behind until manually cleared (matches milestone 090's stay-set semantics).
- Rehydration: if cache dir exists AND contains the expected marker file (`.corpus-pin-verified` — sha256 of the pinned SHA vs cache-dir name), skip the clone/pull. Otherwise clone/pull fresh.
- Source clone: `git clone <url> <cache-dir>/repo && git -C <cache-dir>/repo checkout <sha> && touch <cache-dir>/.corpus-pin-verified` — same shell-out pattern as milestone 090.
- Image pull: `docker pull <image>@<digest>` — pull is idempotent; the image ends up in the local Docker daemon's storage (NOT in `~/.cache/mikebom/corpus/`). For the SBOM scan invocation, mikebom's existing `--image` flag references the image by digest directly; we do NOT extract to a rootfs.

**References**:
- `mikebom-cli/build.rs` — milestone 090 fixture-cache implementation. Reused via the same `git clone` shell pattern (not the same code path — the cache scope differs).
- `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:733` — milestone 053's `git describe` shell pattern (subprocess spawn model).

## R4 — Layer 1 (coarse assertions) shape

**Decision**: Per-target coarse-assertion functions live in `mikebom-cli/tests/public_corpus/layer1_assertions.rs` as a `HashMap<TargetName, Fn(&EmittedSboms) -> Result<(), AssertionFailure>>` (or equivalent — a switch on target name). Each function receives the three parsed SBOMs (CDX + SPDX 2.3 + SPDX 3) and performs a small ordered list of assertions per FR-005 layer 1:

- Graph-completeness value (e.g., "must be `complete`" or "must be `partial` with reason set exactly `{TransitiveEdgesUnresolvable{ecosystems: ["generic", "golang"]}}`" for `image-postgres16`).
- Reachability floor (e.g., "at least 90% of components reachable from root").
- Canonical PURL presence (e.g., for `go-cobra`, must contain `pkg:golang/github.com/spf13/cobra@vX.Y.Z` and `pkg:golang/stdlib@vX.Y.Z`).
- Ecosystem-specific class-of-bug tripwires (e.g., for `npm-express`, must have at least one workspace-peer edge; for `image-postgres16`, must have at least one `pkg:deb/*` component; for `rust-ripgrep`, main-module PURL must be `pkg:cargo/ripgrep@vX.Y.Z`).

On failure, the function returns a structured `AssertionFailure { invariant_name, observed, expected, suggested_action }` that the harness renders per FR-009.

**Rationale**:
- Function-per-target keeps assertions readable and easy to grow (adding a new invariant is one line of new code, not a schema change).
- Structured `AssertionFailure` matches the milestone-108 fingerprint-match diagnostic pattern.
- Alternative (declarative JSON invariant files) rejected — invariants often depend on version numbers that change across pin refreshes; a Rust function can dynamically construct the expected canonical PURL from the pinned tag/SHA, whereas a JSON file would need a templating layer.

## R5 — Layer 2 (full-SBOM golden diff) reuse

**Decision**: Layer 2 reuses the exact byte-identity comparison + non-deterministic-field masking helpers used by `cdx_regression.rs`, `spdx_regression.rs`, and `spdx3_regression.rs`. Golden files live at `mikebom-cli/tests/fixtures/public_corpus/<target>/{cdx,spdx-2.3,spdx-3}.json`.

**Rationale**:
- Every masking rule already exists (workspace path rewrite, hash normalization, timestamp masking, HOME isolation) per the memory `feedback_cross_host_goldens`. No new masking logic needed.
- Update env var: `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` (parallel to the `MIKEBOM_UPDATE_CDX_GOLDENS=1` etc pattern). When set, the diff-compare is replaced with a file-write.
- Layer 2 runs ONLY IF Layer 1 passes (Layer 1's diagnostic is more actionable per the R4 rationale). If Layer 1 fails, Layer 2 is skipped and the harness reports the Layer 1 failure only.

**Alternatives considered**:
- Structural diff (JSON-tree walk with per-field allowlist/blocklist) — more informative but strictly heavier code. Byte-identity + masking is already the mikebom convention; keeping conventions consistent avoids reviewer surprises.

## R6 — CI workflow shape

**Decision**: New workflow `.github/workflows/public-corpus.yml` with two triggers:

- `schedule: cron: '17 6 * * *'` — nightly at 06:17 UTC (offset by 17 min to avoid the top-of-hour GH Actions rate-limit spike per the memory `project_ci_timing` implication).
- `workflow_dispatch: inputs.branch:` — manual trigger on any branch.

Runner: `ubuntu-latest` (has Docker preinstalled). Steps: (a) `actions/checkout` at pinned SHA, (b) `dtolnay/rust-toolchain@stable`, (c) `Swatinem/rust-cache@v2`, (d) `cargo build -p mikebom --release` (release-mode binary for realistic perf), (e) `MIKEBOM_RUN_PUBLIC_CORPUS=1 cargo test --test public_corpus --release -- --nocapture --test-threads=<capped>`, (f) `upload-artifact` on failure to capture the actual SBOMs + diffs for post-mortem.

**Rationale**:
- Cron trigger surfaces regressions within 24h of merge; matches Q2 clarification.
- Dispatch trigger lets maintainers validate a PR branch on demand.
- Release-mode build matches the release binary customers actually run (matches milestone-094 perf-test convention).
- `--test-threads` cap avoids `git clone` / `docker pull` thrash from parallel targets (each target competes for network + disk; excess parallelism is counterproductive).
- Artifact upload on failure is essential — post-mortem needs the actual SBOM the corpus failed to match.

**Alternative rejected**: including corpus in the existing `.github/workflows/ci.yml` — that workflow runs on every push/PR, which SC-004 explicitly forbids.

## R7 — Refresh-pins helper

**Decision**: `scripts/corpus/refresh-pins.sh` — an intentionally simple bash script that iterates the manifest, resolves upstream refs to current SHAs / digests, and prints a diff of the manifest that a maintainer commits by hand. Does NOT auto-commit.

**Rationale**:
- Auto-commit would break the FR-008 invariant ("intentional invariant changes MUST land in the same PR as the mikebom behavior change"). Auto-refresh could silently rebaseline goldens when upstream churn changes an emitted PURL for reasons unrelated to a mikebom fix.
- The script's job is to reduce refresh-pins toil (looking up `git ls-remote refs/tags/<tag>^{}` + `docker manifest inspect <image>:<tag> --format '{{.Digest}}'`) but keep the human in the loop for the actual commit.

## R8 — Corpus target invariants seeded from m194 session data

**Decision**: The initial Layer 1 invariant set for each target is seeded from the actual output observed during the m194 session:

- `image-postgres16` expected value: `partial` with reason `TransitiveEdgesUnresolvable{ecosystems: ["generic", "golang"]}`; expected component count: ~145 (post-file-tier-exclusion).
- Go source targets (cobra): expected value: `complete` (per m194 US1 stdlib fix + all baseline behavior); expected stdlib edge present.
- Rust/npm/Python/Java targets: expected value seeded from a first-pass scan at authoring time; documented in the initial commit.

**Rationale**:
- Seeding from real observed data anchors the corpus to actual current behavior rather than aspirational behavior. Any future behavior change legitimately updates the invariants in-PR.
- Prevents the "invariants describe how mikebom SHOULD work" trap where corpus red-flags legitimate mikebom output because the spec author imagined a different graph shape.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Go target | `spf13/cobra@v1.9.1` | `kubernetes/kubernetes` | Fits budget; rich Go signal without size cost |
| Rust target | `BurntSushi/ripgrep@14.1.1` | `sharkdp/bat` | Cargo workspace shape exercises more readers |
| npm target | `expressjs/express@5.1.0` | `facebook/react` | Canonical happy-path npm; skip monorepo weight |
| Python target | `pallets/flask@3.1.2` | `psf/requests` | Richer extras / optional-dependency signal |
| Maven target | `google/guice@7.0.0` | `apache/logging-log4j2` | Multi-module Maven; moderate weight |
| Image target | `postgres:16` by digest | `nginx:latest` | Polyglot (deb + gosu Go bin) exercises m177 |
| Harness | cargo integration test + env gate | separate bin crate | Matches milestone-101 pattern |
| Cache layout | `~/.cache/mikebom/corpus/<sha>/<pin>/` | in-repo cache dir | Matches milestone-090 pattern |
| L1 shape | Rust functions per target | JSON invariant files | Version-parametric expected PURLs; readable code |
| L2 shape | Byte-identity golden + existing masking | Structural JSON diff | Matches existing golden convention |
| CI cadence | Cron nightly + workflow_dispatch | Every-PR required | Q2 clarification: nightly + manual |
| Refresh helper | Manual review (print diff) | Auto-commit | FR-008 gate; keep human in loop |
| Invariant seed | Actual current output | Aspirational behavior | Anchors corpus to reality |
| New Cargo deps | Zero | (n/a) | Existing crates cover everything |
| New `mikebom:*` annotations | Zero | (n/a) | Corpus consumes; does not extend emitter |
