# Phase 1 data model

**Feature**: `1374-exclude-members-from-coverage` | **Issue**: #1380

No persistent state. Two new enum variants, two new counts, one moved
computation. The classifier's shape is unchanged; what changes is that it now
describes every component rather than only the ones it had an opinion about.

---

## 1. `FeedAddressability` — two variants, not a pre-filter

```text
                       is it a closure member?  ──yes──► ExcludedMember
   ResolvedComponent ──┤
                       is it file-tier?         ──yes──► ExcludedFile
                       │
                       └─ has CPEs? ──yes──► WithCandidates
                          │
                          └─ PURL type indexed? ─yes─► AddressableWithoutCpe
                                                └─no──► Unaddressable
```

| variant | meaning |
| --- | --- |
| `WithCandidates` | carries at least one CPE |
| `AddressableWithoutCpe` | no CPE, but a feed indexes this PURL type |
| `Unaddressable` | no CPE, and no feed indexes this PURL type |
| **`ExcludedMember`** | a closure member — a patch, tarball or build fragment |
| **`ExcludedFile`** | a file-tier entry |

**Variants rather than a filter**, and the distinction is load-bearing. A
filter applied before classification leaves the enum describing a population
it no longer covers, so the exhaustive match stops being a guarantee that
every component was considered. With variants, a component that reaches the
classifier always comes out of it, and the match still proves nothing was
silently dropped — which is the reason it is an enum at all (#892).

### Precedence, and why exclusion comes first

**Membership wins over everything, including a CPE.** FR-003: a member is
excluded because of what it *is*, not because of what was or was not
synthesized for it. The synthesizer's guard means no member carries a CPE
today — 0 of 539 on darwin, 0 of 1,087 on linux — but if that guard changed,
the member must still be excluded rather than silently re-entering the
partition through `with_candidates`.

**Membership also wins over addressability**, and that is not hypothetical:
`pkg:hackage/time` on the linux closure is a member *and* carries an
OSV-addressable PURL. It is excluded.

---

## 2. `CpeSynthesisCoverage` — three counts become five

| before (`/v2`) | after (`/v3`) |
| --- | --- |
| `with_candidates` | `with_candidates` — unchanged meaning |
| `addressable_without_cpe` | `addressable_without_cpe` — unchanged meaning, smaller population |
| `unaddressable` | `unaddressable` — unchanged meaning, much smaller population |
| — | **`excluded_members`** |
| — | **`excluded_files`** |

```json
{
  "schema": "waybill-cpe-coverage/v3",
  "with_candidates": 78,
  "addressable_without_cpe": 166,
  "unaddressable": 194,
  "excluded_members": 539,
  "excluded_files": 0
}
```

**Invariant**: the five sum to the emitted component count — not to a
filtered subset the reader must derive. Asserted, not commented.

> `with_candidates` keeps its meaning across both the `/v2` and `/v3` bumps.
> It is the one field a consumer could have relied on since milestone 1367,
> and nothing here touches it.

---

## 3. Where the record is built — a move, not a new structure

The computation moves from **`scan_cmd.rs:4914`** to after
**`scan_cmd.rs:5315`**, the only window in which both excluded populations
exist:

```text
4907  fill_missing_cpes(&mut components)     ← still synthesizes here
4914  (was: cpe_coverage.wire())             ← the counts move out
5213  FileInventoryMode::parse(...)
5315  components.extend(file-tier entries)   ← the second population appears
      ↓ counts computed here, over the final vector
5536  ScanArtifacts { cpe_coverage: ... }
```

Synthesis stays at 4907 — it must, because it mutates `c.cpes` and later
passes read them. Only the **counting** moves, which is why `with_candidates`
can keep its meaning: the set of components carrying a CPE is identical at
both points.

---

## Relationship to existing entities

```text
ResolvedComponent.extra_annotations
    ├── waybill:closure-member-role ──► ExcludedMember
    └── waybill:component-tier=file ──► ExcludedFile
ResolvedComponent.cpes ─────────────► WithCandidates
ResolvedComponent.purl ─────────────► Addressable / Unaddressable
                                              │
                                              ▼
                                   CpeSynthesisCoverage (5 counts)
                                              │
                                   ScanArtifacts.cpe_coverage
                                              │
                        ┌─────────────────────┼─────────────────┐
                        ▼                     ▼                 ▼
                  CDX metadata          SPDX 2.3 doc      SPDX 3 doc
```

**Not touched**: `synthesize_cpes` and every arm, `GENERIC_LIBRARY_CPES`,
`OSV_INDEXED_PURL_TYPES`, `is_release_shaped`, the member classifier, the
file-tier walker, and every component's identifiers.
