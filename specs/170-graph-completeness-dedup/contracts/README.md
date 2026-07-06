# Contracts: m170 Graph-Completeness Dedup

**Feature**: 170-graph-completeness-dedup
**Date**: 2026-07-06

This feature introduces NO new external CLI-flag or API contracts. mikebom's public surface (commands, flags, environment variables, emitted JSON schema keys) is unchanged post-m170.

## Two invariant contracts (informal, mikebom-internal)

### C1 — Emitted `mikebom:graph-completeness` uniqueness (per-SBOM invariant)

**Applies to**: every SBOM emitted by mikebom in CDX 1.6, SPDX 2.3, and SPDX 3.0.1 formats post-m170.

**Rule**: exactly ONE `mikebom:graph-completeness` document-scope annotation MUST appear in the emitted document.

- CDX 1.6: exactly one `.properties[]` entry with `name == "mikebom:graph-completeness"`.
- SPDX 2.3: exactly one document-level `.annotations[]` envelope with `field == "mikebom:graph-completeness"`.
- SPDX 3.0.1: exactly one graph-element `Annotation` targeting the SpdxDocument root IRI with `statement.field == "mikebom:graph-completeness"`.

**Enforcement**: SC-001/SC-002/SC-003 via golden-fixture diff. No runtime assertion — the invariant flows from emission code correctness, and the golden regen catches any regression.

**Consumer contract**: consumers may safely assume `jq '.properties[] | select(.name == "mikebom:graph-completeness") | .value'` (or the SPDX equivalent) returns exactly one value. `docs/reference/reading-a-mikebom-sbom.md` §3.3 already documents this as a singular signal; m170 makes the emission code match the docs.

### C2 — Parity extractor label uniqueness (compile-time invariant)

**Applies to**: `mikebom-cli/src/parity/extractors/mod.rs::EXTRACTORS` const slice.

**Rule**: every `ParityExtractor.label` value in the slice MUST be unique. No two rows may share a label.

**Enforcement**: new unit test `extractors_have_unique_labels` at `mikebom-cli/src/parity/extractors/mod.rs::tests`. Compile-time-static assertion — the test runs on every `cargo test` invocation and fails with a clear error naming the duplicate label + both colliding row IDs.

**Consumer contract**: internal only. Consumers of catalog rows (documentation generators, parity gates) may safely rely on label → row-id being a 1-to-1 map.

## No external CLI contract changes

- No new flags on `mikebom sbom scan`.
- No new environment variables.
- No new output filenames.
- No changes to existing flag semantics.

## No external emission format changes

- No new CDX property keys.
- No new SPDX annotation fields.
- No new SPDX 3 typed Annotation elements.
- No renamed keys — post-m170, `mikebom:graph-completeness` is the same key with the same value space (`complete|partial|unknown`) at the same location. Only the DUPLICATE goes away.

## No new file paths

- No new fixture directories.
- No new documentation files (edits to `docs/reference/sbom-format-mapping.md` C4 row's description are minor annotations, not new files).
- Sibling `mikebom-test-fixtures` repo: NO new fixture paths. Golden regeneration in-place at existing paths (`tests/fixtures/spdx/golang/`, `tests/fixtures/spdx3/golang/`).
