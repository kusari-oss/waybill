# Implementation Plan: m673 — extend Pants lockfile discovery to repo-root + `lockfiles/` conventions

**Branch**: `673-pants-lockfile-layouts` | **Date**: 2026-09-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/673-pants-lockfile-layouts/spec.md`

## Summary

Extend the m223 + m672 Pants pex-lockfile reader discovery pipeline
with two additional canonical directories used by Pants 2.31+ default
layouts, and gate the wide-scope paths with a `pex_version`
content-detection check to avoid false-positive WARNs on unrelated
`.lock` files. Discovery becomes a union of three directories: (a)
`<repo-root>/*.lock` (US1), (b) `<repo-root>/lockfiles/*.lock` (US2),
(c) `<repo-root>/3rdparty/python/*.lock` (m223, unchanged). Files
discovered via (a) and (b) go through a content-detection gate per
FR-003; files discovered via (c) and via the m672 `[python.resolves]`
map + `[python].lockfile` singular retain m223's parse-with-WARN
semantics (per 2026-09-02 clarify Q1). Empirically verified against
`pantsbuild/example-python` and `pantsbuild/example-django` where
today m672 emits 0 components from the actual lockfile.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–672; no nightly required).
**Primary Dependencies**: Existing only — `serde_json` (parse the JSON body + pull `pex_version` for content detection), `toml = "0.8"` (already used by config parser), `tracing`, `anyhow`. **Zero new Cargo dependencies.** No subprocess calls. No network access.
**Storage**: N/A — all state is in-process for the duration of a single scan (matches every reader milestone since m002).
**Testing**: `cargo test -p waybill` — integration tests via synthetic `tempfile::tempdir()` fixtures (m672 T008 precedent). Unit tests inline in `pants/mod.rs` for the new discovery-path enumeration + content-detect gate.
**Target Platform**: Linux + macOS + Windows (portability envelope unchanged from m223 + m672 — pure Rust, no OS-specific syscalls).
**Project Type**: Single-crate CLI feature extension inside `waybill-cli` (matches m223 + m672's crate residency).
**Performance Goals**: The wide-scope discovery reads two additional directories per scan. Upper bound: `read_dir` + O(N) filename-extension check + O(K) content-detect where K = number of `.lock` files at repo-root OR under `lockfiles/`. For a real-world repo K < 20; even a pathological repo with 100 `.lock` files at root adds < 100ms of content-detect overhead. No new SC-bounded latency budget beyond m672 SC-007.
**Constraints**: SC-005 byte-identity gate — pre-m672 layout scans MUST produce byte-identical SBOMs to pre-m673.
**Scale/Scope**: Two additional canonical directories × N `.lock` files each. Real-world monorepos observed: 24 resolves under `3rdparty/python/` (m672 shape); 1 resolve at repo-root (`example-python`); 1 resolve under `lockfiles/` (`example-django`). Upper-bound stress case: hypothetical multi-team monorepo with `lockfiles/team-a/*.lock` and `lockfiles/team-b/*.lock` — deferred to v2 (FR-009 non-recursive).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I — Pure Rust, Zero C

✅ **PASS**. Zero new Cargo dependencies. `serde_json` already parses PEX lockfiles at the point-of-parse; content-detection reuses the same crate. No new C bindings.

### Principle II — eBPF-Only Observation

N/A. This milestone touches the `sbom scan` code path only. `waybill-ebpf` is untouched.

### Principle III — Fail Closed

⚠️ **NUANCED**. FR-004's silent-skip on non-PEX `.lock` files IS a fail-open policy for the wide-scope paths — but it's intentional and scoped. Rationale: repo-root and `lockfiles/` are common locations for `.lock` files from OTHER ecosystems (Cargo, Poetry, bun, npm) — WARNing about those would be a UX regression that outweighs the corruption-detection value. The narrow-scope paths (m223 `3rdparty/python/` + explicit overrides) retain WARN-and-skip per the 2026-09-02 clarify decision. Documented in Complexity Tracking below.

### Principle IV — Type-Driven Correctness

✅ **PASS**. `DiscoverySource` enum (m672) gains a new variant `RepoRootGlob` + `LockfilesGlob`. Content-detection returns a `bool` from a pure function — no unsafe. `PexLockfile` deserialization is the only accept-path; failed content-detection early-returns before `serde_json::from_slice::<PexLockfile>` is called.

### Principle V — Specification Compliance

✅ **PASS**. Reader extension only; no CDX/SPDX/SPDX3 wire-format additions. No new parity C-row. No new document-scope annotation. The `pex_version` accept-criterion is the same standards-native discriminator inherited from m223.

### Principle VI — Three-Crate Architecture

✅ **PASS**. All changes stay inside `waybill-cli/src/scan_fs/package_db/pants/`. No `waybill-common` or `waybill-ebpf` touches.

### Principle VII — Test Isolation

✅ **PASS**. Every new integration test uses `tempfile::tempdir()` (per m670 T007 / m671 T012 / m672 T008 precedent) with synthetic `waybill-fixture-*` package names.

### Principle VIII — Completeness

✅ **PASS**. This milestone exists precisely to close a completeness gap: the Pants reader today silently misses 100% of the resolved-detail on any Pants Python monorepo using the 2.31+ default layout. Fixes the gap for the majority-of-Pants-users case.

### Principle IX — Accuracy

✅ **PASS**. FR-003 `pex_version` content-detection prevents accidental mis-parse of non-PEX `.lock` files (Cargo, Poetry, bun). No shape assumption drift.

### Principle X — Transparency

✅ **PASS**. FR-006 extends m672's zero-discovered log path — the presence of a `<repo-root>/lockfiles/` directory OR at least one repo-root PEX lockfile (content-detected) now counts as a Pants signal that fires the diagnostic INFO log. Non-Pants repos remain silent (FR-006 last sentence).

### Principle XI — Enrichment

N/A. This milestone doesn't emit new enrichment; it recovers dropped detection.

### Principle XII — External Data Source Enrichment

N/A. Zero network access.

### Development Workflow / Pre-PR Verification

Standard: `cargo +stable clippy --workspace --all-targets` + `cargo +stable test --workspace` + `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` all green.

### Strict Boundaries §5 (file-tier duplicates)

FR-008 explicitly preserves file-tier walker access to silently-skipped `.lock` files. No boundary violation.

**GATE VERDICT**: ✅ **PASS** with a single documented Principle III nuance (silent-skip on wide-scope FR-001/FR-002 paths — scope-bounded, clarify-Q1-decided, UX-motivated).

### Post-design re-check (2026-09-02, after Phase 1)

Reviewed the data-model + contracts + quickstart against every principle. No new violations. The design choices tightened rather than widened the surface:

- **`is_pex_lockfile_content` pure function**: strict single-responsibility (`&[u8]` → `bool`); no allocation; content-detect gate for wide-scope paths (Principle IV).
- **`DiscoverySource` gains two variants**: `RepoRootGlob` + `LockfilesGlob`. Existing `DefaultGlob` / `PythonLockfileSingular` / `PythonResolvesMap` retain m672 policies. Type-driven per-origin WARN-policy dispatch (Principle IV).
- **Union-then-canonicalize-and-dedup pipeline** unchanged from m672 — the new discovery sources feed the same `dedup_by_canonical_path` pass (Principle IV + Principle VII test isolation preserved).
- **Golden path SBOM byte-identity** locked by SC-005 test: existing m223 + m672 golden fixtures pass unchanged after m673 lands.

Final verdict: ✅ **PASS**. Ready for `/speckit.tasks`.

## Project Structure

### Documentation (this feature)

```text
specs/673-pants-lockfile-layouts/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── discovery_paths.md
│   └── content_detection.md
├── spec.md              # Feature specification (already done)
├── checklists/
│   └── requirements.md  # Spec quality checklist (already done)
└── tasks.md             # Phase 2 output (`/speckit.tasks` — not this command)
```

### Source Code (repository root)

Only 2 source files touched + 1 new integration test file:

```text
waybill-cli/
├── src/scan_fs/package_db/pants/
│   ├── mod.rs                 # +50 lines — new DiscoverySource variants, 2 new discovery loops, content-detect helper, extended signal-detection for FR-006
│   ├── lockfile.rs            # +5 lines — expose `is_pex_lockfile_content` helper (uses existing `strip_pants_frontmatter` + `serde_json::Value` sniff)
│   ├── config.rs              # (unchanged)
│   └── resolve_classifier.rs  # (unchanged)
└── tests/
    └── scan_pants_m673.rs     # NEW — integration tests for US1/US2/US3
```

No `Cargo.toml` changes. No workflow YAML changes.

**Structure Decision**: Purely additive extension of m672's `discover_lockfiles` in `pants/mod.rs`. Two new discovery loops (repo-root + `lockfiles/`) that feed the same `dedup_by_canonical_path` pass. One new pure-function helper `is_pex_lockfile_content` at `pants/lockfile.rs` gates the wide-scope paths. The `DiscoverySource` enum from m672 gains two new variants (`RepoRootGlob`, `LockfilesGlob`). Zero new files under `waybill-cli/src/scan_fs/package_db/pants/`. m672 dedup semantics apply verbatim.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle III fail-open on FR-001/FR-002 wide-scope paths (silent-skip non-PEX `.lock` files) | The wide-scope paths (repo-root + `lockfiles/`) commonly contain non-PEX `.lock` files from other ecosystems (Cargo, Poetry, bun, npm). WARNing about them would be a UX regression that spams the log on the majority of scanned repos — a false positive with high blast radius. Content-detection via the standards-native `pex_version` field is a robust discriminator with no known false-positive shape. | Alternative "WARN-and-skip on parse failure everywhere" (m223's behavior extended to the new paths): rejected because it would flood the log on any Rust or JS repo scanned. Alternative "attempt full parse then decide": functionally equivalent to content-detect but 2× the CPU on the reject path (parses the whole document twice). The FR-003 content-detect is a cheap first-pass that early-exits on rejection. |
