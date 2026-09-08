# Phase 0 Research: Nightly SBOM Quality Regression Corpus

All decisions below were validated empirically against a real 18-repository trial run on
2026-09-03 (macOS arm64, `waybill` release build, `sbomqs` v2.0.5). Where a decision
contradicts an earlier assumption, the measurement that overturned it is cited.

---

## R1 — Corpus configuration format: TOML

**Decision**: The corpus lives in a committed TOML file at `xtask/corpus/quality-corpus.toml`.

**Rationale**: Ranges are hand-authored (spec Assumptions), which makes the *reason* for a
bound as important as the bound itself. A range of `pkgs = { min = 11, max = 60 }` on
`python-ansible` is inexplicable without a comment noting that ansible commits no lockfile and
yields only 11 package-tier components. TOML carries comments; JSON does not. `toml = "0.8"` is
already in the workspace lockfile via `waybill-cli`, so promoting it into `xtask/Cargo.toml`
costs zero new transitive crates.

**Alternatives rejected**:
- **JSON**, reusing the already-present `serde_json` and matching m669's
  `benchmark/manifest.json`. Rejected: no comments. m669's manifest is a mechanical registry;
  this file is a reviewed judgement record.
- **A Rust `const` array**, matching the m195 corpus manifest at
  `waybill-cli/tests/corpus_harness_195/manifest.rs`. Rejected: violates FR-001 — adding a
  repository would become a code change requiring recompilation.

---

## R2 — Offline scanning: a deliberate floor

**Decision**: Every scan runs `waybill --offline`.

**Rationale**: Reproducibility. With a pinned commit and no network, a changed measurement is
attributable to a waybill change and nothing else, which is the entire premise of the gate.

**Cost, stated plainly**: `--offline` suppresses more than enrichment. For Go it disables the
m055/m160 proxy-fetch ladder, dropping to the m091 `go.sum` fallback;
`.github/workflows/realistic-projects.yml` says as much, running *without* `--offline`
specifically "so steps 1+3 of the ladder can supply edges". It also disables deps.dev
enrichment, the fingerprint corpus, and Go cache warming. **These measurements therefore
describe waybill's offline floor.** An improvement that only manifests online is invisible to
this gate. Recorded here and in spec Assumptions because Constitution Principle VIII
(Completeness) invites the opposite reading.

**Follow-on hazard**: Go's offline edge count depends on `$GOMODCACHE` state — there is an
existing test keyed on exactly this
(`scan_go_source_tree_emits_transitive_edges_when_cache_present`). The job must pin that state
deliberately (empty) rather than inherit whatever the runner happens to have, or Go component
counts will drift under the ranges.

---

## R3 — Flatness: measured independently, never self-reported

**Decision**: Compute flatness from the emitted document's own `dependencies[]` array:
relationship count, count of components having at least one outgoing relationship, and greatest
BFS depth from `metadata.component.bom-ref`. A document is **flat** when greatest depth ≤ 1.
Record waybill's `waybill:graph-completeness` property as a *separate* field.

**Rationale — this is the single most load-bearing finding of the trial run.** Three of
eighteen repositories self-reported `graph-completeness: complete` while measuring as
structurally flat:

| target | waybill self-report | measured depth | components with outgoing edges |
|---|---|---:|---:|
| `cmake-nlohmann-json` | `complete` | 1 | 1 |
| `ruby-jekyll` | `complete` | 1 | 1 |
| `uv-meilisearch-python` | `complete` | 1 | 1 |

Had the gate trusted waybill's annotation, all three would have scored as healthy. A
self-report cannot catch a bug in the thing reporting.

**Alternatives rejected**:
- **Read `waybill:graph-completeness` only.** Zero new logic, and it is what m195's Layer-1
  assertions already use. Rejected on the evidence above.
- **Ratio-based flatness** (e.g. "flat if fewer than 10% of components have outgoing edges").
  Rejected as an unnecessary second threshold to tune; depth ≤ 1 is exact, explicable, and
  needs no calibration.

---

## R4 — Quality score: `sbomqs`, CycloneDX, overall score, version-pinned

**Decision**: Invoke `sbomqs score --json <cdx>` and read `files[0].sbom_quality_score`. Score
the CycloneDX output only. Pin the expected `sbomqs` version in the corpus config and report a
mismatch.

**Rationale**: `files[0].sbom_quality_score` is the documented overall 0–10 score in the v2.x
JSON shape. The trial run initially read a non-existent `avg_score` key and silently produced
`None` for all eighteen targets — a concrete demonstration that an unpinned, unvalidated
external tool fails *quietly*, which is why FR-015 and FR-016 exist.

**Observed range across seventeen targets: 5.75 – 7.70, every target graded C or D.** No
target reached A or B. Ranges must be authored tightly to carry meaning; a ±25%-style relative
band, as m669's bench uses, would be far too loose here.

**Version hazard**: the trial ran `sbomqs` v2.0.5 locally while `.github/workflows/ci.yml:315`
pins v2.0.6 for `sbomqs_parity`. The new workflow MUST pin the same version CI already
installs, and the corpus config records it so a drift is reported rather than silently
rescoring.

**Note**: `sbomqs` counts the document's root component; the independent count in R5 does not.
`go-cobra` reports 8 to `sbomqs` and 7 package-tier components here. Both are correct; the
report records them as distinct fields and must not be reconciled.

**Deferred**: scoring SPDX 2.3 and SPDX 3 as well. FR-030 requires the measurement set be
shaped so adding formats is additive — the report keys scores by format name from the outset,
with only `cyclonedx` populated.

---

## R5 — Component counts: split package-tier from file-tier, filter neither

**Decision**: Record two counts — components carrying a `purl` (package-tier) and components
without one (file-tier) — as independently rangeable measurements. Do not pass any tier filter.

**Rationale**: m133's file-tier reader emits components for unattributed content (shell
scripts, `.ps1` files). In several targets it dominates:

| target | package-tier | file-tier | file share |
|---|---:|---:|---:|
| `python-ansible` | 11 | 375 | 97% |
| `python-pytorch` | 112 | 181 | 62% |
| `cpp-mongo` | 397 | 420 | 51% |
| `go-kubernetes` | 456 | 330 | 42% |

A single blended count would move for two unrelated reasons. Splitting them lets a
package-count regression and a file-count regression each trip their own range.

**Alternative investigated and rejected — `--tier source-only`.** This was the initial
recommendation and **the measurement disproved it.** Running both modes side by side:

| target | mode | pkgs | files | edges |
|---|---|---:|---:|---:|
| `python-ansible` | all | 11 | 375 | 386 |
| `python-ansible` | `source-only` | **0** | **376** | 376 |
| `npm-express` | all | 43 | 1 | 43 |
| `npm-express` | `source-only` | **0** | **2** | 2 |
| `go-kubernetes` | all | 456 | 330 | 1752 |
| `go-kubernetes` | `source-only` | 456 | **330** | 1752 |

Three conclusions: it does **not** remove file-tier components (kubernetes keeps all 330;
ansible rises 375→376); it **zeroes** targets whose components are `design`-tier because they
commit no lockfile; and it is a **no-op** where everything is already `source`-tier. It
removes the signal and keeps the noise. *(The small file-count increases under filtering are
unexplained — dropping components should not add any — and are worth a separate look; they do
not affect this decision.)*

**Also rejected — `--file-inventory=off`** (`waybill-cli/src/cli/scan_cmd.rs:891`), which
*would* correctly suppress file-tier emission. Rejected because it scans differently from how
a real user scans, and it hides a file-count regression rather than surfacing it.

---

## R6 — Acquisition: shallow fetch at pinned SHA, cache locally, cold in CI

**Decision**:
```
git init && git remote add origin <url>
git fetch --depth 1 origin <sha> && git checkout FETCH_HEAD
```
Cache at `~/.cache/waybill/quality-corpus/<name>/<sha>/`, reused when present. CI restores no
Actions cache for it.

**Rationale**: m195's cache does a full `git clone` then `git checkout <sha>`
(`waybill-cli/tests/corpus_harness_195/cache.rs:93`) — fine for cobra, ruinous for
kubernetes, pytorch and mongo. GitHub serves arbitrary SHAs, so a depth-1 fetch retrieves one
commit's tree and no history. Measured cost for all eighteen targets at parallelism 4:
**~95 s and 2.2 GB**, worst single target `cpp-mongo` at 839 MB / 29 s.

**Why no Actions cache**: GitHub caps total cache at 10 GB per repository and evicts after 7
days unused. Three multi-hundred-MB targets would crowd that ceiling and start evicting the
Rust build caches other workflows depend on. At ~95 s, fetching fresh each night is cheaper
than the coordination cost — and it removes a whole class of stale-cache confusion. Local
caching is still worth having for developer iteration, which is why the cache directory exists
at all.

**Sub-repositories are NOT retrieved.** `pytorch/pytorch` keeps ~30 nested sub-repositories
under `third_party/`; a non-recursive fetch leaves them empty. This is a deliberate, recorded
choice (spec Assumptions): it is deterministic and therefore rangeable, but pytorch's counts
are correspondingly lower than a developer's working copy.

---

## R7 — Corpus membership and measured baseline

**Decision**: Eighteen git repositories at first landing. Container images are out of scope
(the m195 suite already covers one).

Full measurements, offline, at the pins recorded in
[data-model.md](./data-model.md):

| target | wall | sbomqs | pkgs | files | edges | depth | flat | waybill says |
|---|---:|---:|---:|---:|---:|---:|:--|:--|
| go-kubernetes | 103437ms | 7.04 | 456 | 330 | 1752 | 3 | no | partial |
| cpp-mongo | 22470ms | 6.39 | 397 | 420 | 424 | 6 | no | partial |
| python-pytorch | 3932ms | 5.91 | 112 | 181 | 4 | 1 | **flat** | partial |
| python-ansible | 2737ms | 5.87 | 11 | 375 | 386 | 1 | **flat** | partial |
| gradle-apache-solr | 1983ms | 6.74 | 1108 | 80 | 1371 | 8 | no | partial |
| ruby-rails | 1907ms | 6.79 | 726 | 20 | 1291 | 8 | no | partial |
| pants-backend-ai | 1746ms | 7.27 | 271 | 59 | 918 | 5 | no | partial |
| gradle-bitwarden-android | 1213ms | 6.79 | 337 | 9 | 385 | 5 | no | partial |
| rust-zizmor | 354ms | 7.70 | 463 | 1 | 1140 | 11 | no | partial |
| maven-guice | 263ms | 6.56 | 35 | 12 | 32 | 1 | **flat** | partial |
| cmake-nlohmann-json | 233ms | 6.57 | 19 | 1 | 20 | 1 | **flat** | complete |
| pnpm-vue-core | 218ms | 7.66 | 620 | 22 | 890 | 7 | no | partial |
| rust-ripgrep | 185ms | 7.49 | 51 | 7 | 94 | 4 | no | partial |
| go-cobra | 136ms | 7.59 | 7 | 0 | 7 | 2 | no | complete |
| npm-express | 103ms | 5.81 | 43 | 1 | 43 | 1 | **flat** | partial |
| python-flask | 99ms | 7.52 | 104 | 1 | 151 | 2 | no | partial |
| yarn-guac-visualizer | 88ms | 6.08 | 900 | 0 | 1925 | 8 | no | partial |
| uv-meilisearch-python | 49ms | 6.79 | 29 | 1 | 30 | 1 | **flat** | complete |

**`jekyll/jekyll` was measured and rejected.** It produced **zero** package-tier components —
all 33 were file-tier shell scripts — because it commits no `Gemfile.lock`. `rails/rails`
replaced it and is one of the strongest targets in the set: 726 package-tier components, all
`source`-tier, depth 8.

**Three targets are permanently flat for upstream reasons, not waybill defects**:
`npm-express` (no `package-lock.json`), `python-ansible` (no pip lockfile), and
`uv-meilisearch-python`. They are retained — they still exercise reader paths and still detect
count and quality regressions — with their flatness expectation authored as `flat`.

**Two earlier concerns were disproved by measurement**: `mongodb/mongo` yields 397 package-tier
components (its Bazel migration is real), and `apache/solr`'s Gradle reading works well
(1108 package-tier, depth 8 — the richest source target). Both were candidates for removal on
suspicion; both stay.

---

## R8 — Wall time: gated, single-sample, generously ranged

**Decision**: Gate wall time. Time only the `waybill` invocation. Take one sample per target
per run.

**Rationale**: Because retrieval is excluded and the scan is offline, network variance is
already out of the measurement — the operator's explicit reasoning when this was raised. What
remains is processor variance on shared runners, which is real but is a property of the
infrastructure rather than of waybill.

**Consequence to respect when authoring ranges**: m669's bench takes a median of five samples
and still shows fixtures clustered where its 25% threshold sits inside the noise floor. A
single sample on a 99 ms target cannot support a tight bound. Wall-time bounds should be
authored as order-of-magnitude guards (catching a 100 ms → 10 s collapse), not as tight
performance assertions. The report records the raw sample so a future move to multi-sampling
is additive.

**`go-kubernetes` at ~103 s is ~75% of total scan time** and is retained deliberately as the
large-repository benchmark.

---

## R9 — Evaluation and exit policy

**Decision**: Evaluate every measurement on every target before exiting. Exit non-zero if any
violation or any unmeasurable target exists. Distinguish three outcomes per measurement —
`pass`, `fail`, `unmeasured` — and two failure classes overall: configuration errors
(malformed range, duplicate name) and measurement violations.

**Rationale**: FR-018 exists because a first-failure exit hides the blast radius; a maintainer
needs to know whether one target regressed or all eighteen did. Separating "could not measure"
from "measured and out of range" (Principle X, Transparency) prevents an unreachable repository
from being read as a quality regression — the failure mode a naive implementation would produce
by scoring it zero.

**Unranged measurements are observe-only** (FR-020), which is what makes the feature landable
before any range is authored and what lets a new repository be added without immediately
inventing bounds for it.
