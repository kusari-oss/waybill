# Phase 1 Data Model: Nightly SBOM Quality Regression Corpus

Five entities: **CorpusConfig** (what to measure and what is acceptable), **Target** (one
repository), **Expectations** (hand-authored bounds), **TargetMeasurement** (what was
observed), and **QualityReport** (the run's complete record). Wire formats are specified in
[contracts/](./contracts/).

---

## 1. CorpusConfig

Parsed from `xtask/corpus/quality-corpus.toml`. Top-level, one per repository checkout.

| Field | Type | Notes |
|---|---|---|
| `sbomqs_version` | `String` | Exact expected version, e.g. `"v2.0.6"`. FR-015 — a mismatch is reported, not silently tolerated. MUST match the version `.github/workflows/ci.yml` installs. |
| `default_timeout_secs` | `u64` | Per-target scan budget. Default 600. Overridable per target (FR-014). |
| `targets` | `Vec<Target>` | The corpus. MUST be non-empty; names MUST be unique (FR-004). |

**Validation at parse time** (all are configuration errors, distinct from measurement
violations per FR-021):
- duplicate `Target::name`
- any `Range` whose `min > max`
- any negative or non-integer count bound
- `sbomqs_version` empty or malformed

---

## 2. Target

| Field | Type | Notes |
|---|---|---|
| `name` | `TargetName` (newtype over `String`) | Unique, stable, used as the cache directory and report key. Never derived from the URL, so re-pointing a URL does not orphan history. |
| `url` | `String` | Clone location. |
| `pin` | `Pin` | See below. |
| `ecosystem` | `String` | Documentation only — never gates. Lets a reader see coverage at a glance. |
| `timeout_secs` | `Option<u64>` | Overrides `default_timeout_secs`. |
| `expect` | `Option<Expectations>` | Absent ⟹ observe-only (FR-020). |

### Pin

```rust
enum Pin {
    Sha { hex: String },     // 40-char lowercase hex — today's only variant
    Ref { name: String },    // FR-003 forward compatibility; NOT implemented this milestone
}
```

`Pin` is an enum from the outset purely so that switching a target to a moving branch later is
a configuration change plus one match arm, never a schema migration (FR-003). Only `Sha` is
implemented now; `Ref` MUST be rejected at parse time with a message saying it is not yet
supported, rather than silently ignored.

---

## 3. Expectations

Every field is optional. An absent field means that measurement is observed but cannot fail
the run (FR-020). This is what makes the feature landable before any range is authored.

| Field | Type | Gates |
|---|---|---|
| `wall_ms` | `Option<Range>` | Scan duration. Author loosely — see research R8. |
| `sbomqs` | `Option<RangeF>` | Overall CycloneDX score, 0.0–10.0. |
| `pkgs` | `Option<Range>` | Package-tier component count (components with a `purl`). |
| `files` | `Option<Range>` | File-tier component count (components without one). |
| `edges` | `Option<Range>` | Total relationship count. |
| `max_depth` | `Option<Range>` | Greatest distance from the root component. |
| `flat` | `Option<bool>` | `true` ⟹ expected flat; `false` ⟹ expected not flat (FR-022). |

```rust
struct Range  { min: u64, max: u64 }   // inclusive both ends (FR-017)
struct RangeF { min: f64, max: f64 }   // inclusive both ends
```

Both are newtypes with a validating constructor rejecting `min > max` (Constitution IV), so an
inverted range cannot exist at runtime.

`graph_completeness` is deliberately **not** rangeable. It is recorded for comparison
(FR-013) but never gates — gating on waybill's self-report is precisely the mistake research
R3 documents.

---

## 4. TargetMeasurement

One per target per run.

| Field | Type | Notes |
|---|---|---|
| `name` | `TargetName` | |
| `status` | `MeasurementStatus` | `Measured` \| `Unmeasurable { reason }` |
| `wall_ms` | `Option<u64>` | Scan only — excludes fetch, scoring, analysis (FR-009). |
| `sbomqs` | `Option<HashMap<String, f64>>` | Keyed by format name. Only `"cyclonedx"` populated this milestone; the map shape is what makes FR-030 additive. |
| `pkgs` / `files` | `Option<u64>` | |
| `edges` | `Option<u64>` | Sum of `dependsOn` lengths. |
| `nodes_with_out_edges` | `Option<u64>` | Components having ≥1 outgoing relationship. |
| `max_depth` | `Option<u64>` | BFS depth from `metadata.component.bom-ref`. |
| `flat` | `Option<bool>` | Derived: `max_depth <= 1`. |
| `graph_completeness` | `Option<String>` | waybill's self-report, verbatim. Recorded, never gated. |
| `sbom_bytes` | `Option<u64>` | Emitted document size — free to collect, useful context. |

```rust
enum MeasurementStatus {
    Measured,
    Unmeasurable { reason: UnmeasurableReason },
}

enum UnmeasurableReason {
    FetchFailed { detail: String },   // FR-007
    ScanFailed  { detail: String },
    ScanTimedOut { budget_secs: u64 },// FR-014
    ScoringFailed { detail: String },
    NoDocumentEmitted,
}
```

`Unmeasurable` is a distinct status rather than zeroed measurements — Principle X. A repository
that could not be fetched must never read as "quality collapsed to zero".

### Flatness derivation

```text
adj        = { dep.ref -> dep.dependsOn } from the document's dependencies[]
edges      = Σ |dependsOn|
nodes_out  = |{ ref : dependsOn non-empty }|
max_depth  = greatest BFS depth from metadata.component.bom-ref over adj
flat       = max_depth <= 1
```

A document with no root component, or with an empty `dependencies[]`, yields `max_depth = 0`
and is therefore flat. That is correct: nothing hangs off anything.

---

## 5. Violation and QualityReport

```rust
struct Violation {
    target: TargetName,
    metric: MetricKind,       // WallMs | Sbomqs { format } | Pkgs | Files | Edges | MaxDepth | Flat
    expected: ExpectedBound,  // Range | RangeF | Flat(bool)
    observed: ObservedValue,  // U64 | F64 | Bool
}
```

| QualityReport field | Type | Notes |
|---|---|---|
| `schema_version` | `u32` | `1`. |
| `waybill_sha` | `String` | Which build produced these numbers (FR-025). |
| `corpus_sha` | `String` | Which corpus revision (FR-025). |
| `sbomqs_version` | `String` | Version actually invoked, not merely expected. |
| `started_at` / `finished_at` | RFC 3339 | |
| `runner` | `String` | `uname`-style host descriptor, as m669's bench records. |
| `measurements` | `Vec<TargetMeasurement>` | Sorted by `name` (FR-026). |
| `violations` | `Vec<Violation>` | Sorted by `(name, metric)` (FR-026). |
| `config_errors` | `Vec<String>` | Malformed ranges, duplicate names (FR-021) — kept separate from `violations`. |

---

## 6. The corpus at first landing

Eighteen targets. **Ships with no `[expect]` blocks** — every measurement is observe-only until
a maintainer authors bounds (FR-020, and the operator's explicit choice of hand-authored
ranges over automatic capture). The observed values below are recorded as TOML comments beside
each target so the author has them inline while writing bounds.

Pins resolved via `git ls-remote --tags --refs` on 2026-09-03.

| name | repository | tag | pinned sha | eco |
|---|---|---|---|---|
| `go-cobra` | spf13/cobra | v1.9.1 | `a655097faf7d54f78933a815984b9919d51a05d2` | go |
| `go-kubernetes` | kubernetes/kubernetes | v1.37.0 | `157e582fcc3ebba3c22b16721f49d6890f784c1f` | go |
| `rust-ripgrep` | BurntSushi/ripgrep | 14.1.1 | `0e8390a66fbcf6eeac1aeb0541b367663a597c79` | cargo |
| `rust-zizmor` | zizmorcore/zizmor | v1.30.0 | `fb814d6687450fc8e0b0fba8d958b1ac40c0647f` | cargo |
| `npm-express` | expressjs/express | v5.1.0 | `e99649895f714c9dc9b3538e2cb0f58954f0ecfa` | npm |
| `pnpm-vue-core` | vuejs/core | v3.5.42 | `d63616ca17de965ed32dcb449a4c5cd9982f15d2` | pnpm |
| `yarn-guac-visualizer` | guacsec/guac-visualizer | v0.6.4 | `cd322cd47518f37ff6ec8a24377143eeb911e2e7` | yarn |
| `python-flask` | pallets/flask | 3.1.2 | `80be49be88b534d2a72ef6bf5ea4aabf89f3305b` | pip |
| `python-ansible` | ansible/ansible | v2.21.3 | `6e7ec0333c89b258753534ec68caff167345079c` | pip |
| `python-pytorch` | pytorch/pytorch | v2.14.0 | `2b3ec34829036a65cd9d1398ea72a0167dc37470` | pip |
| `uv-meilisearch-python` | meilisearch/meilisearch-python | v0.50 | `8147a9dcff97da126663360588229079ab8400c8` | uv |
| `pants-backend-ai` | lablup/backend.ai | 25.19.1 | `809fcd394dd8e39456986dd742e7d51c6aedd647` | pants+uv |
| `maven-guice` | google/guice | 7.0.0 | `b0e1d0fab0167cd555ab8d262333c1a32db7d492` | maven |
| `gradle-apache-solr` | apache/solr | releases/solr/10.0.0 | `28c1bc1f9394be8d6187a3550db964c49a9a8b0a` | gradle |
| `gradle-bitwarden-android` | bitwarden/android | v2026.8.1-bwa | `d817f6b4bf7c17172a74fabca1e09e738c7ec6c9` | gradle |
| `ruby-rails` | rails/rails | v8.1.3 | `90588c21894456d979d7195502e6f5918f8d59ea` | gem |
| `cmake-nlohmann-json` | nlohmann/json | v3.12.0 | `65ee68451d8eb2b5f3a30b410476ab83deb3289b` | cmake |
| `cpp-mongo` | mongodb/mongo | r8.3.8 | `41a5752480dd86b432e7d31e8150847002e16924` | bazel/polyglot |

Observed values for every target are tabulated in [research.md § R7](./research.md).

**Authoring notes to carry into the config comments**

- `npm-express`, `python-ansible`, `uv-meilisearch-python` are permanently flat because the
  upstream projects commit no lockfile. Author `flat = true`; a change to `false` would be
  news, not a failure.
- `python-ansible` yields only 11 package-tier components against 375 file-tier. Its `pkgs`
  bound must be small and its `files` bound large — the inverse of most targets.
- `python-pytorch` has empty `third_party/` sub-repositories by design (research R6). Its
  counts are lower than a working copy's; do not "correct" them.
- `go-kubernetes` at ~103 s needs a wall-time bound roughly an order of magnitude wide.
- `sbomqs` bounds live in a narrow 5.75–7.70 band across the whole corpus; ±0.5 is a
  meaningful width here, ±25% is not.
