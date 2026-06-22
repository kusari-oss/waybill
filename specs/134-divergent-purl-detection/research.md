# Research — milestone 134 divergent-PURL detection

Resolves the four open design / unknown items from `plan.md`'s Technical Context: the Constitution Principle V audit, the wire-format placement per emission path, the JSON-shape decision for the structured value, and the existing milestone-064 + milestone-038 integration points.

## R1: Constitution Principle V audit — does any native field express "same identity, divergent content"?

**Decision**: No native field exists. `mikebom:*` annotation is justified per the Principle V escape clause ("permitted only to carry finer-grained information the standard does not express").

**Rationale**:

- **CycloneDX 1.6** evaluated fields:
  - `components[].evidence.identity[]` — describes HOW a component was identified (technique, confidence, methods). Has nothing to do with "this PURL appeared at multiple paths with divergent content."
  - `components[].evidence.occurrences[]` — lists physical locations where a component was discovered. CLOSEST native construct, BUT it doesn't carry divergence semantics — two occurrences sharing a PURL is by-design (e.g., npm `node_modules/.pnpm/X` + `node_modules/X` symlink). Cannot signal "divergent dep sets" without a custom property anyway.
  - `compositions[]` — declares completeness of subsets. Not collision-related.
  - `components[].variants` — no such field in 1.6.
  - `vulnerabilities[]` — vulnerability-specific.
- **SPDX 2.3** evaluated fields:
  - `relationships[].relationshipType: VARIANT_OF` — declares package A is a variant of package B (different PURLs, related semantics). Does NOT fit — divergent-PURL is about ONE PURL across multiple paths, not two different PURLs.
  - `relationships[].relationshipType: COPY_OF` — declares package A is a copy of B. Same shape mismatch as VARIANT_OF.
  - `packages[].sourceInfo` — narrative provenance string. Could carry divergence info but is unstructured prose; consumers can't programmatically query it.
- **SPDX 3.0.1** evaluated fields:
  - `software_Package.contentBy` — does NOT exist in 3.0.1 (verified against the official ontology release notes).
  - Relationships including the same `VARIANT_OF` / `COPY_OF` from 2.3 — same shape mismatch.
  - `software_Package.software_sourceInfo` — same unstructured-prose limitation as SPDX 2.3.

**Alternatives considered**:

- **Reuse CDX `evidence.occurrences[]` and add only the divergence semantic via a sub-property.** Rejected: the occurrences array is descriptive ("found here too"), not normative ("these differ"); overloading it would mislead consumers who assume occurrences are interchangeable.
- **Emit a SPDX `VARIANT_OF` relationship between the two PURLs.** Rejected: that semantically means "different but related crates", which is the wrong message — the whole point is that the PURLs are IDENTICAL but the content isn't.
- **Encode divergence as a free-text narrative in `sourceInfo` / `software_sourceInfo`.** Rejected: unstructured prose fails SC-005 ("structured machine-readable signal — no log-parsing required").

**Audit narrative for `docs/reference/sbom-format-mapping.md`**: two new C-rows (`mikebom:duplicate-purl-divergent` per-component + `mikebom:purl-collisions-detected` document-scope), classified as **KEEP-NO-NATIVE**. Justification clause cites this research entry verbatim.

## R2: Wire-format placement per emission path

**Decision**: Follow the existing milestone-061 `mikebom:graph-completeness` placement pattern in each format.

- **CDX 1.6**:
  - Per-component → `components[].properties[]` with `name: "mikebom:duplicate-purl-divergent"`, `value: <JSON-string-of-structured-value>`.
  - Document-scope → `metadata.properties[]` (top-level `properties[]` would also work; `metadata.properties[]` is the milestone-061 precedent).
- **SPDX 2.3**:
  - Per-component → `packages[].annotations[]` with `annotator: "Tool: mikebom-..."`, `annotationType: "OTHER"`, `comment: <JSON-encoded-envelope>` (the existing `MikebomAnnotationCommentV1` envelope from the annotation-parity infrastructure).
  - Document-scope → top-level `annotations[]` with the same envelope shape.
- **SPDX 3.0.1**:
  - Per-component → `software_Package` element's `Element.extension[]` with the mikebom namespace.
  - Document-scope → `SpdxDocument.extension[]` with the mikebom namespace.

**Rationale**: Milestone-061 graph-completeness has already been audited and shipped through this exact placement pattern. Consumers' jq queries against `properties[].name == "mikebom:..."` and `annotations[].comment | fromjson | .property` already work. Reusing the path avoids a second new wire-format precedent.

**Alternatives considered**:

- **Use `components[].evidence.occurrences[].additionalContext`** (CDX). Rejected per R1 — overloads the "found here too" semantic.
- **New top-level `mikebom:*` namespaced object on the document root** (CDX + SPDX). Rejected: would NOT validate against schemas without schema extensions; reviewers reject schema-extending changes.

## R3: JSON shape of the structured value

**Decision**: A canonical envelope shared across both per-component and document-scope surfaces:

```json
{
  "v": 1,
  "purl": "pkg:cargo/foo@1.2.3",
  "reason": "deps-differ",
  "paths": [
    "crates/foo/Cargo.toml",
    "vendor/foo/Cargo.toml"
  ],
  "dep_sets_by_path": {
    "crates/foo/Cargo.toml": ["serde", "tokio"],
    "vendor/foo/Cargo.toml": ["serde", "tokio", "anyhow"]
  }
}
```

When the reason includes `hashes-differ`, additional key:

```json
  "hashes_by_path": {
    "crates/foo/Cargo.toml": "<64-hex-sha256>",
    "vendor/foo/Cargo.toml": "<64-hex-sha256>"
  }
```

When the reason is `both`, BOTH `dep_sets_by_path` and `hashes_by_path` are present.

For the **document-scope** annotation, the wrapper is an array of these records keyed under `collisions`:

```json
{
  "v": 1,
  "collisions": [
    { "purl": "...", "reason": "...", "paths": [...], "dep_sets_by_path": {...} },
    { "purl": "...", "reason": "...", "paths": [...], "hashes_by_path": {...} }
  ]
}
```

**Rationale**:

- `v: 1` is the schema-version marker — future-proofs the wire format for evolution.
- `purl` is duplicated into the per-component payload (redundant with the component it lives on) only to keep the structure identical to the document-scope entry shape — eases consumer code that walks either surface uniformly.
- Per-path attribution (`dep_sets_by_path`, `hashes_by_path`) is the locked choice from `/speckit.clarify` Q2.
- `paths` is the sorted (in walk order) list of every manifest path — duplicates `dep_sets_by_path`/`hashes_by_path` keys but kept as a separate field for quick "how many participated?" queries without object-key iteration.
- The reason enum: `"deps-differ"`, `"hashes-differ"`, `"both"` — three values total.

**Path representation**: rootfs-relative per milestone-100's `normalize_sbom_path_relative` (see `mikebom-cli/src/scan_fs/sbom_path.rs:63`). Already-normalized path strings are forward-slash separated on every platform.

**Alternatives considered**:

- **Skip `v: 1`** — rejected: consumers should be able to refuse unknown-version payloads cleanly; schema-versioning is the same discipline that just paid off in the milestone-110 fingerprint corpus.
- **Symmetric-difference precomputed for `divergent_dep_names`** — rejected per `/speckit.clarify` Q2: lossless per-path attribution is the locked choice.

## R4: Existing milestone-064 + milestone-038 integration points

**Milestone-064 dedup site** (cargo main-module emission):

- File: `mikebom-cli/src/scan_fs/package_db/cargo.rs`
- Function: `read_cargo_manifests` (the per-file Cargo.toml processor) accumulates `(purl, path, dep_set)` tuples into a Vec, then the orchestrator-level dedup hash-set picks the first-discovered for each PURL.
- The dedup site is where the `tracing::warn!(...)` from milestone 064 fires when a duplicate PURL is observed. The same site is the natural place to compute the divergence record.
- Implication: the cargo reader needs to accumulate per-path `dep_set` (already does) AND optionally per-path deep-hash (NEW — currently only the surviving component's deep-hash is computed).

**Milestone-038 deep-hash** (per-component file-tree SHA-256):

- File: `mikebom-cli/src/scan_fs/scope/deep_hash.rs`
- Function: `compute_deep_hash(component_root: &Path) -> Result<String>` — walks the component's source tree, sorts by relative path, concatenates per-file SHA-256s, emits one combined SHA-256 hex string.
- Cost: O(file-count × file-size) for the component's source tree. Only fires when `--deep-hash` is set.
- Implication: for US2 (hashes-differ detection), the cargo reader must call `compute_deep_hash` PER colliding path, not just for the surviving one. Cost scales with collision count × per-component deep-hash cost — but only when `--deep-hash` is set, so default-mode users see no overhead.

**Tracing preservation** (FR-008):

- The milestone-064 `tracing::warn!(...)` continues to fire from its existing call site BEFORE the divergence record is computed and emitted. The new emission is additive.

## R5: Ecosystem-agnostic data model layer (FR-010 forward-compatibility)

**Decision**: The `DivergenceRecord` struct lives in `mikebom-common` and is keyed only on `Purl` + paths + dep-name lists + optional hash hex strings. No cargo-specific fields.

**Rationale**: Future milestones extending to npm / maven / pip / gem / go-binary just need to:
1. Populate the same `DivergenceRecord` from their reader's dedup site.
2. The format emitters then handle the per-component property + document-scope summary identically across ecosystems.

This makes future expansion a per-ecosystem one-PR change at the reader site, with zero changes to the emitter or parity-catalog plumbing.

**Alternatives considered**:

- **Per-ecosystem subclasses with format-specific fields** (e.g., cargo-specific feature flags). Rejected: bloats the wire format with fields most consumers ignore; can be added later in a `v: 2` schema bump if real ecosystem-specific needs emerge.

---

## Summary of Phase 0 unknowns

| Unknown | Decision | Reference |
|---|---|---|
| Native-field audit for Principle V | No native field exists; `mikebom:*` justified | R1 |
| Wire-format placement (CDX / SPDX 2.3 / SPDX 3) | Reuse milestone-061 graph-completeness pattern | R2 |
| JSON shape of structured value | `{ v, purl, reason, paths, dep_sets_by_path[, hashes_by_path] }` | R3 |
| Milestone-064 + 038 integration points | Cargo reader at dedup site; deep-hash per-path under `--deep-hash` only | R4 |
| Ecosystem-agnostic forward-compatibility | `DivergenceRecord` in mikebom-common, cargo-agnostic shape | R5 |

All Phase 0 unknowns resolved. Ready for Phase 1 (data-model + contracts + quickstart).
