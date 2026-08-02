# Implementation Plan: Pants coursier JVM lockfile reader

**Branch**: `224-pants-coursier-jvm` | **Date**: 2026-08-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/224-pants-coursier-jvm/spec.md`

## Summary

Adds a new source-tier reader at `waybill-cli/src/scan_fs/package_db/pants_jvm/`
that discovers Pants-generated coursier lockfiles (default
`3rdparty/jvm/*.lock`, or `pants.toml`-configured paths) and emits
one `pkg:maven/<group>/<artifact>@<version>` component per locked
distribution. Reuses the m191 reconciler for FR-005 dedup against
the existing Maven reader. Reuses m223-shipped `waybill:pants-resolve`
(C143) + `waybill:source-url` (C144) parity-catalog rows +
extractors verbatim — zero new parity work vs m223.

Zero new Cargo dependencies (coursier lockfiles are TOML; `toml = "0.8"`
already workspace-pervasive). Multi-resolve support with lifecycle-scope
tagging via a JVM-specific dev-tool allowlist. Pants-header
discrimination per FR-011 (distinguishes Pants-generated coursier
lockfiles from standalone coursier output).

**Critical Phase 0 items** (research must resolve):
1. Exact coursier lockfile TOML schema — top-level fields,
   `[[entries]]` shape, `[entries.coord]` optional fields
   (`classifier`, `packaging`, `url`), `[entries.file_digest]` shape.
   Empirically verified against a real Pants JVM sample; document
   for the parser.
2. Coordinate-string parse format for `dependencies[]` +
   `directDependencies[]` — shape is
   `"group:artifact:version[,url=X,jar=Y]"`. Need robust splitting
   that survives future metadata additions.
3. Maven PURL construction — reuse `maven::build_maven_purl` at
   `waybill-cli/src/scan_fs/package_db/maven.rs:2365` (module-private
   today, promote to `pub(crate)` in T003).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from
milestones 001–223; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `toml = "0.8"` (coursier
lockfile TOML parsing; workspace pervasive, already used by m223,
cargo, pip readers), `serde` (Deserialize types), `waybill_common::types::purl::{Purl, encode_purl_segment}`
(PURL construction), `tracing` (INFO/WARN diagnostics), `anyhow`/`thiserror`
(error propagation). **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan; mirrors every
language-reader milestone since 002.
**Testing**: `cargo test --workspace` per Constitution Principle VII
(no privilege escalation, unprivileged CI runners). New test binary
`waybill-cli/tests/pants_coursier_jvm_reader.rs` for integration
coverage; per-module `#[cfg(test)]` blocks for parser + coord-string
+ classifier unit tests. Synthetic fixtures under
`waybill-cli/tests/fixtures/pants_coursier_jvm/` — per `feedback_fixture_synthetic_package_names`,
`dev.waybill.fixture:*` Maven coordinates only.
**Target Platform**: Linux + macOS + Windows (matches m100+, m223).
Lockfile parsing is pure filesystem-read + TOML parse.
**Project Type**: Rust CLI (three-crate workspace per Principle VI).
**Performance Goals**:
- Reader must add <100ms to scan runtime on a repo with a single
  500-entry coursier lockfile (matches m223's per-lockfile budget).
- Default emit path (no coursier lockfiles found) must be
  byte-identical to today's goldens per SC-003 (zero cost when unused).
**Constraints**:
- Byte-identical golden output when no coursier lockfiles present
  (SC-003 / FR-007).
- Fail-open on per-lockfile corruption per FR-006 / SC-005 (WARN +
  skip, not scan-abort).
- No shell-out to `pants` or `coursier` binaries.
- **Zero new parity-catalog rows** — m223 already shipped C143
  (`waybill:pants-resolve`) + C144 (`waybill:source-url`); this
  feature reuses both as-is.
- FR-011: only discover Pants-generated coursier lockfiles (identified
  by the `# --- BEGIN PANTS LOCKFILE METADATA` header). Standalone
  coursier lockfiles skipped with an INFO log.
**Scale/Scope**: 3 user stories (P1/P2/P3), 11 functional
requirements, 6 success criteria. Estimated diff: ~400 LOC production
(reader + coord parser + resolve classifier) + ~250 LOC tests + 5–6
synthetic fixture files. No changes to CLI surface, no new
subcommands, no new parity catalog rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Applies? | Verdict | Notes |
|-----------|----------|---------|-------|
| I. Pure Rust, Zero C | ✅ | PASS | Zero new Cargo dependencies. Existing `toml` handles the format. No transitive C-native additions expected. Verified post-implementation by rerunning the workspace's existing `no_c_dependencies_in_tree` regression test. |
| II. eBPF-Only Observation | ➖ | N/A | User-space reader; `waybill-ebpf` untouched. |
| III. Fail Closed | ✅ | PASS | FR-006 mandates WARN-and-skip on per-lockfile corruption; matches m223 + every existing package-db reader's fail-open-per-file posture (scan-wide-halt is worse UX for repos with mixed valid/corrupt lockfiles). |
| IV. Type-Driven Correctness | ✅ | PASS | Reuses `waybill_common::types::purl::Purl` newtype. New Deserialize types (`CoursierLockfile`, `Entry`, `EntryCoord`, `EntryFileDigest`) all `#[derive(Deserialize)]` with explicit fields — no `toml::Value` bag types in hot path. `#[cfg_attr(test, allow(clippy::unwrap_used))]` at test-mod level per existing convention. |
| V. Specification Compliance | ✅ | PASS | Native-fields-first (Principle V bullet 5): PURLs, hashes go to native slots. `waybill:pants-resolve` + `waybill:source-url` reused from m223 — no new `waybill:*` inventions. |
| VI. Three-Crate Architecture | ✅ | PASS | All new code lands in `waybill-cli`. No new crates. |
| VII. Test Isolation | ✅ | PASS | Reader runs without root/CAP_BPF. Integration tests use synthetic fixtures. No network access (lockfiles are on-disk). |
| VIII. Completeness | ✅ | PASS | Coverage delta: adds JVM discovery for Pants repos where zero components emit today. |
| IX. Accuracy | ✅ | PASS | Source-tier fidelity: lockfile is authoritative; sha256 fingerprints recorded verbatim; `sbom_tier="source"`. No fabrication. |
| X. Transparency | ✅ | PASS | FR-010 INFO log records lockfile count + component count per scan — Principle X signal. WARN diagnostics on per-file corruption name the offending file + reason. FR-011 discriminator log tells operators why a non-Pants coursier lockfile was skipped. |
| XI. Enrichment | ➖ | N/A | Metadata-only feature. |
| XII. External Data Source Enrichment | ➖ | N/A | No external data source. Reader is purely filesystem-local. |

**Result**: PASS on all 12 principles. Zero gate violations. No entries required in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/224-pants-coursier-jvm/
├── plan.md                                    # This file
├── spec.md                                    # /speckit.specify output
├── research.md                                # Phase 0 (this command)
├── data-model.md                              # Phase 1 (this command)
├── quickstart.md                              # Phase 1 (this command)
├── contracts/                                 # Phase 1 (this command)
│   └── coursier-lockfile-schema.md            # Format shape + fail-open contract
├── checklists/
│   └── requirements.md                        # /speckit.specify output (16/16 PASS)
└── tasks.md                                   # /speckit.tasks output (NOT created by this command)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── mod.rs                         # +pub mod pants_jvm; registration + read_all wire-in
│           ├── maven.rs                       # +pub(crate) fn build_maven_purl (promote from fn per T003)
│           └── pants_jvm/                     # NEW module directory
│               ├── mod.rs                     # Public read() entry + orchestrator
│               ├── lockfile.rs                # Coursier TOML parser + entry_to_entry mapping
│               ├── config.rs                  # pants.toml [jvm].default_resolve + [jvm.resolves]
│               ├── coordinate.rs              # Coordinate-string parser
│               │                              # ("group:artifact:version[,url=X,jar=Y]" → coord triple)
│               └── resolve_classifier.rs      # JVM dev-tool allowlist (scalatest, junit, ktlint, ...)
├── tests/
│   ├── pants_coursier_jvm_reader.rs           # NEW integration test file
│   └── fixtures/
│       └── pants_coursier_jvm/                # NEW synthetic fixtures directory
│           ├── minimal_jvm/                   # US1 baseline
│           │   └── 3rdparty/jvm/default.lock
│           ├── multi_resolve/                 # US1 scenario 4: default + junit + scalatest
│           │   └── 3rdparty/jvm/{default,junit,scalatest}.lock
│           ├── pants_toml_custom_path/        # US3: [jvm.resolves] table
│           │   ├── pants.toml
│           │   └── build-support/jvm/prod.lock
│           ├── with_pom_xml/                  # US2: dedup vs pom.xml
│           │   ├── 3rdparty/jvm/default.lock
│           │   └── pom.xml
│           ├── with_classifier/               # Non-default packaging/classifier PURL qualifiers
│           │   └── 3rdparty/jvm/default.lock
│           ├── non_pants_coursier/            # FR-011: skip lockfiles without Pants metadata header
│           │   └── 3rdparty/jvm/default.lock
│           └── corrupt_lockfile/              # SC-005: fail-open on corruption
│               └── 3rdparty/jvm/default.lock
```

**No changes**:
- `docs/reference/sbom-format-mapping.md` — parity rows already exist (C143, C144).
- `waybill-cli/src/parity/extractors/*.rs` — extractors already exist.

**Structure Decision**: Module-directory layout (`package_db/pants_jvm/`)
matches existing multi-file readers (pip, npm, pants — feature 223).
Coordinate-string parsing gets its own `coordinate.rs` file because
the shape is more complex than the Pex `requires_dists` PEP 508
strings (has `,url=X,jar=Y` metadata after the coord triple + colon-
separated group/artifact/version), warranting standalone unit tests.

Reader-surface contract (matches existing readers):
- `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>` at
  `pants_jvm/mod.rs`, called from `scan_fs/package_db/mod.rs::read_all`.
- Emits `PackageDbEntry` per shared struct with
  `sbom_tier: Some("source")`.
- Fail-open on per-file corruption + non-Pants coursier discrimination.

Dedup against the existing Maven reader (`pom.xml`, Gradle,
`~/.m2/`) handled by the m191 reconciler at `scan_fs/mod.rs`; new
reader emits with `sbom_tier="source"` + `hashes.len() > 0` so it
wins the dedup precedence over pom-tier (no hashes). Validated by
m223 US2 for the pip/pants-python analogous case.

## Complexity Tracking

> Populated only if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _none_ | — | — |

## Phase Progression

- [x] Phase 0: research.md generated (5 research items resolved)
- [x] Phase 1: data-model.md, contracts/coursier-lockfile-schema.md, quickstart.md generated + agent context updated
- [x] Constitution re-check post-design: still PASS on all 12 principles

## Follow-ups (out-of-scope for this branch)

- **Standalone coursier lockfile support** (lockfiles produced by
  direct `coursier resolve` CLI without Pants): deferred per FR-011.
  Format differs slightly (no Pants metadata header); usage segment
  is a fraction of Pants-JVM users. Revisit when operator demand
  emerges.
- **Coursier lockfile v2 schema** (hypothetical future Pants format):
  handled reactively — waybill's version guard skips unknown versions
  with a WARN; when Pants ships v2, waybill adds a v2-branch parser.
- **`BUILD` file walker for Pants JVM** (`jvm_artifact(...)`,
  `scala_source(...)`, etc.): design-tier signal that duplicates what
  the lockfile already carries. Deferred until the pants-python
  equivalent (also deferred per m223 follow-ups) has demand.
- **Promote `maven::build_maven_purl` at maven.rs:2365 to `pub(crate)`**:
  filed as future work when a real consumer emerges (Gradle reader,
  hypothetical Bazel-Maven bridge). One-line change; deferred to
  avoid dead-code-at-merge per finding A1 from `/speckit-analyze`.
- **eBPF trace of `coursier` / `pants` build subprocesses**:
  cross-cutting build-observation feature; not JVM-specific.
