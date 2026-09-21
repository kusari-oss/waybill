# Contract: enrichment default and failure behaviour

**Feature**: `923-enrich-batch-default` (#927)
**Consumers**: anyone running a scan with enrichment enabled; anyone reading a document produced by one.

---

## C-1. Enrichment is batched unless asked otherwise

Absent flags, a scan enriches via the batched path. On a repository of ~2,000
packages this is the difference between seconds and minutes.

## C-2. The two paths produce equivalent documents

Identical package identities, identical licence values, identical dependency
edges. **This is what licenses the default change**; without it, "faster" would
mean "different", and the choice of default would be a correctness decision
rather than a performance one.

Asserted by test, not assumed.

## C-3. The slow path stays selectable and exercised

An opt-out selects per-component enrichment. Both paths are covered by tests
that fail if either breaks.

The per-component path is the fallback the default's safety argument rests on.
A fallback that cannot be selected is one nobody tests, and it is what stands
between an upstream API change and a broken scan.

## C-4. The existing opt-in flag keeps working

Scripts passing `--enrich-batch` today continue to run. The flag becomes a
no-op rather than an error.

## C-5. A batch failure costs speed, not coverage

Work falls back to the per-component path and the document's enrichment
content is unchanged. This is milestone 839's existing behaviour and this
feature depends on it.

## C-6. A failure trips the circuit exactly once

After the first batch failure the scan stops attempting the batched path.
A persistent upstream failure costs **one** wasted attempt — not one per
chunk, which on a 2,000-package repository would be ~23.

Achievable because batch requests are issued sequentially (research R2). If
that changes, the guarantee becomes "at most one concurrency group" and the
change must be deliberate.

## C-7. A trip is visible to both audiences

- **The operator, while it happens**: a log line naming the failure.
- **A consumer, afterwards**: the existing document-scope degradation record.

Different audiences, different lifetimes. A log nobody kept does not help
someone reading the document a week later; a document field does not help
someone watching a scan take longer than expected.

## C-8. Disabled enrichment is untouched

`--offline`, deps.dev disabled, or restricted sources: byte-identical output
to before this change. The default flip must not introduce a network call
where there was none.

## C-9. The `v3alpha` premise is watched

A standing check detects the batch endpoint leaving `v3alpha`. This feature
accepts a risk whose justification is that the upstream surface is unstable;
when that stops being true, the trade deserves re-examination, and nobody will
think to look unless something says so.

---

## Verification

| Contract | How verified |
|---|---|
| C-1 | a scan with no flags uses the batched path |
| C-2 | same repository both paths; package identities, licences and edges compared |
| C-3 | the opt-out selects per-component; both paths covered by tests |
| C-4 | a scan passing the old flag succeeds |
| C-5 | with the endpoint failing, enrichment content matches a successful run |
| C-6 | with the endpoint failing, **attempt count is one**, not one per chunk |
| C-7 | the failing scan emits the log line and the document records the degradation |
| C-8 | enrichment-disabled output byte-identical to pre-change |
| C-9 | the check exists and fires on a non-alpha endpoint |
