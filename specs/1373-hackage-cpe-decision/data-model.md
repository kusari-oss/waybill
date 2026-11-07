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
| purl type | as `Purl::ecosystem()` returns it — `hackage`, `pypi`, `npm` |
| OSV ecosystem | as OSV's published list spells it — `Hackage`, `PyPI`, `npm` |

**The table holds exactly three rows**: `hackage`, `pypi` and `npm`. Those are
the three for which **both** halves of FR-003 are satisfied against committed
output. A sixteen-row draft was written first, covering every type waybill
emits, and cut after `/speckit.analyze` found that thirteen of them satisfied
list membership alone — the half that asserts coverage rather than measuring
it.

**Cutting them changes no emitted figure.** Measured across all 17 corpus
targets: zero components are classified addressable on an unprobed type,
because `cargo`, `golang`, `maven` and the rest already carry CPEs from
existing arms and never reach that branch. The table is therefore smaller and
every row in it is measured, at no cost to any count.

**Both halves of FR-003 are enforced against this table**, not assumed:

- Each row's OSV name must appear in
  `measurements/osv-ecosystems-2026-11-07.txt`, which is committed.
- Each row must have a probe result in `measurements/` showing that
  ecosystem's advisories returned for a known-vulnerable version.

`generic` has no row. It is in neither the list nor a successful probe — its
single `OSV-2017-79` hit is an OSS-Fuzz finding keyed on a project name, which
is why the rule needs both halves and not either.

A type with no row is `Unaddressable`. For the thirteen cut types this is
**conservative and currently invisible**: it understates nothing today because
none of their components reaches the branch, and the moment one does, the
honest answer is "nobody measured whether a feed indexes this" rather than a
claim made from the ecosystem list alone.

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
