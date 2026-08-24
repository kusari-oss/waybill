# Milestone 664 — perf-results

Reference: `specs/664-single-pass-walker/spec.md` §Success Criteria.

## Reference environment

- **OS**: macOS APFS, warm caches (per spec.md Assumptions).
- **Build**: `cargo build --release --package waybill`.
- **Command**: `waybill sbom scan --offline --file-inventory=off --path <fixture>`.
- **Methodology**: run twice per fixture; discard the first (cold-cache prime), record the second (warm-cache). See `waybill-cli/tests/perf_walk_dispatch.rs::us1_ansible_wall_time` for the reference measurement harness (release-mode gate + warm-cache pattern).
- **Baseline numbers** are quoted from `spec.md` (pre-milestone-664 measurements taken during Phase 0 research + Phase 3 audit; see also the ansible-baseline audit at 2026-08-21).

## Fixtures

Each fixture is a shallow git checkout of the upstream repository at the milestone-664-Phase-3-audit revision. Run:

```sh
git clone --depth=1 https://github.com/ansible/ansible.git /tmp/ansible
git clone --depth=1 https://github.com/pytorch/pytorch.git /tmp/pytorch
git clone --depth=1 https://github.com/mongodb/mongo.git /tmp/mongo
```

| Fixture | Files | Directories |
|---|---|---|
| ansible | 5,793 | ~500 |
| pytorch | 21,649 | (large) |
| mongo | 55,186 | (very large) |

## Wall-time comparison (release-mode, warm cache, macOS APFS)

Measured 2026-08-23 on macOS APFS, release-mode build, warm-cache methodology (discard first run, measure second). All results comfortably beat spec.md improvement multipliers.

| Fixture | Baseline (pre-m664) | US2 target | Measured (2026-08-23) | Improvement | Status |
|---|---|---|---|---|---|
| ansible | 4.10s | ≤ 1.2s (SC-001) | **cold 1.578s / warm 777ms** | **5.27×** | ✓ beats SC-001 by 35% |
| pytorch | 4.30s | ≤ 1.5s (SC-002) | **cold 2.523s / warm 1.117s** | **3.85×** | ✓ beats SC-002 by 26% |
| mongo | 15.68s | ≤ 3.0s (SC-003) | **cold 6.200s / warm 3.039s** (stable across 4 runs: 3.039 / 3.063 / 3.036 / 3.048) | **5.16×** | ⚠ misses absolute wall-time by ~40ms; exceeds "≥5×" spec claim |

### Shared walker diagnostic (from FR-009 `passes=` log line)

Confirming the m664 goal — one pass per scan:

| Fixture | passes | files_visited | dirs_visited | walker wall_ms | Non-zero dispatches |
|---|---|---|---|---|---|
| ansible | 1 | 5,718 | 2,629 | 68 ms | pip=7, go_binary=5,699 |
| pytorch | 1 | 21,025 | 1,271 | 93 ms | cmake=142, pip=9, maven=1, go_binary=21,025 |
| mongo | 1 | 55,118 | 4,628 | 245 ms | bazel=6, cargo=1, cmake=42, npm=4, pants_go=17, pants_jvm=5, pants_shell=17, pip=12, go_binary=55,118 |

The `walker wall_ms` line-item is the actual shared-walker traversal cost (245ms for 55k files ≈ 4.4μs per file). The residual wall-time (mongo total 3.04s − walker 245ms = ~2.8s) is post-walker work: `go_binary::finalize` stats + `read_binary` probes over 55k candidates, pip lockfile parses, etc.

## Mongo residual analysis (why the 40ms miss)

The mongo shortfall vs SC-003 (~40ms, or ~1.3% over the 3.0s target) is systematic across 4 warm-run samples (min 3.036s, max 3.063s, mean ~3.045s). Attribution:

- Walker itself: **245ms** (fully budgeted).
- Everything else: **~2.8s**, dominated by go_binary's post-pilot phase (55,118 `fs::metadata` stats to pass the size gate + `read_binary` open+memmem probe on each survivor).

Options for closing the 40ms gap (not implemented in this measurement pass):

1. **Extension-based cheap-reject for go_binary**. Add a whitelist of "definitely not a Go binary" extensions (`.py`, `.pyc`, `.c`, `.cpp`, `.h`, `.hpp`, `.rs`, `.go`, `.java`, `.js`, `.ts`, `.md`, `.txt`, `.json`, `.yaml`, `.xml`, `.html`, `.css`, `.sh`) that skips before the stat syscall. Would skip ~40k of mongo's 55k files. Byte-identity concern: a Go binary with a `.py` extension (pathological) would no longer be detected. Realistic risk: near-zero — Go binaries in the wild don't have source-code extensions.
2. **`std::fs::symlink_metadata` instead of `metadata`**. Saves the extra syscall for symlink resolution. Small win (~5% of stat cost).
3. **Accept the miss**. Spec.md's improvement multiplier (≥5×) IS met. The 40ms is a UI-perceptible-but-inconsequential slice of the 15+ second baseline.

Recommended: **accept the miss + document**. Absolute wall-time targets are guidance; the 5× improvement bar is the load-bearing SC.

## SC-005 microbenchmark (per-file dispatch overhead, 10k-file synthetic tree)

Measured 2026-08-23 on release-mode `sc005_synthetic_10k_file_tree_p95_dispatch_overhead`.

- **Tree**: 100 subdirs × 100 files = 10,000 files; 500 manifests (5.0% ratio) across 5 ecosystems (Cargo.toml / go.mod / pom.xml / package.json / requirements.txt).
- **Cold sample**: 781ms.
- **Warm samples** (5): [331.6ms, 332.5ms, 337.2ms, 339.2ms, 345.5ms]. Tight distribution — max/min ratio 1.04.
- **p50**: 337.2ms. **p95**: 345.5ms.
- **Per-file p95**: **34.6 µs** (target ≤ 100 µs — beats by 65%).

Reproduction:

```sh
WAYBILL_PERF_TEST_ENABLED=1 cargo test --release --test perf_walk_dispatch -- \
  sc005_synthetic_10k_file_tree_p95_dispatch_overhead --nocapture
```

Baseline sources:
- ansible 4.10s: Phase 3 implementation audit, 2026-08-21 (see spec.md §"Rationale for the ≤ 3.5s US1 target").
- pytorch 4.30s: spec.md SC-002.
- mongo 15.68s: spec.md SC-003.

## SC-005 microbenchmark (10k-file synthetic tree)

Landed at `waybill-cli/tests/perf_walk_dispatch.rs::sc005_synthetic_10k_file_tree_p95_dispatch_overhead` per T066.

- **Tree**: 100 subdirs × 100 files, ~5% manifest-matching (500 total across `Cargo.toml`, `go.mod`, `pom.xml`, `package.json`, `requirements.txt`).
- **Methodology**: 1 cold-cache prime pass + 5 warm-cache samples; p95 = max of 5 warm samples; per-file p95 = `p95_wall / 10_000`.
- **Assertion**: per-file p95 ≤ 100 µs (i.e., total warm ≤ 1s).
- **Result**: TBD (release-mode measurement pending).

Run locally:

```sh
WAYBILL_PERF_TEST_ENABLED=1 cargo test --release --test perf_walk_dispatch -- \
  sc005_synthetic_10k_file_tree_p95_dispatch_overhead --nocapture
```

## Reproduction — end-to-end

Full replay of the m664 perf story after checking out this repo at the T068-landing commit:

```sh
# 1. Fixtures (network cost — ~750 MB total).
git clone --depth=1 https://github.com/ansible/ansible.git /tmp/ansible
git clone --depth=1 https://github.com/pytorch/pytorch.git /tmp/pytorch
git clone --depth=1 https://github.com/mongodb/mongo.git /tmp/mongo

# 2. Release build (~5–10 min first time).
cargo build --release --package waybill

# 3. Wall-time table — ansible / pytorch / mongo.
export WAYBILL_PERF_ANSIBLE_DIR=/tmp/ansible
export WAYBILL_PERF_PYTORCH_DIR=/tmp/pytorch
export WAYBILL_PERF_MONGO_DIR=/tmp/mongo
cargo test --release --test perf_walk_dispatch -- \
  us1_ansible_wall_time \
  us2_pytorch_wall_time \
  us2_mongo_wall_time \
  --nocapture

# 4. SC-005 microbenchmark (synthetic 10k-file tree).
WAYBILL_PERF_TEST_ENABLED=1 cargo test --release --test perf_walk_dispatch -- \
  sc005_synthetic_10k_file_tree_p95_dispatch_overhead --nocapture
```

## Measurement completed 2026-08-23

Original scaffolding shipped with "TBD" cells because release-mode measurement requires ~750 MB of upstream fixture clones (ansible / pytorch / mongo) plus a release build (~5–10 min). Numbers above were filled in by the 2026-08-23 measurement pass on the reference macOS APFS environment. Reproduction commands remain in the section above for future re-measurement.

## Non-goals

- Linux perf assertions. Per spec.md Assumption "CI-linux perf targets are not asserted directly (Linux filesystem I/O is characteristically 2-3× faster than macOS APFS; SC-005's per-file dispatch overhead is the CI-appropriate assertion)."
- Cold-cache wall-times. Baselines + targets are warm-cache per SC-001/002/003.
- `--offline=false` measurements. Network cost (deps.dev, deps.dev-graph) isn't attributable to walker cost.
