# Implementation Plan: Quality metadata backfill for milestone-130 new components

**Branch**: `131-quality-metadata-backfill` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/131-quality-metadata-backfill/spec.md`

## Summary

Three tracks, prioritized by audit-image scorecard regression magnitude:

1. **US1 (P1)** — PE/CLR `CustomAttribute` walking (Phase B). Extends the milestone-130
   `pe_clr.rs` reader with the `CustomAttribute` table (token 0x0C) parser per ECMA-335 §II.22.10
   to extract `AssemblyInformationalVersionAttribute` + `AssemblyFileVersionAttribute` strings.
   PURL version routes through the milestone-129 Q3 ladder (Informational > File > 4-tuple).
   Resolves all 373 VERSION_MISMATCH cases on the audit image. ~300 LOC.
2. **US2 (P2)** — License backfill on the three new reader paths. (a) Nested-JAR walker reuses
   the existing `parse_pom_xml`'s `<licenses>` extraction (already extracted by the milestone-009
   top-level path; needs to flow through milestone-130's nested-emit path). (b) PE/CLR reader
   probes the assembly's parent directory for `LICENSE` / `LICENSE.txt` / `LICENSE.md` /
   `COPYING` / `COPYING.txt` (case-insensitive). When found, emit the first 4 KB as
   `PackageDbEntry.licenses` via the existing `SpdxExpression::from_free_text` path with a
   `mikebom:license-source = "package-dir"` annotation. (c) cargo-auditable components from
   `source = "crates-io"` get a `mikebom:license-source = "registry-required"` annotation.
   Targets License Coverage 1/5 → ≥3/5.
3. **US3 (P3)** — Supplier external-reference URL synthesis. Extends the existing
   `mikebom-cli/src/scan_fs/mod.rs::supplier_from_purl` function with new heuristic patterns for
   `pkg:cargo`, `pkg:nuget`, and `pkg:maven` (nested-only via source-mechanism gate). Parses
   cargo-auditable `source = "git+https://..."` for VCS references. Targets Supplier Attribution
   2/5 → ≥3/5.

Per SC-007, each user story is independently shippable. Recommended cadence: three sequential PRs
matching milestone 130's pattern. US1 is the largest single piece (~300 LOC); US2 and US3 are
~150 LOC each.

**Zero new Cargo dependencies.**

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–130; no
nightly required).

**Primary Dependencies**: Existing only — `object = "0.36"` (workspace; reused for the US1
CustomAttribute table extension to the existing pe_clr.rs metadata walker), `quick-xml` (workspace;
reused for US2 nested-JAR `<licenses>` parsing via the existing `parse_pom_xml` helper), `regex`
(workspace; reused for US3 cargo-auditable `git+https://...` source parsing), `tracing`, `anyhow`,
`thiserror`. **No new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; no caches, no persistence. Matches every
milestone since 002.

**Testing**: Standard `cargo +stable test --workspace`. New unit tests inside the three modified
files: `nuget/pe_clr.rs` (US1), `package_db/maven.rs` (US2 nested licenses),
`scan_fs/mod.rs::supplier_tests` (US3). Integration tests deferred — the end-to-end audit-image
scorecard comparison is the acceptance gate (SC-001..SC-004 + SC-006).

**Target Platform**: Linux rootfs (`mikebom sbom scan --image` invariant from milestones 001+).

**Project Type**: CLI tool (the `mikebom sbom scan` polyglot-scanner pipeline). Not the eBPF
trace pipeline.

**Performance Goals**:
- US1 CustomAttribute walking: <5ms per managed DLL (small fixed-size table walks).
- US2 nested-JAR license extraction: zero-cost extension to existing parse_pom_xml call.
- US3 URL synthesis: <1µs per PURL (pure-string template formatting).
- Total scan time growth on the audit image: <30% relative to milestone 130 (per SC-006).

**Constraints**:
- Zero new Cargo dependencies (verified via `cargo tree -p mikebom`).
- Byte-identity preservation across the 33 alpha.48 goldens (verified via
  `./scripts/regen-goldens.sh` producing zero `.cdx.json` / `.spdx.json` churn).
- All three reader paths MUST respect `--offline` and `--exclude-path`.
- US2's package-directory probe per FR-013 is bounded to 4 KB per license file.

**Scale/Scope**:
- US1: ~300 LOC extension to `pe_clr.rs`. New ECMA-335 wire-format parsers for `MemberRef`,
  `TypeRef`, blob-prolog decoder. Predicted reduction: 373 → <20 VERSION_MISMATCH cases.
- US2: ~150 LOC across `nuget/pe_clr.rs` (license-file probing) + `package_db/maven.rs` (nested
  license propagation) + `binary/entry.rs` (cargo-auditable registry-required annotation).
  Predicted lift: 1/5 → ≥3/5.
- US3: ~100 LOC extension to `scan_fs/mod.rs::supplier_from_purl` + tests. Predicted lift:
  2/5 → ≥3/5.
- 1 new `mikebom:*` annotation key catalogued (C96): `mikebom:license-source` with values
  `"package-dir"` / `"pom-xml"` / `"registry-required"` / `"package-dir-no-license"`.

## Constitution Check

Audit against `mikebom Constitution v1.4.0`:

| Principle | Verdict | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ Pass | All extensions Rust; zero new transitive deps. |
| II. eBPF-Only Observation | ✓ N/A | Polyglot-scanner pipeline. |
| III. Fail Closed | ✓ Pass | Parse failures emit `warn` + skip; scan continues. US1's CustomAttribute walk failures fall through to the existing Phase A 4-tuple ladder rung (no silent omission). |
| IV. Type-Driven Correctness | ✓ Pass | All new PURLs flow through `Purl::new`. CustomAttribute parsing uses typed enums for coded-index discrimination. No `.unwrap()` in production. |
| V. Specification Compliance | ⚠ Audit pending | **1 new `mikebom:*` key** (`license-source`) — parity-bridging extension. Neither CDX `licenses[].license.text` nor SPDX `licenseDeclared` has a "where the license came from" sub-field. Catalogued in `contracts/annotation-schema.md` with full Principle V audit narrative. The four cargo / nuget / maven supplier URLs use **standards-native** CDX `externalReferences[].url` directly — no `mikebom:*` annotation for the URLs. |
| VI. Three-Crate Architecture | ✓ Pass | All new code lives in `mikebom-cli`. |
| VII. Test Isolation | ✓ Pass | All tests unprivileged. |
| VIII. Completeness | ✓ Improves | US1 resolves 373 silent VERSION_MISMATCH cases; US2 surfaces licenses pre-130 silently dropped; US3 surfaces supplier URLs pre-130 silently absent. |
| IX. Accuracy | ✓ Pass | All emitted versions / licenses / URLs derived from the artifact itself (Assembly metadata, pom.xml, LICENSE.txt) OR synthesized from PURL via documented registry conventions (crates.io / nuget.org / search.maven.org). No heuristic guessing. |
| X. Transparency | ✓ Pass | `mikebom:license-source` is exactly the kind of transparency annotation Principle X envisions. |
| XI. Enrichment | ✓ Pass | Reuses existing milestone-097 `mikebom:cpe-candidates` channel. |
| XII. External Data Source Enrichment | ✓ Pass | NO external lookups in this milestone. URL synthesis is pure-string PURL templating. The `crates-io = "registry-required"` annotation signals downstream tools (a future deps.dev milestone) where to look — does not consult external sources itself per FR-014. |

**Strict Boundaries**: All four boundaries hold (no lockfile discovery, no MITM, no C deps, no
`.unwrap()` in production).

**Gate verdict**: PASS with the catalog-narrative requirement folded into the plan.

## Project Structure

```text
specs/131-quality-metadata-backfill/
├── plan.md              # This file
├── spec.md              # Feature spec (exists)
├── research.md          # Phase 0 output (this command)
├── data-model.md        # Phase 1 output (this command)
├── quickstart.md        # Phase 1 output (this command)
├── contracts/
│   └── annotation-schema.md     # 1 new mikebom:* key (license-source) with Principle V audit
├── checklists/
│   └── requirements.md  # Spec-quality checklist (exists from /speckit-specify)
└── tasks.md             # Phase 2 output (/speckit-tasks; NOT created by this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   ├── mod.rs                            # MODIFIED — US3 supplier_from_purl extensions
│   │   └── package_db/
│   │       ├── maven.rs                      # MODIFIED — US2 nested-JAR <licenses> propagation
│   │       └── nuget/
│   │           └── pe_clr.rs                 # MODIFIED — US1 CustomAttribute table walking
│   │                                         #          + US2 LICENSE.txt package-dir probe
│   ├── scan_fs/binary/
│   │   └── entry.rs                          # MODIFIED — US2 cargo-auditable license-source annotation
│   └── parity/
│       └── extractors/
│           ├── cdx.rs                        # +1 cdx_anno! entry (C96 mikebom:license-source)
│           ├── spdx2.rs                      # +1 spdx23_anno! entry
│           ├── spdx3.rs                      # +1 spdx3_anno! entry
│           └── mod.rs                        # +1 ParityExtractor slice entry
└── tests/
    └── (unit tests added inline; integration tests deferred)

docs/reference/sbom-format-mapping.md         # +1 C-row (C96) with Principle V audit
CHANGELOG.md                                  # +1 milestone-131 entry under [Unreleased]
```

**Structure Decision**: No new modules. Four surgical extensions to existing modules:
- `nuget/pe_clr.rs` — US1 (CustomAttribute walking) + US2 (LICENSE.txt probing).
- `package_db/maven.rs` — US2 (nested-JAR license propagation through the milestone-130 walker).
- `scan_fs/mod.rs::supplier_from_purl` — US3 (URL synthesis heuristic extensions).
- `binary/entry.rs` — US2 (cargo-auditable license-source annotation).

## Phase 0: Outline & Research

See [research.md](./research.md).

Research topics covered:

1. **US1 CustomAttribute table layout** — ECMA-335 §II.22.10. Columns: `Parent` (HasCustomAttribute
   coded index), `Type` (CustomAttributeType coded index), `Value` (`#Blob` heap reference).
2. **Coded-index resolution** — `Type` resolves through `MemberRef` (token 0x0A) or `MethodDef`
   (token 0x06). For `AssemblyInformationalVersionAttribute`'s `.ctor`, the path is MemberRef →
   TypeRef (token 0x01) → `#Strings` heap.
3. **Custom-attribute blob serialization** — ECMA-335 §II.23.3. Prolog: u16 `0x0001`. Then
   serialized arguments: each fixed argument is encoded inline (for `string` argument: a
   variable-length encoded length-prefix followed by UTF-8 bytes per the SerString format).
4. **US2 license-file probe paths** — `.NET` runtime store packages typically place LICENSE files
   at `/usr/share/dotnet/packs/<name>/<ver>/`. Per-assembly probe walks up from the `.dll`'s
   parent dir up to a reasonable depth (3 levels) looking for case-insensitive
   `LICENSE` / `LICENSE.txt` / `LICENSE.md` / `COPYING`.
5. **US2 nested-JAR `<licenses>` extraction** — The existing `parse_pom_xml` already extracts
   `licenses` (`Vec<License>`) when present. The milestone-130 nested walker calls `parse_pom_xml`
   only for `dependencies` — the licenses field is currently discarded. Fix: thread it through
   the walker's `EmbeddedMavenMeta` struct (add field) or via the existing `extra_annotations`
   channel.
6. **US3 supplier URL conventions** — crates.io: `https://crates.io/crates/<name>/<version>`.
   NuGet: `https://www.nuget.org/packages/<name>/<version>`. Maven Central:
   `https://search.maven.org/artifact/<g>/<a>/<v>/jar`. The cargo-auditable `source` field's
   `git+https://...` form matches the pattern `^git\+(https?://[^#]+)(#[a-f0-9]+)?$`.

## Phase 1: Design & Contracts

See [contracts/annotation-schema.md](./contracts/annotation-schema.md).

**1 new `mikebom:*` annotation key** (C96):
- `mikebom:license-source` — emitted on every component for which milestone 131 attempted license
  extraction. Values: `"package-dir"` (LICENSE.txt found and embedded), `"pom-xml"` (nested-JAR's
  pom.xml carried `<licenses>`), `"registry-required"` (cargo-auditable crates-io entry; license
  available externally), `"package-dir-no-license"` (probed but absent). Catalogued with full
  Principle V audit narrative.

### Data Model

No new entities. US1 extends the existing `ManagedAssembly` struct with `informational_version:
Option<String>` and `file_version: Option<String>` fields. US2 reuses the existing
`PackageDbEntry.licenses: Vec<SpdxExpression>` field. US3 reuses the existing PURL → external-reference
synthesis at `scan_fs/mod.rs::supplier_from_purl`.

### Reader Behavior Contract

- **US1 / `pe_clr.rs`**: after `parse_tables_stream` reads Assembly row 0, walk the
  CustomAttribute table for `Type` rows whose resolved typeref-name equals
  `"AssemblyInformationalVersionAttribute"` or `"AssemblyFileVersionAttribute"`. Decode each match's
  Value blob into a UTF-8 string. Populate the new `ManagedAssembly` fields. PURL version selection
  per FR-008 ladder.
- **US2a / `pe_clr.rs`**: per managed assembly emitted, probe the assembly's parent dir for
  `LICENSE` / etc. variants. When found, read the first 4 KB and emit as
  `PackageDbEntry.licenses` via the existing text-license path. Set
  `mikebom:license-source = "package-dir"`. When not found, set
  `mikebom:license-source = "package-dir-no-license"`.
- **US2b / `maven.rs`**: in the nested-JAR walker's emission site, pass nested-JAR licenses
  (already extracted by `parse_pom_xml`) through to the emitted `PackageDbEntry.licenses`. Set
  `mikebom:license-source = "pom-xml"`.
- **US2c / `binary/entry.rs`**: in `cargo_auditable_packages_to_entries`, for each
  `packages[]` entry whose `source == "crates-io"`, emit
  `mikebom:license-source = "registry-required"` annotation.
- **US3 / `scan_fs/mod.rs::supplier_from_purl`**: add `pkg:cargo`, `pkg:nuget`, `pkg:maven`
  heuristics returning `ExternalReference { ref_type: "website", url: ... }`. For cargo with
  `source = "git+https://..."` parseable, ALSO return a `vcs` reference. Existing golang/gitlab/
  bitbucket heuristics unchanged.

### Agent context update

After Phase 1, the plan invokes `.specify/scripts/bash/update-agent-context.sh claude` which
appends a milestone-131 entry to CLAUDE.md.

## Complexity Tracking

> Nothing requiring complexity-tracking entries. Four surgical extensions to existing modules.
