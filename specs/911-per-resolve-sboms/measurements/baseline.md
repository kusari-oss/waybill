# Baseline — before any change (2026-09-17)

Fixture: `waybill-cli/tests/fixtures/pants_resolve_edges/`, extended per T001
with `waybill-fixture-common@1.0.0` pinned by **both** resolves at the **same**
version. Its declared dependency `waybill-fixture-shared` is pinned
differently in each — `1.0.0` in `app`, `2.0.0` in `tools` — which is the
FR-011b case.

## The defect, reproduced

```
pkg:generic/app                               ['app']
pkg:generic/tools                             ['tools']
pkg:pypi/waybill-fixture-common@1.0.0         ['app']      <- in BOTH lockfiles
pkg:pypi/waybill-fixture-consumer-a@1.0.0     ['app']
pkg:pypi/waybill-fixture-consumer-b@1.0.0     ['tools']
pkg:pypi/waybill-fixture-shared@1.0.0         ['app']
pkg:pypi/waybill-fixture-shared@2.0.0         ['tools']
```

`waybill-fixture-common` is pinned by `app.lock` **and** `tools.lock`. Its
membership names only `app`. Dedup kept the winner's value and dropped the
other — the #902 item 1 defect, on four lines.

## FR-011b is ALREADY satisfied — and that changes Phase 2

The same run emits **both** edges from the two-resolve component:

```
waybill-fixture-common@1.0.0 -> waybill-fixture-shared@1.0.0
waybill-fixture-common@1.0.0 -> waybill-fixture-shared@2.0.0
```

This was not expected. The reason is an ordering fact neither the spec nor the
plan established:

| line | what happens |
|---|---|
| `scan_fs/mod.rs:1081` | edges emitted, **per entry** |
| `scan_fs/mod.rs:1253` | `deduplicate(components)` |

Edges are built **before** dedup, one pass per `PackageDbEntry`. An entry comes
from exactly one lockfile, so **at edge-emission time a requirer belongs to
exactly one resolve.** The two `common` entries each emit their own edge,
each correctly scoped by #910. Dedup then collapses the two components into
one and drops a resolve from membership — but it never touches relationships.

So the clarify decision (one edge per resolve) describes behaviour the
architecture already produces. What it needs is a **regression test**, not an
implementation.

### What this does to the task list

- **T006 shrinks.** It said "resolve each bare name in EVERY resolve the
  requirer belongs to". At that point in the pipeline a requirer belongs to
  one resolve, always. T006 becomes: handle the array encoding, and do not
  break the per-entry scoping #910 added.
- **FR-011b/SC-007a become verification tasks.** Worth keeping precisely
  because the behaviour is emergent rather than intended — nothing currently
  states it, so nothing currently protects it.

### The risk this moves rather than removes

Membership is unioned at dedup, which is **after** edges are emitted. So at
edge-emission time each entry's membership is a **single-element array**. The
accessor must handle that and must not assume post-dedup plurality. A reader
of the entry-level annotation and a reader of the emitted document are looking
at two different things — one always singular, one possibly plural.

## T003 — the reported monorepo figures

Not reproducible here: the repository is the issue author's and is not
available in this workspace. The figures (2,466 → 1,319 components; 20 of 24
resolves surviving; 4 empty) stand as reported, and #910 and #901 do not touch
membership or dedup, so they are expected unchanged.

This is the check to hand back with a pre-release build rather than one to
fake locally. Recorded so nothing downstream reads it as verified.
