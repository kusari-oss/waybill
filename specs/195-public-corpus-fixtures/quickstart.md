# Quickstart: Public SBOM Regression Corpus

**Date**: 2026-07-14
**Audience**: mikebom maintainer running the corpus locally or CI operator wiring it up.

## Prerequisites

- Working mikebom checkout on branch at or after m195 merge.
- `git` on `$PATH` (already required for mikebom development).
- `docker` on `$PATH` — required for the `image-postgres16` target. If absent, set `MIKEBOM_CORPUS_SKIP_OCI=1` to skip image-tier targets on developer machines.
- Approximately 5 GB free disk space in `$HOME/.cache/mikebom/corpus/` on first run (cloned repos + Docker image storage).
- Reliable outbound HTTPS access to `github.com`, `docker.io`, and the language-ecosystem registries (crates.io, npmjs.com, pypi.org, maven central).
- Approximately 30 minutes wall-clock for a cold-cache first run (per SC-005); ~5 minutes for subsequent warm-cache runs.

## Reproducer 1 — Run the full corpus locally

```bash
# From the mikebom repo root:
cargo build --release -p mikebom --bin mikebom

MIKEBOM_RUN_PUBLIC_CORPUS=1 \
  cargo test --test public_corpus --release -- --nocapture --test-threads=3
```

**Expected**: all 6 corpus targets report `PASS`; exit 0.

**On first run**: expect `git clone` + `docker pull` progress lines; expect wall-clock in the 20-30 minute range depending on your connection.

**On subsequent runs**: cache is warm; wall-clock in the 3-5 minute range (scan work only).

## Reproducer 2 — Run a single target

```bash
MIKEBOM_RUN_PUBLIC_CORPUS=1 \
  cargo test --test public_corpus --release corpus_go_cobra -- --nocapture
```

Substitute `corpus_go_cobra` with `corpus_rust_ripgrep`, `corpus_npm_express`, `corpus_python_flask`, `corpus_maven_guice`, or `corpus_image_postgres16` as needed.

## Reproducer 3 — Skip image-tier targets (Docker unavailable)

```bash
MIKEBOM_RUN_PUBLIC_CORPUS=1 \
MIKEBOM_CORPUS_SKIP_OCI=1 \
  cargo test --test public_corpus --release -- --nocapture
```

**Expected**: 5 source targets run; the `corpus_image_postgres16` target prints `skipping: MIKEBOM_CORPUS_SKIP_OCI set` and passes trivially.

## Reproducer 4 — Regenerate golden snapshots after an intentional mikebom change

When an intentional mikebom improvement (e.g., a new milestone that changes graph-completeness classification for a corpus target) requires regenerating Layer 2 golden files:

```bash
# Ensure the improved mikebom is built:
cargo build --release -p mikebom --bin mikebom

# Regen goldens:
MIKEBOM_RUN_PUBLIC_CORPUS=1 \
MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
  cargo test --test public_corpus --release -- --nocapture

# Review the diff:
git diff mikebom-cli/tests/fixtures/public_corpus/

# If the diff is consistent with the intentional behavior change,
# include it in the same PR as the behavior change (per FR-008).
```

**Guard rail**: `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS` writes goldens ONLY for targets whose Layer 1 assertions pass. Layer 1 failures block Layer 2 golden regen — this prevents baselining a corrupted graph shape.

## Reproducer 5 — Refresh pinned SHAs / digests

When upstream corpus targets release new versions and the maintainer wants to bump pins:

```bash
./scripts/corpus/refresh-pins.sh
```

**Expected output**: a unified diff of the proposed manifest changes, printed to stdout. The maintainer reviews the diff and manually applies the changes to `mikebom-cli/tests/public_corpus/manifest.rs`, then re-runs the corpus with `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDENS=1` (per Reproducer 4) to rebaseline goldens against the new pins.

**Guard rail**: The script does NOT auto-commit and does NOT auto-modify the manifest. Human review is mandatory per FR-008.

## Reproducer 6 — Verify a specific class-of-bug regression trip

Simulates a maintainer accidentally reverting m194 US1 (Go stdlib edge synthesis):

```bash
# Revert m194 US1 temporarily (DO NOT commit):
git revert --no-commit <m194-US1-commit>

# Rebuild + run corpus:
cargo build --release -p mikebom --bin mikebom
MIKEBOM_RUN_PUBLIC_CORPUS=1 \
  cargo test --test public_corpus --release corpus_go_cobra -- --nocapture
```

**Expected** per SC-001:
- Exit code 1 (Layer 1 assertion failure).
- Diagnostic block naming the missing stdlib edge: `✗ corpus target FAILED: go-cobra ... invariant: stdlib-edge-present ... observed: no edge from pkg:golang/github.com/spf13/cobra to pkg:golang/stdlib@v* ... expected: at least one such edge`.

Reset the working tree after verifying:

```bash
git reset --hard HEAD
```

## Reproducer 7 — CI dispatch on a PR branch

From the GitHub UI or CLI:

```bash
gh workflow run public-corpus.yml --ref my-pr-branch
```

**Expected**: workflow runs on `ubuntu-latest`, produces the `corpus-run-summary.md` artifact whether it passes or fails, and the `corpus-emitted-sboms/` artifact only on failure.

## Troubleshooting

- **`error: cannot find --test public_corpus`**: build the test binary first via `cargo test --test public_corpus --no-run` before adding filter args.
- **`git clone` fails with 403 / 404**: verify the pinned URL in `manifest.rs` still resolves publicly (upstream may have moved / been made private per Edge Cases in spec).
- **`docker pull` hangs / times out**: check network egress; verify the pinned image digest still exists at Docker Hub (rare but happens for unmaintained images).
- **Layer 2 diff shows only whitespace / SPDX-ID reordering**: likely a mikebom emission-determinism regression, not a real behavior change. Investigate before regenerating.
- **Cold-cache run exceeds 30 min**: check `$HOME/.cache/mikebom/corpus/` for partial clones (`.corpus-pin-verified` marker missing on some dirs); `rm -rf` any incomplete dir and re-run.

## Sizing expectations

| Target | Cache size (approx) | Cold scan time | Warm scan time |
|---|---|---|---|
| `go-cobra` | 5 MB | 30s | 5s |
| `rust-ripgrep` | 20 MB | 60s | 10s |
| `npm-express` | 15 MB | 45s | 8s |
| `python-flask` | 10 MB | 30s | 5s |
| `maven-guice` | 40 MB | 90s | 15s |
| `image-postgres16` | 200 MB image + parse | 10 min | 30s |

Totals — cold: ~15 min, warm: ~1 min (well inside SC-005's 30-min budget).
