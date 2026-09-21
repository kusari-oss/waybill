# Contract — Repo Observation Report

**Feature**: 924-repo-observation-report · **Date**: 2026-09-21
**Stability**: `alpha` — additive change expected (FR-017)

The report is the feature's only external interface. This file is the
contract; [data-model.md](../data-model.md) gives field semantics.

---

## C-1 — Serialisation

JSON, UTF-8, one document per run. A JSON Schema is published alongside and
every emitted report validates against it (FR-016, SC-006).

Named fields and explicit enumerated strings throughout — no positional
arrays, no packed encodings, no abbreviations (FR-018). A person and a
language model must both be able to read it without a decoder.

## C-2 — Versioning

`schema_version` is `{major, minor}` (FR-017a).

| Change | Bump |
|---|---|
| New field | minor |
| New enumeration member | minor |
| Field removed | **major** |
| Field renamed | **major** |
| Field's type changed | **major** |
| Field's meaning changed with type intact | **major** |

The last row is the one that gets missed: a field that keeps its name and type
but starts meaning something else is the most dangerous change a consumer can
meet, because nothing mechanical catches it.

## C-3 — Consumer obligations

- **Unrecognised major** ⇒ refuse to interpret (FR-017b). Do not partially
  parse; a major bump means an assumption you hold is void.
- **Unrecognised minor** ⇒ proceed, ignoring unknown fields.
- **Unknown enumeration member** ⇒ preserve and surface it as unknown
  (FR-017c). **Never** coerce to a known member and never drop it.

C-3's third clause exists because adding an enum member is *additive to the
schema* and therefore only a minor bump, yet it silently breaks a consumer
that matches exhaustively. This project has already shipped that bug class
once — an allowlist that silently missed variants added later, undetected for
roughly two years — so the obligation is stated in the contract rather than
left to consumer good sense.

## C-4 — Reconciliation invariant

```
files_walked == files_claimed + files_unclaimed + Σ files_skipped[reason]
```

Holds in every report, in both redaction modes, whether or not directories
were aggregated (FR-003, FR-021b). **A report violating C-4 is invalid** — a
consumer may reject it outright. This is what makes the census trustworthy:
it is checkable without access to the repository it describes.

## C-5 — Determinism

Two runs over an unchanged repository produce byte-identical documents once
the fields named in `volatile_fields` are masked (FR-020).

`volatile_fields` is **self-describing**: it lists its own volatile paths, so
a differ needs no out-of-band knowledge and stays correct as the schema grows.

## C-6 — Redaction

| Guarantee | `redaction_mode: none` (default) | `redaction_mode: paths` |
|---|---|---|
| Absolute filesystem paths | never present | never present |
| Content excerpted from scanned files | never present | never present |
| Repository-relative path segments | present verbatim | replaced by stable identifiers |

Both rows of unconditional guarantees hold in **every** mode (FR-019) — they
are properties of the report, not of a setting.

Under `paths`, identical segments map to identical identifiers within a
report, so nesting depth and repetition survive while names do not (FR-019b).

`redaction_mode` is always present (FR-019c) so a reader never has to infer
whether an absent name was absent or removed.

## C-7 — Independence of claim and ambiguity

`claim_status` is exclusive (FR-012a). `ambiguity` is independent and may
accompany any claim status (FR-012b).

A consumer MUST NOT infer absence of ambiguity from a `claimed` status. The
canonical counter-example is in this repository: `waybill-cli/tests/` holds 89
lockfiles across ecosystems — 27 `go.mod`, 24 `package.json`, 21 `Cargo.toml`
— which readers do claim, and which are not waybill's dependencies. Claimed
and ambiguous simultaneously, and the ambiguity is the more important half.

## C-8 — What the report never does

- Never transmits itself anywhere (FR-023).
- Never issues a network request to produce itself (FR-022).
- Never changes emitted SBOM content (FR-024).
- Never resolves genuine ambiguity by preference (FR-014). Where evidence does
  not determine the answer, the ambiguity **is** the answer.

## C-9 — Bounded size

`directories_recorded` grows with a repository's *significant* structure, not
its directory count (FR-021a). Measured: a 5,118-directory repository yields
393 records (7.7%) at the default threshold of 25 (research R3).

`significance_threshold` is reported (FR-021c). Two reports produced under
different thresholds are **not** comparable, and a consumer diffing them must
check the value first.
