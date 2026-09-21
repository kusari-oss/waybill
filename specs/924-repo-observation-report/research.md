# Phase 0 — Research: Repo Observation Report

**Feature**: 924-repo-observation-report · **Issue**: #932 · **Date**: 2026-09-21

Every finding below was obtained by reading or running the code, not by
recall. Where a number appears, the measurement that produced it is shown.

---

## R1 — The claimed/unclaimed signal already exists as a return value

**Decision**: Capture the census at `walk_registry::walker::walk_inner`'s
existing per-file dispatch call. Add no traversal.

**Evidence**: `dispatch::dispatch_file` (`walk_registry/dispatch.rs:30`)
already **returns `Vec<ReaderId>`** — the readers that claimed the file — and
`walker.rs:264` already binds it:

```rust
let dispatched_to = { … dispatch::dispatch_file(file, registrations, &ctx, scope) };
self.metrics.tick_file(&dispatched_to);
```

`dispatched_to.is_empty()` **is** "unclaimed". The signal is not merely
derivable; it is a value the walker already computes, names, and passes to a
metrics sink one line later.

**Rationale**: This is the cheapest possible insertion point and it cannot
drift from dispatch behaviour, because it *is* dispatch behaviour.

**Alternatives considered**: A second independent walk (rejected — duplicates
cost and would drift from reader match semantics the moment a reader's
patterns change); re-deriving claims from emitted `PackageDbEntry`
`source_path` values (rejected — loses files that matched a reader which then
produced nothing, which is exactly the FR-004 distinction).

---

## R2 — Determinism (FR-020) is free

**Decision**: No sorting, ordering or normalisation work is required for
FR-020. Inherit the walker's existing guarantees.

**Evidence**, all in `walk_registry/walker.rs`:

- The walk is **sequential** — `walk_inner` recurses in a plain `for` loop
  (line 273); the m772 parallel walker lives in `scan_fs/walker.rs`, a
  different path.
- Directory entries are **sorted before use**: `filenames.sort_unstable()`
  (line 245, contract C3) and `files.sort_unstable()` (line 247).
- Dispatch order is **registration order by contract** — `dispatch.rs:5`,
  contract C1: *"dispatch order is REGISTRATION ORDER."*

**Rationale**: A report assembled in walk order is therefore already
reproducible run to run. The only volatile fields will be the ones we
deliberately add (timestamp, tool version), which FR-020 requires be
enumerated anyway.

**Alternatives considered**: Sorting the assembled report before emission
(rejected as redundant given the above, and it would mask an ordering
regression rather than surface it).

---

## R3 — Significance threshold: **25 unclaimed files**

**Decision**: A directory with no marker, no reader claim and no boundary role
gets its own record once it holds **more than 25 files**. Stated in every
report per FR-021c.

**Measured** across 8 real repositories — this repo, the 2,291-package
polyglot reference repo, and six public-corpus targets. "Recorded" = has a
marker OR exceeds threshold N:

| repo | dirs | w/marker | N=5 | N=10 | **N=25** | N=50 | N=100 |
|---|---|---|---|---|---|---|---|
| waybill | 5118 | 334 | 1146 | 536 | **393** | 351 | 338 |
| polyglot reference | 1187 | 3 | 231 | 79 | **29** | 15 | 6 |
| corpus a | 225 | 16 | 47 | 34 | **23** | 18 | 17 |
| corpus b | 49 | 4 | 14 | 10 | **5** | 4 | 4 |
| corpus c | 29 | 0 | 2 | 0 | **0** | 0 | 0 |

**Rationale**: Read the *size-triggered* records — records beyond the
marker-bearing baseline. For this repo that is 202 at N=10, **59 at N=25**,
and 17 at N=50. N=10 is noise; N=50 is too coarse to surface a large
unclassified blob. N=25 keeps a 5,118-directory repository to 393 records —
7.7% — which is a document a person can read.

**Rationale for making it a stated, tunable value rather than a constant**:
the table above is 8 repositories, all of them ones this project already had
lying around. It is enough to choose a starting value and not enough to call
it correct. FR-021c requires every report to state its threshold precisely so
that when this moves, old reports remain interpretable.

**Note on the zero row**: corpus c records nothing at N≥10 because it has no
markers and no large directories. FR-021a's scan-root clause means the root is
always recorded, so such a repository still produces a report that says, in
effect, "nothing here was recognisable" — which is a true and useful answer.

---

## R4 — Binary vs text (FR-011) with no new dependency

**Decision**: Classify from the first 8 KiB: a NUL byte ⇒ binary; otherwise
valid UTF-8 ⇒ text; otherwise binary. Report the sampled byte count alongside
the verdict.

**Evidence**: no content-sniffing helper exists in `scan_fs/file_tier/`
(searched for `is_binary`, `looks_like_text`, `content_type`, NUL handling —
the only hit is an unrelated `String::from_utf8_lossy` on subprocess output).
`std::str::from_utf8` covers the whole decision.

**Rationale**: Satisfies Constitution Principle I with zero new crates. The
NUL-byte test is the long-standing convention and is what `git` and `grep`
effectively use.

**Alternatives considered**: A content-type detection crate (rejected — a new
dependency for a heuristic the spec already declares as a heuristic);
extension-based inference (rejected — FR-008 already forbids leaning on
extensions for classification, and it would be circular here).

**Carried to the spec's honesty requirement**: the Assumptions section already
states this is a heuristic. The 8 KiB sample bound must be reported so a
reader knows a verdict came from a sample, not the whole file.

---

## R5 — Offline (FR-022) is structural for enrichment, parameterised for resolution

**Decision**: The report path never invokes enrichment or emission, and
constructs resolution with offline semantics **unconditionally** — not exposed
as an operator flag.

**Evidence**: enrichment is invoked at three `else if !offline` call sites in
`cli/scan_cmd.rs` (3670, 3689, 3716). A path that does not reach them cannot
enrich; that half of FR-022b is genuinely structural. Resolution is different:
`golang/graph_resolver.rs` fetches from the module proxy *unless* offline
(`graph_resolver.rs:27`, `:72`), so resolution is network-capable by
construction and must be told not to be.

**This qualifies FR-022b as written.** "Structural rather than
flag-dependent" holds precisely in this sense: the report path hard-codes
offline resolution with no operator-facing switch, so no user input can cause
a network call. It is not true that no code path exists — only that none is
reachable from this command. SC-007 (run with network denied, no offline flag
passed) is what keeps that honest, and it should be understood as guarding a
hard-coded value rather than proving an absence.

**Alternatives considered**: Skipping resolution entirely (rejected — FR-006's
component counts and FR-004's matched-but-produced-nothing distinction both
require it, and those are two of the more diagnostic signals in the feature).

### CORRECTION at implement time

R5 concluded the report path "constructs resolution with offline semantics
unconditionally". **It did not, because there was no way to.** `read_all` takes
no offline parameter, so the report path passed nothing and the Go resolver ran
with its default, network-capable behaviour.

Measured with an uncached module and an empty `GOMODCACHE`:

| | proxy tier | wall clock |
|---|---|---|
| `sbom scan` (control, no forced offline) | attempts fetch, `connection refused` | **5.196s** |
| `repo report` (after the fix) | `proxy_count=0`, falls to gosum | **0.039s** |

Fixed by setting the resolver's own documented gate (`WAYBILL_OFFLINE`,
`graph_resolver.rs:1147`) inside `report::build`. Only *transitive edge*
resolution is affected; components come from readers, so no requirement loses
anything.

**What made this findable was a control.** The report path alone looked fine —
`proxy_count=0` on both sides — because the test module happened to be cached.
Running the same fixture through a command that does *not* force offline is
what separated "did not need the network" from "was never going to use it".

---

## R6 — Seeding the unsupported-ecosystem table (FR-007, FR-009)

**Decision**: Seed with markers verified to have **no** registered reader.
Confirmed absent at the time of writing: `deno.json`, `Pipfile`, `pixi.toml`,
`build.zig`, `shard.yml` (Crystal), `Project.toml` (Julia), `nimble.toml`
(Nim), `dune-project` (OCaml).

**Evidence / method validation**: the same search reported `Package.swift` as
**already handled** (`package_db/swift/manifest.rs`, milestone 122) and it was
therefore excluded. A seeding method that cannot detect an existing reader
would have produced a table claiming waybill does not support Swift.

**Rationale**: every entry is a real, currently-true gap.

**The staleness trap, and the guard for it**: this table is precisely the
shape that rots — a reader lands, and the table keeps announcing the ecosystem
as unsupported. This project has already been bitten by that class of bug
(recorded as the enum-allowlist finding, where a `matches!` allowlist silently
missed variants added later and survived roughly two years).

**Therefore a required test, not a convention**: assert that no entry in the
unsupported-ecosystem table matches any pattern in the live reader registry.
The table and the registry are both enumerable in-process, so the check is
cheap and it fails the moment a new reader makes an entry obsolete.

---

## R7 — Schema validation tooling (FR-016, SC-006)

**Decision**: Publish a JSON Schema and validate emitted reports against it in
tests using the existing dev-dependency.

**Evidence**: `jsonschema = { version = "0.46", default-features = false }` is
already a `waybill-cli` dev-dependency (`Cargo.toml:218`), already used to
validate SPDX 2.3, SPDX 3.0.1 and OpenVEX output against vendored schemas.

**Rationale**: Zero new dependencies; an established in-repo pattern for
exactly this job.

**Carried warning from prior experience**: a schema gate that stubs `$ref`
resolution validates nothing while appearing green — this project has hit that
before. The SC-006 test must be proven to have teeth by reintroducing a
deliberate violation and observing a failure, not merely by observing a pass.

---

## Summary of decisions

| # | Decision | New dependency |
|---|---|---|
| R1 | Capture at the existing `dispatch_file` return value | none |
| R2 | Inherit walker determinism (sequential + sorted + C1) | none |
| R3 | Significance threshold **25**, stated per report | none |
| R4 | NUL-byte then UTF-8 over an 8 KiB sample | none |
| R5 | Enrichment never invoked; resolution hard-coded offline | none |
| R6 | Seed table with 8 verified-absent markers + anti-staleness test | none |
| R7 | JSON Schema validated by the existing `jsonschema` dev-dep | none |

**Zero new Cargo dependencies at any level.**
