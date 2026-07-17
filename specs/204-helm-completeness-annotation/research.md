# Research: `mikebom:image-extraction-completeness` Document-Scope Annotation

**Date**: 2026-07-17
**Purpose**: Resolve 3 mechanical unknowns before task decomposition.

## R1 — Reuse the m161 `go_workspace_mode` C112 plumbing pattern verbatim

**Investigation**: Every existing document-scope `mikebom:*` annotation driven by a `ScanDiagnostics` field follows the same 8-hop chain:

1. **Reader sets** `diagnostics.<field> = Some(...)` at scan time.
2. **`package_db::read_all` returns** a `DbScanResult { diagnostics, ... }`.
3. **`scan_fs::scan_path` mirrors** `scan_result.diagnostics.<field>` into a local `<field>` variable (line 344-354 for existing fields).
4. **`ScanResult { <field>, ... }` returned** with the new field (definition at line 76+).
5. **`scan_cmd.rs` destructures** `let scan_fs::ScanResult { ..., <field>, ... } = scan_fs::scan_path(...)` (line 2515).
6. **Threads into `ScanArtifacts`** as `<field>: <field>.as_ref()` (line 3187) — struct definition at `generate/mod.rs:51+`.
7. **`build_metadata` receives** `<field>: Option<&<Type>>` as an explicit argument (line 45+); `annotate_document` and `v3_annotations::annotate_document` receive it via `&ScanArtifacts`.
8. **Emitters branch** on `if let Some(<v>) = <field> { push!(..., "mikebom:<name>", <v>.as_wire_str()); }`.

Verified 5 precedents: `go_transitive_coverage` (m160), `go_transitive_fallback_count` (m172), `go_cache_warming` (m173), `go_workspace_mode` (m161), `divergence_records` (m134).

**Decision**: Follow this pattern verbatim. New chain for `helm_extraction_mode`:

- **ScanResult field**: `pub helm_extraction_mode: Option<scan_fs::package_db::HelmExtractionMode>` (m188 already has the enum in `scan_fs::package_db`).
- **ScanArtifacts field**: `pub helm_extraction_mode: Option<&'a scan_fs::package_db::HelmExtractionMode>` (borrow reference).
- **CDX emit branch** at `cyclonedx/metadata.rs` (after C112, before C118 in the go-cache-warming block):
  ```rust
  if let Some(mode) = helm_extraction_mode {
      properties.push(json!({
          "name": "mikebom:image-extraction-completeness",
          "value": mode.as_wire_str(),
      }));
  }
  ```
- **SPDX 2.3 emit branch** at `spdx/annotations.rs::annotate_document` (after C112):
  ```rust
  if let Some(mode) = artifacts.helm_extraction_mode {
      push(&mut out, "mikebom:image-extraction-completeness", json!(mode.as_wire_str()));
  }
  ```
- **SPDX 3 emit branch** at `spdx/v3_annotations.rs` (after C112):
  ```rust
  if let Some(mode) = artifacts.helm_extraction_mode {
      push(out, "mikebom:image-extraction-completeness", json!(mode.as_wire_str()));
  }
  ```
- **Parity catalog** at `parity/extractors/mod.rs` (immediately after C112 line 472):
  ```rust
  ParityExtractor {
      row_id: "C123",
      label: "mikebom:image-extraction-completeness",
      cdx: c123_cdx,
      spdx23: c123_spdx23,
      spdx3: c123_spdx3,
      directional: Directionality::SymmetricEqual,
      order_sensitive: false,
  },
  ```
- **Per-format extractor rows** in `cdx.rs`, `spdx2.rs`, `spdx3.rs` (single-line each, adjacent to the C112 line):
  ```rust
  cdx_anno!(c123_cdx, "mikebom:image-extraction-completeness", document);
  spdx23_anno!(c123_spdx23, "mikebom:image-extraction-completeness", document);
  spdx3_anno!(c123_spdx3, "mikebom:image-extraction-completeness", document);
  ```

**Rationale**: Five sibling annotations already ship this way. Reviewer-familiar. Zero new architecture. No new abstractions.

**Alternatives considered + rejected**:
- Pack the mode into an existing multi-value annotation (e.g. reuse `mikebom:go-transitive-coverage-reason` semantically): rejected — coupling unrelated signals under a shared key breaks parity extractor tests and confuses consumers.
- Emit only on `Rendered` and leave `Unrendered` implicit: rejected — spec FR-001 requires `"partial"` explicitly emitted so consumers can distinguish "no helm scan happened" from "helm scan happened, reduced fidelity".

**References**:
- `mikebom-cli/src/scan_fs/mod.rs:76-149, 311-352` — `ScanResult` + `scan_path` mirror sites.
- `mikebom-cli/src/generate/mod.rs:51-249` — `ScanArtifacts` struct.
- `mikebom-cli/src/generate/cyclonedx/metadata.rs:561-569` — C112 emit branch.
- `mikebom-cli/src/generate/spdx/annotations.rs:625-639` — C112 SPDX 2.3 emit branch.
- `mikebom-cli/src/generate/spdx/v3_annotations.rs:595-601` — C112 SPDX 3 emit branch.
- `mikebom-cli/src/parity/extractors/mod.rs:472` — C112 catalog row.

## R2 — Wire-string mapping for `HelmExtractionMode`

**Investigation**: The enum has two variants (`Unrendered`, `Rendered`) — spec FR-001 pins the wire values `"partial"` and `"full"` respectively. m161's `WorkspaceMode::as_wire_str()` at `package_db/golang/gowork.rs` is the direct precedent — a small `impl` method returning `&'static str`.

**Decision**: Add a `pub fn as_wire_str(&self) -> &'static str` inherent method to `HelmExtractionMode` at `mikebom-cli/src/scan_fs/package_db/mod.rs:427+`:

```rust
impl HelmExtractionMode {
    /// Wire-format string for the `mikebom:image-extraction-completeness`
    /// document-scope annotation across CDX 1.6 / SPDX 2.3 / SPDX 3.
    /// Milestone 204 (#554). Determinism-critical — MUST match the
    /// values in the m071 parity catalog row C123 exactly.
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            HelmExtractionMode::Unrendered => "partial",
            HelmExtractionMode::Rendered => "full",
        }
    }
}
```

Also drop the `#[allow(dead_code)]` on `HelmExtractionMode::Rendered` (currently at line 435) — m204 makes it live in every emitter path.

**Rationale**: Same pattern m161 uses. Kebab-case-ish wire values match m188/m203 issue-body specification exactly. `&'static str` return keeps callers zero-alloc.

**Alternatives considered + rejected**:
- `#[serde(rename_all = "lowercase")]` on the enum and emit `Debug` output: rejected — the wire values `"partial"` / `"full"` don't map from variant names via naming convention (variants are `Unrendered` / `Rendered`); explicit mapping is clearer.
- `impl Display for HelmExtractionMode`: rejected — Display is expected to be lossless-ish and human-readable; wire-str is a different concern. m161 keeps them separate for the same reason.

**References**:
- `mikebom-cli/src/scan_fs/package_db/golang/gowork.rs` — `WorkspaceMode::as_wire_str()` precedent.

## R3 — Test strategy: US1 in-CI, US2 gated

**Investigation**: The three P1 stories map to test lanes as follows:
- **US1** (`"partial"` emitted for default unrendered helm scan): needs only a `Chart.yaml`-bearing directory. m188/m203 tests already build these inline via `tempfile::tempdir()`. Runs in default CI — no external tools.
- **US2** (`"full"` emitted for `--helm-render` success): needs a real `helm` binary + the m203 subprocess path. Gated behind `MIKEBOM_HELM_INTEGRATION=1` per m188/m203 precedent.
- **US3** (annotation absent on non-Helm scans): scan a random directory without any `Chart.yaml`. Assert `mikebom:image-extraction-completeness` string does not appear in emitted JSON. Runs in default CI.

Existing `helm_reader.rs` test file has the plumbing (mikebom bin invocation via `env!("CARGO_BIN_EXE_mikebom")`, `scan_dir` helper, `components_by_purl_prefix`, `get_property`). Add three new `#[test]` functions there.

**Decision**: Add these three integration tests to `mikebom-cli/tests/helm_reader.rs`:

1. `m204_us1_partial_annotation_present_on_unrendered_helm_scan` — build a chart with `Chart.yaml` inline via tempdir, scan without `--helm-render` in all three formats (CDX, SPDX 2.3, SPDX 3), assert each output contains a document-scope annotation whose name is `mikebom:image-extraction-completeness` with value `"partial"`. Extract via format-specific accessor (properties[] for CDX, decoded envelope for SPDX 2.3, `statement` for SPDX 3).
2. `m204_us2_full_annotation_present_on_rendered_helm_scan` — MIKEBOM_HELM_INTEGRATION-gated; same chart + `--helm-render` + real helm; assert `"full"` in all three formats.
3. `m204_us3_annotation_absent_on_non_helm_scan` — scan a directory containing only `readme.txt`; assert the string `mikebom:image-extraction-completeness` does not appear in any of the three emitted JSON outputs.

The m071 parity test suite (`tests/holistic_parity.rs` + `tests/parity_synthetic_drift.rs`) will exercise C123 automatically once the ParityExtractor is registered — no additional per-format assertion tests needed beyond US1's explicit format-specific checks.

**Rationale**: US1 default-CI coverage is sufficient for the primary code path; US3 default-CI is the byte-identity regression guard. US2 gated is the m203-established convention for helm-binary-dependent tests.

**Alternatives considered + rejected**:
- Assert on golden JSON files instead of live scan output: rejected — the m204 test data is trivially generated inline; golden files would just add maintenance burden for a signal already covered by parity tests.
- Skip US3 (rely on golden regen audit): rejected — a small explicit test guards against future emitter refactors accidentally regressing the byte-identity contract.

**References**:
- `mikebom-cli/tests/helm_reader.rs:56` — `scan_dir` helper pattern.
- `mikebom-cli/tests/helm_reader.rs:434` — `default_scan_without_chart_yaml_is_byte_identical` — the m188 byte-identity precedent for US3-style tests.
- Memory `reference_spdx3_validator` — precedent for env-var-gated external-binary tests.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Plumbing pattern | Verbatim m161 `go_workspace_mode` C112 pattern (8-hop chain, `Option<&…>` in ScanArtifacts, per-format emit branches) | Novel abstraction / packing into existing annotation | 5 sibling precedents; zero new architecture |
| Wire-string mapping | Inherent `HelmExtractionMode::as_wire_str() -> &'static str` returning `"partial"` / `"full"` | Custom serde rename / Display impl | m161 precedent; explicit wire values match spec verbatim |
| Test strategy | US1 default-CI (`"partial"`), US2 gated MIKEBOM_HELM_INTEGRATION=1 (`"full"`), US3 default-CI (absence) | Golden-based / parity-only | 3-way P1 story alignment with 3 concise tests; parity tests catch three-format equivalence for free |
| Catalog row ID | C123 (next free) | (n/a) | C122 is highest used; verified via grep |
| New Cargo deps | Zero | (n/a) | Nothing needed |
