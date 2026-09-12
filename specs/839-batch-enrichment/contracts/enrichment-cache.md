# Contract: enrichment disk cache

Feature: `839-batch-enrichment` · Covers FR-011, FR-012/012a/012b/012c,
FR-013, FR-014.

This is the first persistent state on the enrichment path. The governing
risk is not slowness but **silent staleness**: deps.dev offers no `ETag`,
so a cache that serves an outdated record produces an SBOM that looks
entirely correct and is not.

---

## C-6 — Key and location

**C-6.1** The key is the enrichment request identity — system, name,
version — hashed.

**C-6.2** The key MUST be constructed identically on the batch and
per-component paths, or the two will write and miss different entries for
the same package.

**C-6.3** Location is `$HOME/.cache/waybill/deps-dev/`, sibling to the
existing `$HOME/.cache/waybill/clearly-defined/`.

**C-6.4** A stored entry MUST record enough of its key to detect a hash
collision, and MUST refuse a mismatched entry rather than serve it. The
sibling cache already does this.

---

## C-7 — Freshness

**C-7.1** An entry is usable only while `now - retrieved_at < max_age`.

**C-7.2** `max_age` MUST come from the originating response's
`Cache-Control: max-age` directive, defaulting to **one hour** when the
header is absent or unparseable.

**C-7.3** `max_age` MUST be stored per entry, not applied globally at read
time. If deps.dev changes its policy, existing entries keep the bound
they were fetched under and no migration is needed.

**C-7.4** An expired entry is a miss and MUST be re-fetched (FR-012c).
There is no "serve stale" path at read time.

**C-7.5** FR-012b's extend flag MUST act at fetch time, raising the
`max_age` written into the entry — never by overriding the C-7.1 check.
An operator's decision to accept staleness is then recorded in the data
rather than applied invisibly at every later read.

**C-7.6** A recorded absence MUST be cached and MUST expire under the same
rules. Not caching absences leaves the long tail of packages deps.dev does
not carry being re-requested every scan, which on a large repository is
most of the traffic this feature exists to remove.

*Test*: an entry written with `max_age` of one hour and a `retrieved_at`
of two hours ago reads as a miss; the same entry at thirty minutes reads
as a hit.

---

## C-8 — Failure and isolation

**C-8.1** A corrupt, truncated, or unreadable entry is a **miss**, never
an error (FR-013).

**C-8.2** Writes are best-effort. A read-only or full cache directory
degrades speed, never correctness, and MUST NOT fail the scan.

**C-8.3** Writes MUST be atomic — write-then-rename, not write-in-place —
so an interrupted scan cannot leave a partial entry that a later scan
would read as valid.

**C-8.4** A disabled cache MUST miss on every read and drop every write,
with no special-casing at call sites.

**C-8.5** Tests MUST point the cache at a per-test temporary directory.
A test that touches the real `$HOME` shares mutable state with the
developer's other scans and with every other test — Constitution
Principle VII, and the reason the sibling cache's tests are written the
same way.

*Test*: a deliberately corrupted entry yields a miss and a completed
scan; a cache directory made unwritable yields a completed scan.
