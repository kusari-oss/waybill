# Contract: deps.dev batch client

Feature: `839-batch-enrichment` · Covers FR-001, FR-004/004a, FR-005,
FR-006, FR-007, FR-015.

The upstream surface is `v3alpha`, which its own documentation says "may
change in incompatible ways from time to time". Every clause here is
therefore paired with the behaviour required when it stops holding.

---

## C-1 — Chunking

**C-1.1** A batch request MUST carry at most **5000** entries. The service
returns HTTP 400 above that; this is a documented hard limit, not a
tuning parameter.

**C-1.1a** The *chosen* batch size MUST be approximately **500**, well
below the ceiling (FR-005a). These are two different numbers and
conflating them is the hazard: 5000 is what the service permits, 500 is
what waybill sends. At the ceiling a large scan becomes two requests and
the progress count freezes for the duration of each, which defeats US2
while every test still passes.

**C-1.1b** Batches MUST be issued concurrently under the same ceiling as
the per-component path, so the finer chunking costs no wall-clock time.

**C-1.2** Chunking MUST be driven by the input size, with no assumption
about a maximum. A 60,000-component repository is ~120 chunks, not an
error.

**C-1.3** A 400 response MUST NOT be retried as a batch. It indicates a
malformed or oversized request, and retrying it unchanged is a loop.

*Test*: an input of 1,001 requests produces three chunks at the default
size, none exceeding it; and a configured size above 5000 is rejected or
clamped rather than sent.

---

## C-2 — Pagination

**C-2.1** Paging MUST continue while `next_page_token` is **non-empty**.

**C-2.2** The termination test MUST NOT be field presence. deps.dev
returns `"nextPageToken": ""` on the final page rather than omitting it,
so `Option<String>` deserialises to `Some("")` and a presence check never
terminates.

**C-2.3** A follow-up page request MUST reuse the initial request body
verbatim except for `page_token`, per the API docs' requirement that "all
other request fields must be the same as in the initial request".

**C-2.4** Enrichment MUST NOT be considered complete while a non-empty
token remains. Stopping early produces a plausible-looking SBOM that is
quietly missing enrichment — worse than an error, because nothing signals
it.

*Test*: a recorded two-page fixture yields the union of both pages, and a
single-page fixture whose token is `""` terminates after one request.
This test exists specifically because C-2.2 is the bug it would otherwise
be natural to write.

---

## C-3 — Identity matching

**C-3.1** Responses MUST be matched to requests by the echoed
`responses[].request.versionKey`, never by array position.

**C-3.2** Matching MUST use the **uncanonicalized** key as echoed. The
API documents the echo as uncanonicalized, and deps.dev normalises names
per ecosystem (PEP 503 for PyPI, lowercasing for NuGet, `group:artifact`
for Maven). Re-deriving a key through waybill's own canonicalisation and
matching on that will mismatch wherever the two normalisations differ.

**C-3.3** A response entry with **no `version` field** means deps.dev has
no data. That component is left unenriched; it is not an error and MUST
NOT fail the scan or the batch.

**C-3.4** A requested entry absent from the response entirely MUST be
treated as C-3.3, not as a reason to discard the batch.

*Test*: a fixture mixing a hit, a miss, and a reordered response enriches
exactly the hit and leaves the miss untouched.

---

## C-4 — Fallback

**C-4.0** A failed batch costs only its own entries (FR-005c). With the
FR-005a default that is ~500 components falling back, not 5000.

**C-4.1** On any batch failure — transport error, non-200, unparseable
body — the affected requests MUST fall back to the per-component path.

**C-4.2** The fallback MUST use the **concurrent** per-component path.
Falling back to a sequential one reproduces issue #766, which is
especially likely here because the endpoint is explicitly unstable.

**C-4.3** Fallback MUST be bounded by the same concurrency ceiling as the
normal per-component path. A fallback that fans out harder than the path
it is replacing converts an upstream hiccup into upstream pressure.

**C-4.4** The scan MUST complete. Enrichment may be reduced or absent.

*Test*: an injected transport failure on the batch path yields the same
enrichment content as the per-component path for the same input.

---

## C-5 — Network suppression

**C-5.1** Under `--offline`, **no** request is issued: not batch, not
per-component, not a cache revalidation.

**C-5.2** A cache read is not a network request and remains permitted
under `--offline`. A cached entry past its freshness bound is a miss, and
under `--offline` that miss MUST NOT trigger a fetch — the component is
simply left unenriched.

*Test*: a scan under `--offline` with a populated cache issues zero
requests and still emits whatever the cache can serve.
