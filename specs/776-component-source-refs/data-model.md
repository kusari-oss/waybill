# Phase 1 Data Model — m776 component source-provenance references

**Feature**: 776-component-source-refs
**Status**: Complete
**Date**: 2026-09-05

Per-scan in-process types. No persistence, no new wire representation — the emitted representation is CycloneDX's existing `externalReferences[]` and its SPDX counterparts.

## Existing types — reused unchanged

### `ExternalReference` (`waybill-common`, unchanged)

```rust
pub struct ExternalReference {
    pub ref_type: String,   // a CycloneDX-native externalReference.type value
    pub url: String,
}
```

**Milestone change**: NONE. This milestone populates `ResolvedComponent.external_references: Vec<ExternalReference>`, a field that already exists and that all three emitters already consume (research R5).

**Validation rules introduced by this milestone**:
- `ref_type` MUST be one of the natively-defined CycloneDX 1.6 types this milestone maps to: `vcs`, `issue-tracker`, `documentation`, `website`, `attestation`, `distribution`. All six verified present in the 1.6 `externalReference.type` enum.
- `url` MUST be a non-empty, well-formed absolute URL (FR-004). Entries failing this are omitted, not emitted with a placeholder.
- The pair `(ref_type, url)` is the identity for deduplication (FR-006, research R4). Same URL under two different kinds is two distinct references, deliberately.

---

### `Link` (`enrich/deps_dev_client.rs`, unchanged)

```rust
pub struct Link {
    pub label: String,   // service-defined vocabulary
    pub url: String,
}
```

**Milestone change**: NONE. Already deserialized into `VersionInfo.links` on every enrichment-enabled scan and then discarded (research R1). This milestone reads it.

---

### `VersionInfo` (`enrich/deps_dev_client.rs`, unchanged)

```rust
pub struct VersionInfo {
    pub licenses: Vec<String>,      // consumed today
    pub advisory_keys: Vec<String>, // not consumed
    pub links: Vec<Link>,           // NOT consumed today — this milestone consumes it
}
```

**Milestone change**: NONE to the type. Only the stale comment at `deps_dev_client.rs:4` (asserting `links` "aren't yet" consumed) is corrected.

---

## Mapping — US1

### deps.dev label → CycloneDX reference kind

| Label | Kind | Observed frequency (30-component npm sample) |
|---|---|---:|
| `SOURCE_REPO` | `vcs` | 30/30 |
| `ORIGIN` | *(deferred — unmapped)* | 30/30 |
| `HOMEPAGE` | `website` | 25/30 |
| `ISSUE_TRACKER` | `issue-tracker` | 21/30 |
| `ATTESTATION` | `attestation` | 20/30 |
| *(any other label)* | *(unmapped — skipped, counted)* | — |

**Validation rules**:
- The mapping is total over the five mapped labels and rejects everything else. An unmapped label produces no reference and increments the unmapped-skip counter (FR-003, FR-014b).
- `ORIGIN` is treated exactly as an unmapped label until its semantics are confirmed upstream (Clarifications Q1). It is *not* special-cased into silence — it counts as a skip, so the counter reflects reality.
- Mapping is label-driven only. The URL's shape MUST NOT influence the chosen kind: a `HOMEPAGE` pointing at a GitHub URL is still `website`, not `vcs`. Inferring kind from URL shape would be the guess FR-003 forbids.

---

## New entity — observability

### Mapping summary (per scan)

The aggregate reported once per scan per FR-014a.

```
references emitted, by kind:
    vcs | issue-tracker | documentation | website | attestation | distribution
links skipped:
    unmapped-label   (vocabulary drift signal)
    malformed-url    (upstream data-quality signal)
```

**Validation rules**:
- Emitted exactly once per scan regardless of component count (FR-014a). Never per-component, never per-link — FR-003 forbids per-occurrence output and a 369-component fixture would make it unusable.
- Per-kind emitted counts MUST equal the references actually present in the emitted document (SC-009a). The summary is a report of what happened, not an independent estimate.
- The two skip counters MUST remain distinct (FR-014b). They call for opposite responses: a rising unmapped count means map a new label; a rising malformed count means do not.

**Lifecycle**: accumulated during the scan, reported at scan end, dropped. Not persisted, not emitted into the SBOM.

---

## Unchanged surfaces

- **Every emitter.** `generate/cyclonedx/builder.rs`, `generate/spdx/packages.rs`, `generate/spdx/v3_packages.rs` already consume `external_references` (research R5). No emitter is touched.
- **Catalog rows A9 / A10 / A11** (homepage / vcs / distribution) and their parity extractors — already exist, already wired. This milestone populates them rather than adding rows (research R6).
- **`ResolvedComponent`** — no new field; `external_references` already exists.
- **The deps.dev client, its transport, and its per-scan response cache** — unchanged. No new request is made (FR-007).
- **Operator surface** — no flags, no environment variables (FR-014).

---

## Diagram — where references come from

```text
                    ┌──────────────────────────────────────────┐
                    │  component discovered by a reader         │
                    │  (ecosystem reader, binary reader, …)     │
                    └───────────────┬──────────────────────────┘
                                    │
              ┌─────────────────────┴─────────────────────┐
              │                                           │
              ▼                                           ▼
  ┌───────────────────────────┐            ┌──────────────────────────────┐
  │ US2 — offline derivation  │            │ US1 — enrichment mapping     │
  │ external_refs_from_purl   │            │ apply_version_info           │
  │                           │            │                              │
  │ pure fn of the PURL:      │            │ reads VersionInfo.links —    │
  │  • no network             │            │ ALREADY fetched alongside    │
  │  • distribution URLs      │            │ licenses, currently discarded│
  │    where the registry     │            │  • no new network request    │
  │    scheme is determined   │            │  • 5 labels mapped           │
  │    by name+version        │            │  • ORIGIN + unknown → skip   │
  │  • existing website refs  │            │    (counted)                 │
  │    PRESERVED (FR-011)     │            │  • malformed URL → skip      │
  │                           │            │    (counted separately)      │
  │ works under --offline     │            │ requires enrichment enabled  │
  └────────────┬──────────────┘            └───────────────┬──────────────┘
               │                                           │
               └──────────────────┬────────────────────────┘
                                  ▼
                 ┌────────────────────────────────────────┐
                 │ ResolvedComponent.external_references   │
                 │  • dedup on (kind, url)      FR-006     │
                 │  • stable sort on (kind, url) FR-013    │
                 │  • operator-supplied refs preserved     │
                 │    and un-reordered           FR-012    │
                 └───────────────┬────────────────────────┘
                                 │  (emitters already consume this — R5)
            ┌────────────────────┼────────────────────┐
            ▼                    ▼                    ▼
    CycloneDX 1.6          SPDX 2.3              SPDX 3
  externalReferences[]   externalRefs[]        software_sourceInfo (vcs)
   {type, url}           category: OTHER,      software_homePage   (website)
   all 6 kinds           ref_type verbatim     software_downloadLocation
                         all 6 kinds           ⚠ issue-tracker / documentation /
                                                 attestation have no scalar slot
                                                 (accepted asymmetry — R5)
```

**Invariants**:
- A component never gains a reference this milestone cannot source from either the PURL (US2) or an already-fetched enrichment link (US1). Nothing is inferred from URL shape or package naming (Principle IX).
- The two paths are independent: US1 is inert under `--offline`, US2 is unaffected by enrichment availability. A component may receive references from both, deduplicated on `(kind, url)`.
- Output difference relative to baseline is confined to *added* references (SC-010). No component, relationship, license, or annotation changes.

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| `VersionInfo.links` deserialized, then discarded | Mapped to references for five labels | FR-001, FR-002, R1 |
| `ORIGIN` / `ATTESTATION` / `HOMEPAGE` silently dropped despite appearing on most components | `HOMEPAGE` + `ATTESTATION` mapped; `ORIGIN` skipped *and counted* | Clarifications Q1, FR-002a |
| `external_refs_from_purl` covers 4 ecosystems, 3 of them emitting landing pages | Distribution references added where the registry scheme is PURL-determined; existing landing pages preserved | FR-009, FR-011, R7 |
| No visibility into label handling | One aggregate summary per scan; unmapped and malformed skips counted separately | FR-014a, FR-014b, Q2 |
| A9/A10/A11 parity extractors compare empty against empty | Same extractors exercise real data across all three formats | R6 |
| py-uv ~1/109, npm-nodejs 0/369 components carry a source reference | ≥80% on both, given measured upstream availability (npm 100%, pypi 93%) | SC-001, SC-002, R3 |

Every transition preserves the `ExternalReference` shape, the emitters, the catalog rows, the operator surface, and the existing references (FR-011/FR-012).
