# Implementation Plan: Component Source-Provenance References (m776)

**Branch**: `776-component-source-refs` | **Date**: 2026-09-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/776-component-source-refs/spec.md`

## Summary

Two independent population fixes for `ResolvedComponent.external_references`, which every emitter already consumes but almost nothing populates.

**US1 (P1)** maps the deps.dev `links[]` array — already fetched and parsed into `VersionInfo.links` on every enrichment-enabled scan, then discarded — onto external references inside `depsdev_source.rs::apply_version_info`, the same function that already applies the license half of that payload. Four labels map to natively-defined CDX types (`SOURCE_REPO`→`vcs`, `ISSUE_TRACKER`→`issue-tracker`, `DOCUMENTATION`→`documentation`, `HOMEPAGE`→`website`, `ATTESTATION`→`attestation`); `ORIGIN` is deferred per Clarifications Q1. Zero new network requests.

**US2 (P2)** extends `scan_fs/mod.rs::external_refs_from_purl` — today covering only `cargo`, `golang`, `nuget`, and a nested-jar-gated `maven` arm — with deterministic `distribution` URLs for ecosystems whose registry URL scheme is fully determined by the PURL. Offline-safe, no network.

Plus a per-scan aggregate summary (FR-014a/b) reporting references emitted by kind and links skipped, split by reason.

Measured baselines: py-uv ~1/109 components carry a source reference, npm-nodejs 0/369. Live sampling of the enrichment service shows `SOURCE_REPO` present for 30/30 npm and 27/29 pypi components, so SC-001/SC-002's ≥80% targets have headroom.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–775; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `serde`/`serde_json`, `tracing`, `anyhow`. The deps.dev client, its `VersionInfo`/`Link` types, and the HTTP transport all already exist and are unchanged. **Zero new dependencies** (FR-015 + SC-007).
**Storage**: N/A — references are derived per-component during a scan and emitted; no cache, no persistence. The existing per-scan deps.dev response cache in `depsdev_source.rs` is reused unchanged.
**Testing**: `cargo +stable test --workspace`. Existing `cdx_regression`, `spdx_regression`, `spdx3_regression`, `holistic_parity`, per-ecosystem `scan_*` suites, and the m669 corpus goldens. New unit tests for label mapping, URL validation, dedup, ordering, and the summary counters.
**Target Platform**: macOS aarch64 + Linux x86_64 (m669 reference class); Windows inherited unchanged.
**Performance Goals**: NFR-001 + SC-006 — wall time within 3% of baseline. The work is a map over data already in memory plus string formatting; no new I/O.
**Constraints**: No new network requests (FR-007); no new operator surface (FR-008/FR-014); no vendor-prefixed property for source provenance (FR-005 + SC-009); deterministic ordering (FR-013 + SC-005); existing references preserved (FR-011/FR-012); output diff confined to added references (SC-010).
**Scale/Scope**: Five-ecosystem measurement set (Go, Python, Rust, JavaScript, JVM). Largest fixture 369 components.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)** — ✅ Pass. Zero new dependencies; no C toolchain touched.
- **Principle II (eBPF-Only Observation)** — ✅ N/A. Scan-mode metadata enrichment; the trace-mode eBPF discovery surface is untouched. Note the distinction Principle II draws: external sources may **enrich** already-discovered dependencies but must never **add** components. This milestone adds only references to components already discovered by other means — it never introduces a component. That is squarely inside the enrichment allowance.
- **Principle III (Fail Closed)** — ✅ Pass. NFR-002 requires malformed enrichment metadata to leave the component emitted-without-references rather than aborting or dropping it. No silent fallback to a guessed reference: FR-003/FR-004 omit rather than fabricate.
- **Principle IV (Type-Driven Correctness)** — ✅ Pass. No new `.unwrap()` in production paths. URL validation returns an `Option`/`Result` rather than panicking. Test modules retain the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard.
- **Principle V (Specification Compliance)** — ✅ **Pass, and this milestone is squarely aligned with it.** Every emitted kind is natively defined by CycloneDX 1.6 (`vcs`, `issue-tracker`, `documentation`, `website`, `attestation`, `distribution` — all verified present in the 1.6 `externalReference.type` enum). **No `waybill:*` property is introduced** (FR-005), so the Principle V bullet-5 audit requirement is satisfied by construction: the native construct is not merely preferred, it is the only construct used. Catalog rows A9/A10/A11 already exist for homepage/vcs/distribution with native homes in all three formats; this milestone populates them rather than adding rows.
- **Principle VI (Three-Crate Architecture)** — ✅ Pass. `waybill-cli` only; `waybill-common`'s `ExternalReference` type is reused unchanged.
- **Principle VII (Test Isolation)** — ✅ Pass. Label-mapping, validation, dedup, ordering, and counter tests are pure functions over in-memory data — no network, no toolchain, no privilege. Fixture-level verification uses the existing corpus.
- **Principle VIII (Completeness) / IX (Accuracy) / X (Transparency)** — ✅ Pass. FR-003/FR-004 prefer omission over fabrication (Accuracy). FR-014a/b make the mapping's behavior observable rather than silent (Transparency). No completeness claim is altered.
- **Principle XII (External Data Source Enrichment)** — ✅ Pass. deps.dev is an already-integrated enrichment source; this consumes a field of an existing response rather than adding a source or a call.

**Gate result**: PASS. No violations; Complexity Tracking omitted.

## Project Structure

### Documentation (this feature)

```text
specs/776-component-source-refs/
├── plan.md                                   # This file
├── spec.md                                   # Feature specification (post-clarify)
├── research.md                               # Phase 0 output (this run)
├── data-model.md                             # Phase 1 output (this run)
├── quickstart.md                             # Phase 1 output (this run)
├── contracts/
│   ├── enrichment-link-mapping.md            # US1 label→kind contract
│   └── derived-distribution-refs.md          # US2 derivation contract
├── checklists/
│   └── requirements.md                       # From /speckit.specify
└── tasks.md                                  # Phase 2 output (/speckit.tasks — NOT this run)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   ├── enrich/
│   │   ├── deps_dev_client.rs                # READ-ONLY. `Link { label, url }` and
│   │   │                                     # `VersionInfo.links` already exist and are
│   │   │                                     # already populated by serde. The stale comment
│   │   │                                     # at :4 saying `links` "aren't yet" consumed
│   │   │                                     # gets corrected.
│   │   └── depsdev_source.rs                 # PRIMARY EDIT (US1): `apply_version_info`
│   │                                         # (~:83) gains link→reference mapping beside
│   │                                         # the existing license application.
│   └── scan_fs/
│       └── mod.rs                            # EDIT (US2): `external_refs_from_purl`
│                                             # (~:1827) gains distribution-URL arms.
│                                             # EDIT (obs): scan-level summary emission.
└── tests/                                    # Existing suites unchanged; new coverage as
                                              # in-file `#[cfg(test)]` modules.
```

**Structure Decision**: Two production files carry the milestone; a third carries the summary. The summary's emission point is after the last component-set mutation (~`scan_cmd.rs:3883`), not adjacent to the enrichment call — see research R9, since several passes between them can drop components and would make the reported counts disagree with the emitted document. **No emitter is touched** — `generate/cyclonedx/builder.rs` (~:1212), `generate/spdx/packages.rs` (~:520), and `generate/spdx/v3_packages.rs` (~:165) already read `external_references` and map it into each format. Populating the field is sufficient for all three outputs, which is what makes FR-016 free (research R5).

## Complexity Tracking

*No Constitution Check violations. Section intentionally empty.*
