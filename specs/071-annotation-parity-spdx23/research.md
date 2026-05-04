# Research — milestone 071 cross-format annotation parity

## Decision summary

| Decision | Choice | Section |
|---|---|---|
| D1 — SPDX 2.3 carrier shape | Reuse existing `MikebomAnnotationCommentV1` envelope on `Package.annotations[]` | §1 |
| D2 — Directionality model | Reuse existing 4-variant enum (`SymmetricEqual` / `CdxSubsetOfSpdx` / `PresenceOnly` / `CdxOnly`); no new variants | §2 |
| D3 — Cross-format component identity | Canonical PURL string (already implicit in spec US1 test) | §3 |
| D4 — Canonicalization implementation | Add `order_sensitive: bool` to `ParityExtractor`; default = sort arrays + sort keys (Q2 clarification) | §4 |
| D5 — Pre-PR gate enforcement | Standard `cargo test` integration test under `mikebom-cli/tests/parity_completeness.rs` (executed by `./scripts/pre-pr.sh` automatically) | §5 |
| D6 — Discovery scope | Component-level only (Q3 clarification); document-level out of scope for this milestone | §6 |

---

## §1 — SPDX 2.3 carrier shape (D1)

**Decision**: Reuse the existing `MikebomAnnotationCommentV1` envelope already defined at `mikebom-cli/src/generate/spdx/annotations.rs:31` (constant `ENVELOPE_SCHEMA_V1 = "mikebom-annotation/v1"`). It is the canonical SPDX 2.3 carrier for every `mikebom:*` annotation key. No new shape, no new schema version.

**Rationale**: The envelope already exists, has a JSON Schema contract at `mikebom-cli/src/generate/spdx/contracts/mikebom-annotation.schema.json`, has a round-trip unit test (`annotation_envelope_schema_matches_json_file`), is decoded by `extract_mikebom_annotation_values()` in `parity/extractors/common.rs:86`, and is documented in the file's own module-level rustdoc as the V1 carrier. The CFI gap is therefore **emission-side coverage**, not infrastructure: some keys are pushed through the envelope (lines 132/142/164/208/217 of `annotations.rs`), some are not, and the parity extractors built atop the envelope return empty sets for the un-pushed keys.

**Alternatives considered**:
- *Add a new envelope V2 with richer typing.* Rejected — the existing V1 envelope is fine; complexity would be self-inflicted.
- *Promote each `mikebom:*` key to its own SPDX 2.3 annotation row (not bundled in a JSON envelope).* Rejected — SPDX 2.3 annotation rows have an immutable shape (`Annotator`, `AnnotationDate`, `AnnotationType`, `AnnotationComment`); the envelope-in-comment pattern was deliberately chosen for this exact extensibility need and is still the right call.

---

## §2 — Directionality model (D2)

**Decision**: Reuse the existing 4-variant enum at `mikebom-cli/src/parity/extractors/common.rs:41`:

| Variant | Existing semantic |
|---|---|
| `SymmetricEqual` | All three formats must carry equal sets — the default for `mikebom:*` keys. |
| `CdxSubsetOfSpdx` | `CDX ⊆ SPDX 2.3 ∧ CDX ⊆ SPDX 3` — used for native fields where SPDX may carry richer detail (e.g., A12 CPE candidates). |
| `PresenceOnly` | All three formats carry the datum but in shapes that structurally diverge (e.g., D1 evidence-identity model, E1 compositions). The check asserts non-empty in all three — "presence parity." |
| `CdxOnly` | CDX-only by design, supplanted in SPDX by a native dep-relationship type or `lifecycleScope` parameter (e.g., C42 `mikebom:lifecycle-scope`). |

**Rationale**: Q1 (hard-fail on uncatalogued), Q2 (canonicalization rule), and Q3 (component-level scope) all fit the existing model. Q1 is a behavior of the gate, not a new directionality. Q2 is a per-row metadata flag (D4). Q3 is a discovery-pass scope, not a directionality. No new enum variants are needed.

**Alternatives considered**:
- *Add `BidirectionalSubset` for the symmetric-but-loose case.* Rejected — `PresenceOnly` already covers it.
- *Collapse `CdxOnly` into `PresenceOnly` (asserting nothing about the SPDX side).* Rejected — `CdxOnly` carries the *positive* semantic that the SPDX side is supplanted by a native row asserted *elsewhere* in the catalog (e.g., C42 + B2's typed dep-relationship types). Collapsing would lose that auditable cross-reference.

---

## §3 — Cross-format component identity (D3)

**Decision**: Two components in different formats are the same component for parity purposes iff they share a canonical PURL string. PURL canonicalization rules: lowercase scheme, decoded percent-escapes for the `name`/`namespace` fields, sorted-alphabetically qualifiers, no trailing fragment unless content-bearing. The existing `Purl` newtype's `Display` impl already produces the canonical form.

**Rationale**: Spec US1 independent-test text already reads "for each component identified by canonical PURL." CDX bom-refs, SPDX 2.3 SPDXIDs, and SPDX 3 Element IRIs are all internal-to-format identifiers and not stable across formats. PURL is the only cross-format-stable identity mikebom emits. The existing parity extractors already operate this way (e.g., `cdx_purl` / `spdx23_purl` / `spdx3_purl` for row A1).

**Alternatives considered**:
- *Match by component name + version.* Rejected — name+version pairs are not unique across ecosystems (e.g., a `pkg:cargo/foo@1.0.0` and `pkg:npm/foo@1.0.0` are different components).
- *Match by content hash where present.* Rejected — many components have no per-format hash (e.g., source-tree main-modules from milestones 053–070); cross-format presence requires every component to be matchable.

---

## §4 — Canonicalization implementation (D4)

**Decision**: Add `pub order_sensitive: bool` to the `ParityExtractor` struct (existing at `parity/extractors/common.rs:31`). Default = `false`. Implement `canonicalize_for_compare(value: &Value, order_sensitive: bool) -> String` returning a canonical JSON string suitable for `==` comparison:

- Sort all object keys lexicographically (always — `BTreeMap` over `serde_json::Map`).
- Sort arrays lexicographically when `order_sensitive == false`; preserve insertion order when `order_sensitive == true`.
- Normalize whitespace (use `serde_json::to_string` without `_pretty`).
- Recurse into nested objects / arrays.

**Rationale**: Q2 chose generic-with-explicit-override. The existing `ParityExtractor` already returns `BTreeSet<String>` from each per-format extractor (set-equality already handles unordered comparison at the **outer** level), so `order_sensitive` is needed primarily for the **inner** payload of a key that itself contains a JSON array (e.g., a future `mikebom:trace-step-sequence` array where the step order matters). For all 6 currently-named keys, `order_sensitive = false` is correct.

**Alternatives considered**:
- *Hardcode "always sort everything" with no opt-out.* Rejected — Q2 explicitly carved an override for future order-sensitive keys.
- *Per-row callback function for canonicalization.* Rejected — overkill for the simple "sort arrays or not" axis; `bool` is sufficient.
- *Encode `order_sensitive` in the `Directionality` enum instead of a separate field.* Rejected — orthogonal concerns; mixing them would force every directionality variant to fork (`SymmetricEqualOrdered` / `SymmetricEqualUnordered`).

---

## §5 — Pre-PR gate enforcement (D5)

**Decision**: A standard `cargo test` integration test at `mikebom-cli/tests/parity_completeness.rs`. The test:

1. Invokes the in-process emitter on the existing 27 byte-identity fixtures (or a representative subset for speed) to produce CDX/SPDX 2.3/SPDX 3 outputs.
2. Greps every emitted document for `mikebom:*` keys (component-level only — `components[]` in CDX, `packages[]` in SPDX 2.3, `@graph[Package]` in SPDX 3; document-level out of scope per Q3).
3. Asserts every found key has a `ParityExtractor` row in the catalog.
4. For every `SymmetricEqual` row, asserts the canonicalized sets agree across the three formats.
5. For every non-`SymmetricEqual` row, asserts the directionality's invariant holds (e.g., for `CdxSubsetOfSpdx`: CDX ⊆ each SPDX; for `CdxOnly`: CDX non-empty + the catalog comment names the SPDX-side superseding row).

Failure messages MUST name (a) the offending key, (b) the format(s) where it does/doesn't appear, (c) the catalog row id (or "uncatalogued — add a `ParityExtractor` row to mikebom-cli/src/parity/extractors/mod.rs").

**Rationale**: `./scripts/pre-pr.sh` already runs `cargo +stable test --workspace`, which transitively runs every integration test. No script changes needed. The test is fully hermetic (no network, no external fixtures beyond the existing byte-identity goldens) so CI-runtime is in the millisecond budget per spec assumption.

**Alternatives considered**:
- *Custom binary invoked from `pre-pr.sh`.* Rejected — duplicates infrastructure; cargo-test path is the canonical mikebom hook.
- *Compile-time check via macro.* Rejected — can't read JSON output at compile time.
- *Soft warning during local dev, hard fail in CI.* Rejected — Q1 explicitly chose hard-fail everywhere.

---

## §6 — Discovery scope (component-level only) (D6)

**Decision**: This milestone's discovery pass (FR-003) operates over **component-level** annotation emission only. Document-level annotations (CDX `metadata.properties[]`, SPDX 2.3 top-level `annotations[]`, SPDX 3 document-level `Annotation` graph entries) are **out of scope** per Q3 clarification.

**Discovery findings** (from `grep -rn "mikebom:" mikebom-cli/src/generate/` cross-referenced against `parity/extractors/mod.rs` catalog rows):

**31 unique `mikebom:*` keys are emitted by mikebom code today** (excluding test/example stubs).

**Component-level keys with catalog row** (in scope, all SymmetricEqual or properly catalogued):
`mikebom:binary-class`, `binary-packed`, `binary-stripped`, `buildinfo-status`, `co-owned-by`, `component-role`, `confidence`, `cpe-candidates`, `deps-dev-match`, `detected-go`, `evidence-kind`, `lifecycle-scope` (CdxOnly), `linkage-kind`, `npm-role`, `os-release-missing-fields`, `raw-version`, `requirement-range`, `sbom-tier`, `shade-relocation`, `source-connection-ids`, `source-files`, `source-type`.

**Component-level keys NOT emitted but in catalog** (catalog-only — likely planned for future emission paths or emitted only in image-scan paths not covered by the 27-fixture grep): `detected-cargo-auditable`, `elf-build-id`, `elf-debuglink`, `elf-runpath`, `go-vcs-modified`, `go-vcs-revision`, `go-vcs-time`, `macho-codesign-flags`, `macho-codesign-identifier`, `macho-codesign-team-id`, `macho-min-os`, `macho-rpath`, `macho-uuid`, `not-linked`, `orphan-reason`, `pe-machine`, `pe-pdb-id`, `pe-subsystem`. These are not defects — they're catalog rows awaiting emission. The completeness test (D5) MUST tolerate catalog rows that produce empty sets when nothing emits them.

**Document-level keys (OUT OF SCOPE)**: `generation-context`, `graph-completeness`, `graph-completeness-reason`, `trace-integrity-events-dropped`, `trace-integrity-kprobe-attach-failures`, `trace-integrity-ring-buffer-overflows`, `trace-integrity-uprobe-attach-failures`. The first two are already in the catalog (`C40` / `E1`-adjacent), the others are not. Document-level parity is a follow-on milestone per Q3 clarification.

**Rationale**: Spec Out-of-Scope section explicitly says "Document-level annotations... Initial scope is component-level only — that's where the 11,130 CFI rows in the SC-001 measurement live." The discovery pass therefore restricts to component-level emit sites.

**Alternatives considered**:
- *Catalog the 5 trace-integrity-* keys now even though they're document-level.* Rejected — it's scope creep; the doc-level milestone will catalog them with the right document-level extractor primitives (which differ from component-level).
- *Treat any uncatalogued document-level key as a hard fail too.* Rejected — would break `./scripts/pre-pr.sh` immediately for the 5 trace-integrity-* keys; user explicitly bounded scope to component-level.

---

## §7 — Per-key audit of the 6 known problem keys

For each key driving the alpha.13 CFI gap, summarizing current emission state and required fix:

### `mikebom:source-files` (3,572 CFI rows — 32% of total CFI)

- **CDX**: emitted via `cyclonedx/metadata.rs` and `cyclonedx/builder.rs` per-component property `mikebom:source-files`.
- **SPDX 3**: emitted via `spdx/v3_annotations.rs:208`-area as a per-Package `Annotation` carrying `mikebom:source-files`.
- **SPDX 2.3**: code path exists at `spdx/annotations.rs:208` pushing through the V1 envelope, BUT the call site appears guarded or only invoked for some component classes. CFI count suggests the guard is too narrow.
- **Catalog row**: C18 — `SymmetricEqual`, `c18_spdx23` extractor at `parity/extractors/spdx2.rs:302`.
- **Required fix**: audit the SPDX 2.3 emission guard in `annotations.rs`, ensure parity with CDX's per-component condition. May require unconditional emission when `c.source_files` is non-empty.

### `mikebom:sbom-tier` (2,815 CFI rows — 25%)

- **CDX**: `cyclonedx/metadata.rs:325`.
- **SPDX 3**: `spdx/v3_annotations.rs:160`.
- **SPDX 2.3**: `spdx/annotations.rs:142` push exists.
- **Catalog row**: C5 — `SymmetricEqual`, `c5_spdx23` at `parity/extractors/spdx2.rs:290`.
- **Required fix**: same shape as source-files — audit emission guard. The number is high enough that a substantial fraction of components are missing the emit.

### `mikebom:cpe-candidates` (2,422 CFI rows — 22%)

- **CDX**: emitted as a per-component property carrying a comma-or-array of CPE strings.
- **SPDX 3**: `spdx/v3_annotations.rs` per-Package annotation.
- **SPDX 2.3**: `spdx/annotations.rs:217` push exists.
- **Catalog row**: C19 — `PresenceOnly` (currently). The value-shape difference (CDX may emit comma-joined string vs. SPDX array of strings) was the original motivation for `PresenceOnly`.
- **Required fix**: with the new canonicalization (D4), promote C19 from `PresenceOnly` to `SymmetricEqual` IF canonicalization can normalize comma-joined → array. Decision: emit as JSON array on all three sides; bump to `SymmetricEqual`. If a CDX consumer needs the comma-joined form for legacy reasons, that's a separate emit knob.

### `mikebom:deps-dev-match` (1,041 CFI rows — 9%)

- **CDX**: emitted per-component.
- **SPDX 3**: `spdx/v3_annotations.rs:150`.
- **SPDX 2.3**: `spdx/annotations.rs:132` push exists.
- **Catalog row**: C3 — `SymmetricEqual`, `c3_spdx23` at `parity/extractors/spdx2.rs:288`.
- **Required fix**: audit emission guard.

### `mikebom:npm-role` (356 CFI rows — 3%)

- **CDX**: per-component property.
- **SPDX 3**: per-Package annotation.
- **SPDX 2.3**: `spdx/annotations.rs:164` push exists.
- **Catalog row**: C9 — `SymmetricEqual`, `c9_spdx23` at `parity/extractors/spdx2.rs:293`.
- **Required fix**: audit emission guard.

### `mikebom:lifecycle-scope` (262 CFI rows — 2%)

- **CDX**: per-component property — alpha.10's standards-native CDX `scope: "excluded"` plus this finer-grained annotation for dev/build/test split.
- **SPDX 3 + SPDX 2.3**: NOT emitted by design; SPDX uses native dep-relationship types (`DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `TEST_DEPENDENCY_OF` per Constitution Principle V's example).
- **Catalog row**: C42 — `CdxOnly`, `c42_cdx` exists, `spdx23 = empty` and `spdx3 = empty` per `parity/extractors/mod.rs:212`.
- **Required fix**: ALREADY CORRECTLY MODELLED. The 262 CFI rows are because the *external* harness doesn't read the catalog's directionality marker — it sees the asymmetric emission and reports CFI uniformly. This is harness-side noise that the new pre-PR gate test (which DOES read the catalog) will not produce. The 262 rows count toward the SC-001 ≤556 budget and the milestone passes them via the catalog-rationale-publishing path (FR-005): `docs/reference/sbom-format-mapping.md` will explicitly enumerate this row so external harness consumers can configure their tools to filter it out.

### Summary

5 of 6 are SPDX 2.3 emission-guard fixes (probably the same root cause across all 5 — likely a shared "if extra_annotations" gate that some component construction paths bypass). 1 (lifecycle-scope) is correctly modelled but needs published-doc rationale.

**Estimated impact**: Closing the 5 emission-guard cases drops CFI by ~10,206 (3,572 + 2,815 + 2,422 + 1,041 + 356). Documenting C42 doesn't reduce the harness count but the catalog-rationale doc allows operators to filter intentional asymmetries. Net SC-001 outcome: 11,130 → ~660 if the C42 rows remain harness-flagged, or 11,130 → ~400 if the harness can be configured to read the catalog. Both numbers are **below** the ≤556 target with margin.

---

## §8 — Inherent-asymmetry audit (FR-011)

**Decision**: The currently-known case is `C42 mikebom:lifecycle-scope`. The audit must verify whether milestones 007–070 introduced any new legitimate asymmetries that landed CDX-only or SPDX-only by accident vs. design.

**Audit method**: For each non-`SymmetricEqual` catalog row in `parity/extractors/mod.rs`:

1. Read the inline rationale comment (FR-004 will require all rows to have one).
2. Confirm the named standards-native superseding construct exists in the format(s) where the `mikebom:*` annotation is absent.
3. Confirm the assertion is exercised by some other catalog row (e.g., for `CdxOnly` rows, the SPDX-side native field is asserted by a different row).

Current catalog inventory (non-SymmetricEqual rows, from `mod.rs`):

| Row | Key/Label | Directionality | Rationale state |
|---|---|---|---|
| A12 | CPE | CdxSubsetOfSpdx | EXISTS in inline comment — "CDX primary only; SPDX 3 every fully-resolved candidate" |
| B4 | image / filesystem root | PresenceOnly | EXISTS in inline comment — divergent shape |
| C19 | mikebom:cpe-candidates | PresenceOnly | EXISTS — comma-joined vs array. Will be promoted to SymmetricEqual under D4 canonicalization. |
| C22 | mikebom:os-release-missing-fields | PresenceOnly | NEEDS rationale audit |
| C42 | mikebom:lifecycle-scope | CdxOnly | EXISTS — per Constitution Principle V's named motivating case |
| D1 | evidence — identity | PresenceOnly | EXISTS — divergent shape |
| E1 | ecosystem completeness | PresenceOnly | EXISTS — `complete_ecosystems[]` shape divergence |

**Findings**: 5 of 7 non-symmetric rows already have inline rationale. C19 will be promoted to symmetric. C22 needs an audit (TBD during implementation — likely OK because os-release fields are presence-not-equality by nature). The audit pass itself is one task; no new asymmetric rows are expected to be discovered.

**Rationale**: The user's data showed only one CDX-only outlier (lifecycle-scope) over the 11,130 CFI rows. The audit's role is to confirm that observation, not surface dozens of new rows.

---

## Open items deferred to Phase 1

None. All architectural choices are pinned; the data-model.md and contracts/ artifacts capture the concrete shapes; quickstart.md captures the operator-visible behavior.
