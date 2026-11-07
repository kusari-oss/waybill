# Phase 1 data model

**Feature**: `1373-hackage-cpe-decision` | **Issue**: #1375

No persistent state. One enum, one struct replacing another, one committed
table of observations. The CPE synthesizer is **not** touched.

---

## 1. `FeedAddressability` — the new classification

```text
                        has CPEs?
                       ┌─── yes ──► WithCandidates
   ResolvedComponent ──┤
                       └─── no ───► PURL type in OSV's declared list?
                                      ├── yes ──► AddressableWithoutCpe
                                      └── no ───► Unaddressable
```

A **closed enum with an exhaustive match**, not an allowlist that is consulted
and defaulted. Issue #892 survived roughly two years behind an allowlist that
fell out of date silently; the same shape here would let a new PURL type be
counted `Unaddressable` with nobody noticing it was never considered.

| variant | meaning |
| --- | --- |
| `WithCandidates` | carries at least one CPE, from any source |
| `AddressableWithoutCpe` | no CPE, but a feed indexes this PURL type |
| `Unaddressable` | no CPE, and no feed indexes this PURL type |

**The three partition every component.** Not the subset passing
`is_release_shaped`, which is what leaves 24 components in no count today.

### Why `WithCandidates` wins over addressability

A component with a CPE *and* an ecosystem PURL is in `WithCandidates` only.
72 `pkg:pypi` components on the pinned closure are in exactly that position.
Counting them twice would break FR-002's sum; counting them as addressable
would hide that the synthesizer served them.

---

## 2. `OSV_INDEXED_PURL_TYPES` — the committed observation

A table mapping a PURL type to the OSV ecosystem that indexes it.

| field | notes |
| --- | --- |
| purl type | as `Purl::ecosystem()` returns it — `hackage`, `pypi`, `golang` |
| OSV ecosystem | as OSV's published list spells it — `Hackage`, `PyPI`, `Go` |

**Both halves of FR-003 are enforced against this table**, not assumed:

- Each row's OSV name must appear in
  `measurements/osv-ecosystems-2026-11-07.txt`, which is committed.
- Each row must have a probe result in `measurements/` showing that
  ecosystem's advisories returned for a known-vulnerable version.

`generic` has no row. It is in neither the list nor a successful probe — its
single `OSV-2017-79` hit is an OSS-Fuzz finding keyed on a project name, which
is why the rule needs both halves and not either.

> **This table is a dated observation, not a truth.** OSV adds ecosystems. A
> row added without re-probing is the defect `probe-cpe-dictionary.sh` was
> written to prevent, one feed over.

---

## 3. `CpeSynthesisCoverage` — replaced, same carrier

| before | after |
| --- | --- |
| `with_candidates: usize` | `with_candidates: usize` — unchanged meaning |
| `eligible_unmapped: usize` | **removed** |
| — | `addressable_without_cpe: usize` |
| — | `unaddressable: usize` |

Wire form, schema bumped because the shape changes incompatibly:

```json
{
  "schema": "waybill-cpe-coverage/v2",
  "with_candidates": 78,
  "addressable_without_cpe": 166,
  "unaddressable": 733
}
```

**Invariant**: the three sum to the component count. Asserted, not commented.

### Why replace rather than add alongside

`eligible_unmapped` is wrong in both directions — it overstates the Haskell
share and understates the real gap by a factor of 2.6 — so keeping it would
mean emitting a number this feature exists because it is misleading. The
schema version is what tells a consumer the shape changed.

---

## Relationship to existing entities

```text
ResolvedComponent.cpes ────┐
ResolvedComponent.purl ────┼──► FeedAddressability ──► CpeSynthesisCoverage
OSV_INDEXED_PURL_TYPES ────┘                                   │
                                                               ▼
                                              ScanArtifacts.cpe_coverage
                                                               │
                                         ┌─────────────────────┼─────────────┐
                                         ▼                     ▼             ▼
                                    CDX metadata        SPDX 2.3 doc    SPDX 3 doc
                                     properties          annotation      annotation
```

**Not touched**: `synthesize_cpes` and every arm in it, `GENERIC_LIBRARY_CPES`,
`is_release_shaped` (it still gates *synthesis*; it simply stops gating the
*metric*), `fill_missing_cpes`'s assignment behaviour, and every component's
`cpes` array.
