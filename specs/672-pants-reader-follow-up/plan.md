# Implementation Plan: m223 Pants pex-lockfile reader follow-up

**Branch**: `672-pants-reader-follow-up` | **Date**: 2026-09-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/672-pants-reader-follow-up/spec.md`

## Summary

Extend the milestone-223 Pants pex-lockfile reader with two additive capabilities discovered during the Altana (24-resolve Pants 2.33) sanity check:

1. **`//`-comment front-matter stripper** — tolerate the pre-Pants-2.30 lockfile shape by stripping leading `//` lines uniformly (per 2026-09-01 clarify Q3 — always-strip-first, no retry-on-failure branching). Recovers stale-shape files silently dropped today.
2. **`[python.resolves]` bare-string map override** in `pants.toml` — union the map's declared paths into the discovery set alongside the default `3rdparty/python/*.lock` glob. Uses the map key as the emitted `resolve_name` (authoritative over file-stem derivation). Table-shape values WARN-and-skip per 2026-09-01 clarify Q2.

Plus a diagnostic improvement (US3): the `pants-pex reader complete` INFO log fires even on the zero-discovered path when at least one Pants signal is present, and carries a `legacy_shape_lockfiles=<N>` field per FR-013 (log-only in v1 per clarify Q1). No document-scope annotation, no new parity C-row, no golden churn.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–671; no nightly required for this user-space-only reader extension).
**Primary Dependencies**: Existing only — `serde_json` (already used for lockfile parsing), `toml = "0.8"` (already used by cargo/pip/pants config parsers), `tracing`, `anyhow`, `thiserror`. **Zero new Cargo dependencies.** No subprocess calls. No network access.
**Storage**: N/A — all state is in-process for the duration of a single scan (matches every reader milestone since m002).
**Testing**: `cargo test -p waybill` — integration tests via synthetic `tempfile::tempdir()` fixtures (m670 T007 / m671 T012 precedent — no new files under `waybill-cli/tests/fixtures/`). Unit tests inline in `pants/lockfile.rs` + `pants/config.rs` for the stripper + TOML parser paths.
**Target Platform**: Linux + macOS + Windows (same portability envelope as m223 — pure Rust, no OS-specific syscalls).
**Project Type**: Single-crate CLI feature extension inside `waybill-cli` (matches the m223 reader's own crate residency).
**Performance Goals**: SC-007 bounds the stripper at ≤ 5 ms overhead vs. clean-JSON parse on a same-sized file. The typical case (clean JSON starting with `{`) is a single-line peek — bounded to < 1 KB of prefix scan even on 100 MB files.
**Constraints**: Uniform strip pass per Q3 — no fast-path bypass. Byte-identity for post-m223 SBOMs on Pants repos that use only default-shape lockfiles + no override (SC-003 gate).
**Scale/Scope**: Real-world Pants monorepos observed with up to 30 resolves × up to 400 pinned entries per resolve (~ 12 K components). Altana adopter: 24 resolves × ~400 entries avg = ~9,838 components (SC-004 baseline).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I — Pure Rust, Zero C

✅ **PASS**. Zero new Cargo dependencies. No new C bindings. The stripper is a `&[u8]` → `&[u8]` pure function; the TOML map parser reuses the existing `toml = "0.8"` (already at `waybill-cli/Cargo.toml` since m223 shipped).

### Principle II — eBPF-Only Observation

N/A. This milestone touches the `sbom scan` code path only. `waybill-ebpf` is untouched.

### Principle III — Fail Closed

⚠️ **NUANCED**. FR-004 preserves the m223 fail-open contract (per-file parse WARN + skip, scan continues). FR-007/FR-008 extend this to `[python.resolves]` map entries. This is INTENTIONAL — Pants lockfile scanning is best-effort supplemental (the reader augments the default glob, not the whole scan's correctness). Fail-open is documented as the m223 contract at `contracts/pex-lockfile-schema.md`; m672 inherits it unchanged. Documented in Complexity Tracking below with a "not a violation, inherited from m223" annotation.

### Principle IV — Type-Driven Correctness

✅ **PASS**. `PythonResolvesMap = BTreeMap<String, PathBuf>` type-encodes the map shape (map key → path). Non-string values fail deserialization at the `toml::from_str` boundary and never reach the discovery loop. The stripper works on `&[u8]` with no unsafe.

### Principle V — Specification Compliance

✅ **PASS**. This is a reader extension, not a new emission. No CDX/SPDX/SPDX3 wire-format additions. The Q1 clarification (log-line only, defer annotation) explicitly avoids Principle V's "standards-native fields take precedence" audit — because we're not emitting a new field at all.

### Principle VI — Three-Crate Architecture

✅ **PASS**. All changes stay inside `waybill-cli` — the reader lives at `waybill-cli/src/scan_fs/package_db/pants/`. No `waybill-common` or `waybill-ebpf` touches.

### Principle VII — Test Isolation

✅ **PASS**. Every new integration test uses `tempfile::tempdir()` (per m670 T007 / m671 T012 precedent) with synthetic `waybill-fixture-*` package names (per memory `feedback_fixture_synthetic_package_names`). No shared filesystem state.

### Principle VIII — Completeness

✅ **PASS**. The whole reason this milestone exists: recover lockfiles the m223 reader silently drops today (legacy shape + non-default paths). Emits the recovered components' full detail; adds an INFO log field naming the legacy-shape count so operators can drive regeneration.

### Principle IX — Accuracy

✅ **PASS**. Q2 clarification (bare-string values only, WARN-and-skip on tables) prevents accidental misinterpretation of complex `[python.resolves]` shapes. The FR-013 log field is bounded to a `usize` counter — no chance of drift.

### Principle X — Transparency

✅ **PASS**. FR-010 fires the reader-complete log line on every scan where the reader ran, including zero-discovered outcomes. FR-013 adds the `legacy_shape_lockfiles=<N>` field so operators can see how many stale-shape files their repo carries.

### Principle XI — Enrichment

N/A. This milestone doesn't emit new enrichment; it recovers dropped detection.

### Principle XII — External Data Source Enrichment

N/A. Zero network access.

### Development Workflow / Pre-PR Verification

Standard: `cargo +stable clippy --workspace --all-targets` + `cargo +stable test --workspace` + `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` all green before opening the PR.

### Strict Boundaries §5 (file-tier duplicates)

N/A. This milestone touches only the package-tier Pants reader; no file-tier interaction.

**GATE VERDICT**: ✅ **PASS** with a single documented Principle III nuance (fail-open inherited from m223, not a new violation).

### Post-design re-check (2026-09-01, after Phase 1)

Reviewed the data-model + contracts + quickstart against every principle. No new violations surfaced. The design choices tightened rather than widened the surface:

- **`toml::Value` map value type**: uses an existing workspace API (Principle I preserved — no new crate).
- **`DiscoverySource` enum**: 3-variant `Copy` enum; type-driven discriminator for the dedup logic (Principle IV strengthened).
- **`LegacyShapeCounter` newtype**: single `usize` field; no risk of misuse via the `record_stripped` API (Principle IV).
- **Prefix stripper contract C4 (idempotence)**: locks byte-identity on clean-JSON input (Principle V — no wire-format change).
- **Fixture strategy**: synthetic `tempfile::tempdir()` per-test (Principle VII test isolation preserved).

Final verdict: ✅ **PASS**. Ready for `/speckit.tasks`.

## Project Structure

### Documentation (this feature)

```text
specs/672-pants-reader-follow-up/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── front_matter_stripper.md
│   └── python_resolves_map.md
├── spec.md              # Feature specification (already done)
├── checklists/
│   └── requirements.md  # Spec quality checklist (already done)
└── tasks.md             # Phase 2 output (`/speckit.tasks` — not this command)
```

### Source Code (repository root)

Only 4 files touched, all inside `waybill-cli`:

```text
waybill-cli/
├── src/scan_fs/package_db/pants/
│   ├── mod.rs                 # +30 lines — thread `[python.resolves]` map into discovery
│   ├── config.rs              # +20 lines — add `PythonResolvesMap` field to `PythonSection`
│   ├── lockfile.rs            # +25 lines — front-matter stripper prefix pass
│   └── resolve_classifier.rs  # (unchanged)
└── tests/
    └── scan_pants_m672.rs     # NEW — integration tests for US1/US2/US3
```

No other crates touched. No `Cargo.toml` deps changed. No workflow YAML changed.

**Structure Decision**: Additive-only extension of the m223 reader at
`waybill-cli/src/scan_fs/package_db/pants/`. The 4-file surface matches m223's shape 1:1 —
we're extending `lockfile.rs::parse` with a prefix pass, extending `config.rs::PythonSection`
with a new field, and extending `mod.rs::discover_lockfiles` to union the map's paths.
`resolve_classifier.rs` is unchanged.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle III fail-open policy (per-file WARN + skip on parse failure) | Inherited from m223's own contract at `waybill-cli/src/scan_fs/package_db/pants/contracts/pex-lockfile-schema.md`. Pants lockfile scanning is best-effort supplemental — one malformed lockfile must not abort a whole-repo scan. | Fail-closed on any lockfile error would regress every Pants monorepo user (any single stale/malformed file would zero the whole SBOM). This is not a new deviation — m672 preserves the m223 behavior verbatim; Q1's log-line addition is a diagnostic improvement on the fail-open path, not a policy change. |
