# Research: Say why deps.dev did not enrich a component

Code facts cite `waybill-cli/src/enrich/` (`E/`). External behaviour is measured (`measurements/`).

## R1 — Which components are attempted

**Found**:
- The only filter is `EnrichmentKey::from_purl_parts` (`E/request_key.rs:37-52`, called at `E/depsdev_source.rs:746-758`). It skips a component when its name or version is empty, or when its ecosystem is not one of deps.dev's six (`E/deps_dev_system.rs:12-25`).
- There is **no** skip for components that already carry licences, for nested components, for main modules, or for placeholder versions. `unknown` and `0.0.0-unknown` are sent and return 404.

**Measured**: guice attempted 46 of 94 maven components in the *initial* pass. That pass runs before graph expansion (`cli/scan_cmd.rs:3857`); the post-graph pass re-visits everything.

**Decision**:
- The spec's `not-needed` reason is removed, because nothing is skipped by design.
- The `not-queried` reasons are `unsupported-ecosystem` (deps.dev does not index the PURL type; document count only, per clarification Q4) and `incomplete-coordinate` (empty or placeholder version, R6).

## R2 — Where outcomes collapse, and two defects FR-010 requires fixing

**Per-key path** (`E/deps_dev_client.rs:259-293`):
- a 404 becomes `Ok(None)`;
- non-2xx responses, network errors and JSON decode errors become `Err`.

In `fetch_many` (`E/depsdev_source.rs:487-519`), an `Err` increments `transport_errors` and becomes `None` in memory. It is not written to disk.

**Batch path**, measured (`measurements/batch_outcomes.txt`):
- a missing version, a missing package, a placeholder version and a malformed maven name all return an item with `request` but **no `version`**;
- a real version returns `version`;
- a transport failure fails the whole chunk, which falls back to per-key requests.

So "absent" is decidable per item, and a malformed query is indistinguishable from a genuine absence at the source, as the issue says.

**Defects** (`E/depsdev_source.rs:208-264`, `:400-411`):
1. **Unanswered slots become absences.** Every slot starts as `None`, and a slot the response never echoes stays `None`. Both caches then store it as a confirmed absence.
2. **Duplicate coordinates.** `index.insert` maps a coordinate to its **last** position, so an earlier duplicate in the same chunk keeps `None`, and that `None` is written to the cache.

**Decision**: track answered slots explicitly.
- A slot is `absent` only when an echoed item without `version` matched it.
- Duplicate coordinates are coalesced to one request, with the answer fanned out to every position.
- An unanswered slot is a transport-level miss: it is neither cached nor recorded as absent, and it goes through the existing per-key fallback.

## R3 — Declined

**Found**: `apply_version_info` (`E/depsdev_source.rs:593-608`) silently drops licence strings that are blank or fail `SpdxExpression::try_canonical` (strict `spdx` parse). A record whose licences are all rejected still counts as `matched` (`:775-787`).

**Decision**: `declined-invalid-license` when the record held at least one non-blank licence string and every one failed canonicalisation. A record with no licence strings at all is `matched` (FR-003: deps.dev answered and had nothing). The rejected strings are logged at debug level, never emitted (Q2).

## R4 — Disk cache

**Found** (`E/deps_dev_disk_cache.rs`):
- the record is `{v: 1, key, fetched_at, max_age_secs, record: Option<VersionInfo>}`;
- `record: null` is a cached 404;
- transport errors are never written;
- any other `v` is a miss.

**Decision**: no format change. FR-010's fix in R2 stops unanswered slots being written as `null`. Existing caches can contain wrong `null`s from defect R2; they expire by `max_age_secs` (default 3600 s). No migration is needed.

## R5 — Two passes

**Found**: the initial pass (`scan_cmd.rs:3857-3871`) and the post-graph pass (`:3974-3991`; only when the graph added components) both visit the whole set. The second is served from the shared in-memory cache, and `apply_version_info` is idempotent.

**Decision**: the outcome is written onto the component as `extra_annotations["waybill:deps-dev-outcome"]` in each pass. A matched outcome removes it, so the final pass's outcome is what is emitted (spec edge case). No separate map is kept.

## R6 — Placeholder versions

**Found**: waybill synthesises:
- `v0.0.0-unknown` (Go workspace, `golang/legacy.rs:1245`);
- `0.0.0-unknown` (12 readers);
- `unknown` (maven design tier, cmake);
- `0.0.0` (nuget main module, kotlin-dsl root, bun workspace member, dart).

An existing placeholder test (`scan_fs/mod.rs:2199-2202`, unexported) covers `""`, `unknown`, `noassertion`, `v0.0.0-unknown`, `none` and `latest`, but **misses `0.0.0-unknown`**.

**Decision**:
- **Shared predicate:** promote that predicate to a shared `is_placeholder_version`, add `0.0.0-unknown`, and use it in `EnrichmentKey::from_purl_parts`, which yields `not-queried:incomplete-coordinate` instead of a request.
- **`0.0.0` excluded:** it is a real version of some packages (npm publishes `0.0.0`), so treating it as a placeholder would suppress real lookups. The readers that synthesise `0.0.0` stay queried, which costs a 404 each and is recorded honestly as `absent`.
- **Existing caller:** `scan_fs/mod.rs`, which owns the predicate today, now uses the shared one. The added `0.0.0-unknown` must not change that caller's output; T-task verifies this with the goldens.

## R7 — Emission and catalogue

**Decision**:
- **C191 `waybill:deps-dev-outcome`** (per component): emitted through the existing `extra_annotations` path (CycloneDX `builder.rs:1715-1760`, SPDX 2.3 `annotations.rs:404-421`, SPDX 3 `v3_annotations.rs:427-438`).
- **C192 `waybill:deps-dev-outcomes`** (document scope): a canonical JSON object of non-zero counts, keys sorted, including `not-queried:unsupported-ecosystem`. Emitted through the C158 path (`metadata.rs:371`, `annotations.rs:538`, `v3_annotations.rs:556`).
- **Both** get parity rows (`SymmetricEqual`). Both are emitted only when the deps.dev pass ran online (FR-007).

## R8 — Tests

- **No base-URL override for the binary.** Adding one would be a new operator-visible surface, so it was rejected.
- **Outcome classification:** in-crate tests against a local mock deps.dev, following the existing `depsdev_source.rs` test pattern (the client's `base_url` is injectable). They cover per-key and batch 404, a batch item without `version`, an unanswered slot, a duplicate coordinate, a transport error, declined licences, a placeholder and an unsupported ecosystem.
- **Emission:** generate-layer tests with component fixtures carrying C191, across all three formats plus C192.
- **Live checks:** SC-002 and SC-005 use `measurements/probe_outcomes.sh` against the real service. They are measurements, not CI tests (Principle VII).
- **Goldens:** byte-identical, because they are generated offline (FR-007), checked by a read-only CI corpus run.
