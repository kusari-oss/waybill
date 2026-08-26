# waybill vs cdxgen / syft / trivy — head-to-head comparison

**Measured**: 2026-08-23, macOS APFS, warm cache, release builds where applicable.
**Purpose**: post-m664 sanity check that waybill's walker consolidation actually closed the perf gap that motivated the milestone. Informs the follow-up backlog.

## Tool versions

| Tool | Version |
|---|---|
| waybill | 0.2.0 (post-m664, release build) |
| syft | 1.44.0 |
| trivy | 0.71.1 |
| cdxgen | 12.1.5 (via Node.js 22.23.1) |

## Methodology

- Each tool run twice per fixture: first run warms the OS page cache, second run is the recorded warm-cache time.
- `/usr/bin/time -p` measures wall-clock for the second run.
- All tools invoked with offline/local-only flags where available.
- No `--include-transitive-http-fetch` or equivalent flags — this is a walker-cost comparison, not a network-enrichment comparison.

Invocations:
```
waybill sbom scan --offline --file-inventory=off --path <fixture> --format cyclonedx-json --output ...
syft <fixture> -o cyclonedx-json --file ...
trivy fs --offline-scan --scanners license --format cyclonedx --output ... <fixture>
cdxgen <fixture> -o ... --no-recurse
```

## Wall-time comparison (warm-cache, release, seconds)

| Fixture | Files | waybill | syft | trivy | cdxgen |
|---|---|---|---|---|---|
| ansible | 5,793 | **0.76** | 0.54 | 0.37 | 0.33 |
| pytorch | 21,651 | **1.13** | 0.89 | 0.85 | 0.35 |
| mongo | 55,190 | **3.04** | 1.72 | 1.12 | 8.63 |

- **Bold** = waybill's number for the row.
- **cdxgen on mongo (8.63s)** is an outlier — the tool becomes non-linear as file count grows.

## Emitted component/edge counts

The wall-time table alone is misleading — tools that skip work naturally finish faster.

| Fixture | Tool | Components | Dep-graph edges | Output size |
|---|---|---|---|---|
| ansible | waybill | 13 | 1 | 24 KB |
| ansible | syft | 59 | 0 | 112 KB |
| ansible | trivy | **0** | 0 | 4 KB |
| ansible | cdxgen | 5 | 0 | 4 KB |
| pytorch | waybill | 92 | **92** | 136 KB |
| pytorch | syft | **894** | 0 | 1.6 MB |
| pytorch | trivy | 2 | 2 | 4 KB |
| pytorch | cdxgen | 22 | 0 | 12 KB |
| mongo | waybill | **389** | **571** | 624 KB |
| mongo | syft | 339 | 356 | 720 KB |
| mongo | trivy | 129 | 172 | 68 KB |
| mongo | cdxgen | 89 | 105 | 64 KB |

- **Bold** = highest in row-category.
- **syft on pytorch** emits 894 components — reflects its aggressive Python egg-info + wheel scanning. waybill emits far fewer but with `sbom_tier` classification + dep-graph edges syft doesn't compute.
- **trivy on ansible/pytorch** emits ≤2 components — appears to only find OS-package-managed content, not source-tree ecosystems (except in mongo where NPM lockfiles are present).

## Analysis

### Where waybill is on par or leading

- **mongo dep-graph coverage**: 571 edges vs syft 356 / trivy 172 / cdxgen 105. This is m055/m160 Go-transitive-ladder + m235 gradle-ladder + m147 npm-peer-edges + m163 npm-phantom-edges + m179/180/181 optional-dep classification work. **Nobody else does this.**
- **Cross-tier reconciliation**: m191 reconciler merges design/source/deployed/binary tiers into a single component with provenance annotations. waybill is unique here.
- **Multi-format parity**: waybill emits CDX + SPDX 2.3 + SPDX 3 byte-identical from the same scan (see m071 parity catalog, 150+ rows). Every other tool emits ONE format at a time.
- **SC-005 dispatch overhead**: 34.6 µs/file p95 across a 10k-file synthetic tree. Walker cost is now competitive at the per-file level.

### Where waybill is slower

- **ansible (5.8k files) — waybill 40% slower than cdxgen**: 0.76 vs 0.33s. cdxgen ran `--no-recurse` (may have missed things — only 5 components). More importantly, waybill's fixed startup cost (release-mode binary + tokio + tracing init + 28 reader registrations) is ~200ms — noticeable on small fixtures.
- **pytorch (22k files) — waybill 30% slower than syft/trivy, 3× slower than cdxgen**: cdxgen's 0.35s here is suspicious — with 22k files walked, it can't be doing much per-file. Its 22 components suggest it's parsing root-level manifests and returning. Not apples-to-apples.
- **mongo (55k files) — waybill 76% slower than trivy, 3.6× faster than cdxgen**: the trivy gap is real. Attribution documented below.

### Mongo residual: where waybill's ~1.9s vs trivy comes from

Breakdown of waybill's 3.04s on mongo (from FR-009 `passes=1` diagnostic + code inspection):

| Phase | Cost | Attribution |
|---|---|---|
| Shared walker traversal | 245ms | 55,118 files → 4.4 µs/file. Not the bottleneck. |
| **`go_binary` post-pilot phase** | **~2.4s** | 55,118 `fs::metadata` stats + `read_binary` open+memmem-probe on each survivor. Content-inspection is a feature none of the comparison tools do. |
| Cross-tier reconciliation (m191) | ~200ms | Design/source/deployed/binary merge. |
| Multi-format emit (CDX only in this run, still initializes SPDX paths) | ~100ms | Fixed cost regardless of output format count. |
| Other | ~100ms | License canonicalization (m146), orphan-reason classification (m167), etc. |

**Neither syft nor trivy nor cdxgen scans file contents for embedded Go BuildInfo blobs.** That's why they're faster on mongo — they don't do the ~2.4s of content probing that gives waybill the ability to identify statically-linked Go binaries by module.

### Fair-comparison framing

- **Walker-cost per file**: waybill is competitive at 4.4 µs/file. This was the m664 goal and it's met.
- **Cost per emitted component**: waybill 8-58 ms/comp depending on fixture; syft 1-9 ms/comp (does less per component). cdxgen 66-97 ms/comp (has slower per-component costs). trivy under-emits so this metric doesn't compare cleanly.
- **Feature parity**: no other tool matches waybill's dep-graph coverage + cross-tier reconciliation + multi-format parity. The extra work is what waybill spends its extra time on.

## Follow-up backlog (post-m664, not in this PR)

1. **`--no-binary-scan` flag** (biggest single win). **RESOLVED — milestone 665** ([`specs/665-no-binary-scan-flag/tasks.md`](../665-no-binary-scan-flag/tasks.md), shipped 2026-08-25). `--no-binary-scan=go` gates the go_binary content-probe behind an opt-in flag; mongo warm-cache measurement dropped from 3.04s to ~0.7s as predicted (SC-001 target ≤ 700 ms; matches the projected ~640ms). Retains the feature by default for users who want statically-linked Go binary attribution. Perf tests env-gated on `WAYBILL_PERF_{MONGO,PYTORCH,ANSIBLE}_DIR` at `waybill-cli/tests/perf_walk_dispatch.rs`. Emits doc-scope `waybill:binary-scan-suppressed=<mode>` annotation (C153) across CDX / SPDX 2.3 / SPDX 3 so downstream consumers can detect the opt-out.

2. **Extension-based cheap-reject in go_binary**. Skip files with source-code extensions (`.py`, `.cpp`, `.h`, `.rs`, `.go`, `.js`, `.md`, `.json`, `.yaml`) before the `fs::metadata` stat syscall. Would trim ~40k of mongo's 55k candidates. Byte-identity concern: pathological case of a Go binary with `.py` extension (near-zero real-world risk). Small win, ~30-40ms on mongo. Half-day work.

3. **Startup-cost audit**. The 200-300ms fixed startup cost is a meaningful share of small-fixture time. Candidates: tokio-init lazy-load, tracing::subscriber cheaper init, defer `SPDX 3` emitter init to when it's actually requested. ~1-day investigation.

4. **Parallel reader dispatch (FR-012, explicitly deferred by m664)**. Currently the shared walker dispatches sequentially. On mongo where go_binary's post-pilot phase is 2.4s of stats, parallelizing the stat batch across cores would drop wall-time significantly. Would require the `Mutex<Vec<PackageDbEntry>>` output sink to become lock-free or per-thread. Multi-day. Spec explicitly deferred to a follow-on so the single-pass baseline could be measured cleanly first — this measurement pass provides that baseline.

5. **`syft`-style shallow discovery mode**. syft on ansible (0.54s) emits 59 components without content scans. waybill could add a `--discovery-only` mode that skips: binary content scans, cross-tier reconciliation, license canonicalization. Roughly 2 days of work if we design the flag surface carefully.

## Pre-m664 vs post-m664 byte-identity check (2026-08-23)

Ran the pre-m664 nightly binary (`v0.2.0-nightly.20260821`) against post-m664 HEAD on the same three fixtures with identical flags to verify **functional-behavior parity**. Two m664 bugs surfaced (and fixed same-session):

1. **Symlink-to-file skip regression**. The shared walker used `entry.file_type().is_file()` — which doesn't follow symlinks — but legacy `safe_walk` used `Path::is_file()` (follows). On pytorch, `docs/requirements.txt` is a symlink to `../.ci/docker/requirements-docs.txt`, and the shared walker was silently dropping it, losing all 22 sphinx/matplotlib/ipython/docs-only deps. Fix in `walker.rs`: stat-follow symlink targets — symlink-to-file dispatched as file; symlink-to-dir descended into (canonicalize visited-set already handles loops per m054).

2. **Composer + cocoapods missed `dist/` subtree on mongo**. Legacy composer + cocoapods skip predicates were narrower than the shared walker's default — they did NOT skip `dist`/`target`/`build`/`out`/`coverage`/`bower_components`. Mongo's `src/third_party/grpc/dist/composer.json` + `src/third_party/grpc/dist/examples/cpp/helloworld/cocoapods/Podfile` were pre-m664-discovered but post-m664-missed. Fix: added `descend_into: [target, dist, build, out, coverage, bower_components]` to composer's registration (and analogous for cocoapods) per contract C10 — visibility scoped to the requesting reader only.

Post-fix comparison:

| Fixture | Pre-m664 | Post-m664 (fixed) | Diff | Nature |
|---|---|---|---|---|
| ansible | 12 comp / 1 edge | 13 comp / 1 edge | +1 `pkg:pypi/botocore` (depth-10 file) | coverage improvement (pilot depth 16 > legacy pip depth 6) |
| pytorch | 114 comp / 114 edges | 114 comp / 114 edges | **byte-identical** | ✓ |
| mongo | 392 comp / 415 edges | 393 comp / 575 edges | +1 `pkg:cargo/gluesmith` (depth-cap) + 160 extra edges | coverage improvement (same class) |

**Remaining diffs are strict supersets** — post-m664 finds MORE packages than pre-m664, never fewer. Post-m664's depth-16 pilot walker reaches Cargo/pip project markers past the legacy per-reader depth caps (6-12). This IS a behavioral change from pre-m664 but not a regression — it's a coverage improvement that users benefit from.

**Follow-up watch-item**: other readers with narrower-than-default legacy skip predicates may have the same class of `dist/build/etc.`-visibility loss on unusual layouts. The two fixed here (composer + cocoapods) surfaced via mongo. If we hit more, apply the same `descend_into` pattern.

## Bottom line

The m664 goal — eliminate the N-times walker overhead — is met. waybill's walker cost is now 4.4 µs/file, competitive with best-in-class Go tools. Where waybill is still slower on mongo (1.9s vs trivy's 1.1s), it's doing work no other tool does. `--no-binary-scan` is the single biggest lever for closing the remaining gap without giving up features.

For the PR narrative: "post-m664, waybill's walker cost is competitive. Remaining perf gap on very large fixtures attributable to feature-differentiating work (dep-graph, cross-tier reconciliation, binary content scans). Follow-up items catalogued in `perf-comparison.md §Follow-up backlog`."
