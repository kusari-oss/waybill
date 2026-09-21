# Phase 0 — Research

**Feature**: `923-enrich-batch-default` (#927)

---

## R1 — The default is one line, and the blast radius is small

Three sites reference the flag:

```
scan_cmd.rs:1021   pub enrich_batch: bool          #[arg(long)]  — bare flag, no default_value
scan_cmd.rs:3617   .with_batch(args.enrich_batch)  — the single consumer
scan_cmd.rs:6057   enrich_batch: false             — the test-helper default
```

`#[arg(long)]` on a `bool` is clap's flag form: absent = `false`. Flipping the
default therefore cannot be done by changing a `default_value`; the flag has
to become an opt-**out** whose absence means "batched".

**Decision**: invert to an opt-out flag, and keep the existing opt-in accepted
as a no-op so scripts passing it today do not break (FR-004).

**Alternatives considered**: a tri-state `--enrich-mode=batch|per-component`,
rejected as a larger CLI surface than the decision warrants; removing the old
flag, rejected because it breaks callers for no benefit.

## R2 — **The concurrency the code appears to have does not exist**

The batch loop groups chunks by `CONCURRENT_REQUESTS` and then awaits them one
at a time:

```rust
for group in chunks.chunks(CONCURRENT_REQUESTS) {
    for idxs in group { results.push(async move { … }); }
    for fut in results { let (idxs, got) = fut.await; … }   // sequential
}
```

Rust async blocks are lazy. Collecting them and awaiting in a loop runs them
strictly in order. There is no `join_all`, `FuturesUnordered`,
`buffer_unordered` or `spawn` anywhere in the module.

**Corroborated independently by timing**: 2,291 packages is 23 chunks at
`BATCH_SIZE = 100`; enrichment took ~6.3s of an 8.5s scan, i.e. **~0.27s per
chunk** — one round-trip each. Eight-way concurrency would predict ~0.8s.
Code structure and measurement agree.

**Consequences for this feature**:

1. A batch failure is observed *before the next request is issued*, so the
   circuit breaker can be exact: **one** wasted attempt, not one per chunk and
   not one per concurrency group. The clarification that weakened this was
   based on a misreading and has been withdrawn in the spec.
2. `depsdev_source.rs:187` documents concurrency "bounded at
   `CONCURRENT_REQUESTS`", true only in the sense that 1 ≤ 8. Left alone here;
   filed as **#929**.
3. ~6.3s of the 8.5s is sequential network wait. Out of scope — this feature
   is a default flip, and turning it into a concurrency milestone would put
   the win behind unrelated risk.

**This is the finding that most changed the plan**, and it was only visible by
reading how the futures are *driven* rather than how they are *grouped*.

## R3 — Detecting the `v3alpha` premise expiring (FR-007c)

Two flavours, and they catch different things:

| approach | catches | misses |
|---|---|---|
| A test pinning the endpoint to `v3alpha` | **our** change to the URL, forcing it to be deliberate | upstream graduating; we would never look |
| A scheduled probe for a stable endpoint | **upstream** graduating | nothing relevant |

FR-007c asks for the second — the risk being managed is upstream's, not ours.
The project already has this shape twice (the bpf-linker canary, the corpus
canary), both scheduled workflows that open a deduped issue on a signal.

**Decision**: a scheduled check that probes whether a non-alpha batch endpoint
exists and opens an issue when one does. The pinning test is the cheap floor
and worth having as well, but on its own it does not satisfy FR-007c.

**Note**: `v3alpha` also appears at `hash_resolver.rs:65` for a different
endpoint. A graduation signal should say *which* surface moved, or the next
reader will assume both did.

## R4 — The degradation record already exists and already reaches the document

`depsdev_source.rs:650` records `DegradationMode::BatchUnavailable`, which
flows to catalogue row C158 at document scope. The enum's own documentation
states the invariant this feature depends on — *enrichment content is
unaffected; this costs speed, not coverage*.

**Nothing to build for FR-007.** What changes is *when* it can appear: with
the batched path off by default, no default scan could emit it; after the
flip, a default scan can. That is a consumer-visible difference confined to
the failure path, and it is strictly more information.

**Corpus goldens are unaffected** — the harness hard-codes `--offline`
(`corpus_harness_195/harness.rs:184`, "Corpus scans MUST NOT hit the
network"), so enrichment never runs there. Verified rather than assumed,
because a nondeterministic annotation would have broken the gate repaired in
#918/#921/#922/#923.

## R5 — The equivalence test already exists

`depsdev_source.rs:1168` — `batch_failure_falls_back_and_content_is_unchanged`.
The test helper `src(server, batch)` is already parameterised on the path, so
both can be exercised without new scaffolding.

**Decision**: extend the existing parameterisation rather than build a second
harness. FR-005's "both paths exercised" is mostly a matter of making the
existing coverage explicit and adding the circuit-breaker case.

## R6 — What is NOT established

~~The 154× figure comes from **one** repository, on one network, on one day.~~

**Superseded by T021, and the caution here did not go far enough.** The 154×
figure was not merely un-generalisable — it was **misattributed**. The 1302s
it rests on is ~98% ClearlyDefined, not deps.dev (**#930**). The measured
deps.dev effect is **2.4x on the enrichment phase and 100x on request count**,
from paired runs on that same single repository — so the "one repository, one
network, one day" caveat still applies to the 2.4x, and the request-count
bound is the one that does not depend on the day.

The lesson this section should have carried: a figure derived by *toggling a
flag* needs the per-source timings the tool already logs before it is quoted
at all. Un-generalisable and wrong are different failures, and only the
second one was present.

The honest general claim: **per-component enrichment issues one request per
component; batched issues one per hundred.** The ratio on any given repository
follows from its package count, not from this measurement.
