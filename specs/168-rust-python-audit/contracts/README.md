# Contracts — milestone 168

**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md) | **Data model**: [data-model.md](../data-model.md)

## No new external contracts

Milestone 168 is a documentation + measurement milestone. It does NOT introduce:

- New CLI flags on the `mikebom` binary
- New SBOM annotations or property names
- New parity-catalog rows
- New Rust types crossing crate boundaries
- New file formats consumed or emitted by mikebom

The audit report at `docs/audits/2026-07-06-tauri-airflow.md` is a Markdown document consumed by humans, not a machine-readable interface. Its structural schema is documented in `data-model.md` entities E1 through E6.

## Contract surface INHERITED (not created) by milestone 168

The audit exercises mikebom's existing wire-format contracts:

- **CDX 1.6** (`cyclonedx-json` format): existing, unchanged.
- **SPDX 2.3** (`spdx-2.3-json` format): existing, unchanged.
- **SPDX 3.0.1** (`spdx-3-json` format): existing, unchanged. m166 dedup fix and m167 orphan-reason vocabulary extension are the most recent changes; both landed pre-m168 and are treated as baseline.
- **CLI surface**: `mikebom sbom scan --path <clone> --format <cdx|spdx-2.3|spdx-3> --output <path>` — standard invocation, unchanged.

## m167 vocabulary applicability assessment (FR-012 + SC-012)

The audit MEASURES whether m167's C45 `mikebom:orphan-reason` vocabulary (5 codes: `stale-go-sum-entry`, `dead-lockfile-entry`, `hoisted-unused`, `unresolved-indirect-require`, `flat-attached-fallback`) cleanly describes orphan patterns observed in the Rust + Python ecosystems. IF the vocabulary is insufficient, the audit PROPOSES candidate new codes (with rationale + example PURL) as follow-on milestone recommendations — but the audit itself does NOT add codes to the vocabulary. Extension would be a separate follow-on milestone.

## Non-goals

- No CLI flag additions.
- No wire-format changes.
- No golden regeneration.
- No new parity-catalog rows.
- No changes to mikebom's dependency-detection code paths.
- No changes to the audit-tools' invocation (mikebom, Trivy, Syft each called with their existing CLIs).
