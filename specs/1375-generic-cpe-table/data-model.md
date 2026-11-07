# Phase 1 data model

**Feature**: `1375-generic-cpe-table` | **Issue**: #1381

No new types, no new state, no schema change. Two `const` tables grow and one
block of recorded decisions is added beside them.

---

## 1. `GENERIC_LIBRARY_CPES` — 12 rows become 60

```rust
const GENERIC_LIBRARY_CPES: &[(&str, &[(&str, &str)])] = &[ … ];
//                             slug    vendor  product
```

| | now | after |
| --- | --- | --- |
| rows | 12 | **60** |
| rows emitting >1 candidate | 1 (`curl`) | **10** |

The shape is unchanged. The slug is matched by **exact lowercase component
name**, and that stays — no aliasing, no prefix stripping. None of the 48
accepted names needs it, so adding the mechanism would be speculative.

### Validation rules

- **V1 — every pair has a recorded observation.** Enforced offline by
  `every_table_pair_exists_in_the_nvd_dictionary` (#1246). A row without one
  is a test failure, not a silent zero.
- **V2 — every pair is title-confirmed.** Not machine-checkable; the evidence
  lives in `research.md` R4 and `measurements/nvd-titles.txt`. This is why
  V1 alone is insufficient: `ninja` would pass V1 three times over.
- **V3 — the vendor comes from the dictionary.** Never the component name by
  default. `bash` is `gnu`, not `bash`.

---

## 2. `NVD_DICTIONARY_OBSERVED` — `(vendor, product, entries)`

Grows by one entry per new pair. The counts are the observation, not an
invariant — the test asserts a pair *was seen*, which is the right strength
for a dictionary that grows.

> Dictionary presence is **not** advisory coverage. A row makes a component
> matchable; whether any advisory exists is a different question, and the
> existing header already says so.

---

## 3. The rejections — recorded in source, not only in a spec

```text
// NOT in the table, and deliberately (milestone 1375, #1381):
//
//   ninja      126 entries, all other products: "Ninja Framework Ninja",
//              "ITRS Group Ninja", "Visualshortcodes Ninja … for WordPress".
//              The build tool is absent from NVD.
//   cmake      the only vendor is `cmake_project`, "cmake … for Node.js",
//              an npm package. `kitware:cmake` returns 0.
//   libxcrypt  one entry, vendor `elektrobit`. Too thin to attribute.
```

**Why in the source.** A contributor adding `ninja` is editing this file; a
note in a spec directory is not where they will look. Both names are
*attractive* — 126 and 5 entries, and the name matches — so the next person
re-derives the question and may answer it differently.

A test asserting `ninja` yields no CPE is also added. The test says *what*;
the comment says *why*; neither substitutes for the other.

---

## 4. What a component's output looks like

Nothing structural changes. A component named `bash` at version `5.3` moves
from

```json
{"name": "bash", "purl": "pkg:generic/bash@5.3"}
```

to the same thing plus

```json
{"cpe": "cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*"}
```

with any additional candidates in the existing `waybill:cpe-candidates`
property (C19), exactly as `curl`'s two candidates already behave.

---

## Relationship to existing entities

```text
ResolvedComponent.name ──► GENERIC_LIBRARY_CPES lookup (exact, lowercase)
                                   │
                                   ▼
                         synthesize_cpes "generic" arm
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                             ▼
          component.cpes (primary)      C19 waybill:cpe-candidates
                    │
                    ▼
          classify_addressability  ──►  WithCandidates (was Unaddressable)
                    │
                    ▼
               C209 counts: with_candidates 78 → 127
                            unaddressable  194 → 145
```

**Not touched**: `is_release_shaped`, `OSV_INDEXED_PURL_TYPES`, the C209
schema (still `/v3`), the member and file-tier exclusions, every other
ecosystem arm, and every component outside the 48 accepted names.
