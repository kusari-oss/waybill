# Phase 1 data model: a CPE for the closure components a feed could match

**Feature**: `1367-closure-component-cpes` | **Issue**: #1371

This feature adds no new persistent state and one new in-process entity. It
populates a field (`ResolvedComponent.cpes`) that already exists and that three
emitters already read.

---

## 1. `ReleaseVersion` (a predicate, not a struct)

The FR-003 gate: is this version string something NVD could match?

**Why it is not a type**: the answer is used once, at one call site, to decide
whether to call the synthesizer. Introducing a newtype would push a parse
through five emitters that currently take `String`, for no gain this feature
can measure.

### The rule

A version is release-shaped when it consists of digits and separators and
nothing else — `3.6.2`, `1.3.2`, `10.46`, `21.1.8`, and the letter-suffixed
form CPE already accommodates (`1.0.2k`).

### Measured against the pinned closure

| version | shape | decision |
| --- | --- | --- |
| `3.6.2` (openssl) | release | **synthesize** |
| `8.20.0` (curl) | release | **synthesize** |
| `10.46` (pcre2) | release | **synthesize** |
| `21.1.6+apple-sdk-26.4` (libcxx) | build metadata | decline |
| `20251016-3.1` (libedit) | date-led | decline |
| `0-unstable-2018-06-17` (CoreSymbolication) | unstable marker | decline |
| `26.05pre-git` (nixpkgs hooks) | pre-release marker | decline |
| `boot` (bootstrap stage) | not a version | decline |

**Validation note**: the declined rows are not hypothetical. All five shapes
exist in the measured closure today, and none of them is currently mapped — so
the guard changes nothing on this closure and exists for the table's growth.
That is stated so a test asserting "the guard declines something" is known to
need a synthetic fixture rather than the corpus.

---

## 2. `CpeSynthesisCoverage`

The document-scope record (C209). One per scan.

| field | type | notes |
| --- | --- | --- |
| `schema` | `&'static str` | `"waybill-cpe-coverage/v1"` |
| `with_candidates` | `usize` | components carrying at least one CPE after the pass |
| `eligible_unmapped` | `usize` | components with a release version whose `(ecosystem, name)` produced no candidate |

**Invariant**: both counts are over the final component set, after every pass
that can change a PURL. Computed once, at the same point the record is built.

**Why `eligible_unmapped` and not just a total**: a document with
`with_candidates: 6` and `eligible_unmapped: 117` says *"the mechanism ran,
and the table is the limit"*. One with `with_candidates: 0` and
`eligible_unmapped: 0` says *"there was nothing to do"*. One with
`with_candidates: 0` and `eligible_unmapped: 123` says **"the pass did not
run"** — which is the state this feature exists to make impossible, and which
no count of successes alone can express.

**Emitted when**: always, on any scan. Not gated on `--nix-closure`: the
record describes a pass that runs for every component from every reader, and
gating it would reintroduce the blind spot for non-closure scans.

> This differs from C208 (milestone 1366), which is emitted only where a
> closure was recorded. C208 describes a closure-specific pass; this one does
> not.

---

## 3. `ResolvedComponent.cpes` — existing, newly populated

| | |
| --- | --- |
| type | `Vec<String>` (unchanged) |
| written by | `scan_fs/mod.rs:1346` (existing) and the new post-closure pass |
| written by | `supplement/merge.rs:240` — **operator-supplied, must win** |
| read by | CDX `component.cpe` + C19 property; SPDX 2.3 `externalRefs`; SPDX 3 `externalIdentifier` |

### State transitions

```text
                    ┌─ non-empty ──────────────────────────────► unchanged
                    │  (an earlier pass, or --supplement-cdx)
  component.cpes ───┤
                    └─ empty ──┬─ version not release-shaped ──► stays empty
                               ├─ (ecosystem, name) unmapped ──► stays empty
                               └─ otherwise ───────────────────► candidates
```

**The `non-empty ⇒ unchanged` arm is the whole of FR-004 and FR-005.** It is
what keeps an operator's `--supplement-cdx` CPEs — merged at `scan_cmd.rs:4334`,
before any post-closure pass could run — from being overwritten by synthesized
guesses.

---

## Relationship to existing entities

```text
RawDerivation ──► closure component ──┐
                                      ├─► merged component set
other readers ────────────────────────┘          │
                                                 ▼
                        supplement::merge (operator CPEs land here)
                                                 │
                                                 ▼
                              milestone-1366 identity pass
                                   (pkg:generic → pkg:pypi)
                                                 │
                                                 ▼
                           ▶ THIS PASS: fill cpes where empty
                                                 │
                              ┌──────────────────┴─────────────────┐
                              ▼                                    ▼
                  ResolvedComponent.cpes                 CpeSynthesisCoverage
                  (C19 + native carriers)                      (C209)
```

**Not touched**: `synthesize_cpes` itself, `GENERIC_LIBRARY_CPES`,
`format_cpe`, `cpe_escape`, the existing pass at `scan_fs/mod.rs:1346`, and
every component that already carries CPEs.
