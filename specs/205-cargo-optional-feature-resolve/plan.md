# Implementation Plan: Fix Cargo Optional-Dep Over-Exclusion — Resolve Feature Activation

**Branch**: `205-cargo-optional-feature-resolve` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/205-cargo-optional-feature-resolve/spec.md`

## Summary

Replace the m179 name-only `optional_names.contains(&pkg.name)` check at `cargo.rs:1155` with a feature-resolved `truly_optional_names.contains(&pkg.name)` check. `truly_optional_names` = `optional_names` MINUS the set of dep names that Cargo's actual resolver activates. The activated set is computed by shelling out to `cargo metadata --format-version 1` in the workspace root and taking the union of `resolve.nodes[].deps[]` names. When cargo is absent or metadata fails, fall back to treating ALL optional deps as `Runtime` (safe over-inclusion per FR-004) with a WARN log naming the workspace path + failure class.

Zero new Cargo dependencies. Bounded change surface: 1 source file edit (cargo.rs adds a `resolve_activated_deps_via_cargo_metadata` helper + wires it into the classifier), 1 test file addition (regression tests for US1 / US2 / US3 + FR-004 warn-and-fallback). Estimated ~200 LOC.

Reconnaissance findings (per m199-m204 lesson):
- Bug site verified at `mikebom-cli/src/scan_fs/package_db/cargo.rs:1155`: `else if prod_set.contains(&key) && optional_names.contains(&pkg.name) { LifecycleScope::Optional }`.
- `optional_names` populated by `collect_optional_dep_keys` at `cargo.rs:813-841` — name-only, no feature-set filtering.
- `cargo metadata --format-version 1` output verified locally against a synthetic workspace (`/tmp/m205-test`): `resolve.nodes[].deps[]` correctly excludes non-activated optional deps (e.g., `regex` optional-but-not-in-default omitted; `serde` optional-but-in-default included). Feature resolution is Cargo's job — we just read its answer.
- `cargo` binary already on PATH in every mikebom dev + CI environment (matches m053 `git describe` + m055 `go mod graph` + m173 `go mod download` + m203 `helm template` shell-out precedents). Constitution Principle I subprocess-usage stays allowed.
- Cargo 1.60+ implicit-features rule verified: an optional dep `foo` gets an implicit feature `foo = ["dep:foo"]` unless explicitly overridden — this rule is fully handled by Cargo's own resolver, so mikebom doesn't need to reimplement it.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–204; no nightly).
**Primary Dependencies**: Existing only — `serde` / `serde_json` (parsing cargo metadata JSON output; workspace-pervasive), `std::process::Command` (subprocess spawn, same pattern as m053 / m055 / m173 / m203), `tracing` (WARN log per FR-004), `anyhow` / `thiserror` (error propagation). **Zero new Cargo dependencies.** No new subprocess types beyond the existing `Command`-with-timeout pattern; no network access; no filesystem writes beyond emitted SBOM output.
**Storage**: N/A — all state in-process per scan; the activated-deps set is computed once per workspace and lives for the classifier decision only.
**Testing**: Unit tests in `cargo.rs::tests` for the new resolver helper + integration tests in a new `mikebom-cli/tests/cargo_optional_feature_resolve.rs` file. US1 / US2 tests build synthetic Cargo workspaces via `tempfile::tempdir()` and shell out to real cargo (matches m087 / m173 test pattern; cargo binary is a hard dev prereq). US3 byte-identity via existing non-Cargo public_corpus fixtures. FR-004 fallback test uses `PATH=""` scrub per m203 US2 precedent. Reporter's `test-vaultwarden` case verified manually per quickstart.md Reproducer 1.
**Target Platform**: Same as mikebom itself. cargo binary is a POSIX-ish assumption already satisfied by every mikebom dev env; Windows CI's smoke tests skip the shell-out-required tests (FR-004 graceful fallback path exercised there anyway via the fallback test).
**Project Type**: Bug fix — reader-side classifier refinement. ~200 LOC total: ~80 LOC in `cargo.rs` (new `resolve_activated_deps_via_cargo_metadata` helper + wiring), ~100 LOC integration tests, ~20 LOC WARN-log fixture for FR-004 verification.
**Performance Goals**: Per-workspace cargo-metadata shell-out is <5s on typical projects (measured on test-vaultwarden: ~2s). SC-005 pre-PR delta ≤ 10s vs baseline (allowing for the added step; if the scan has N Cargo workspaces, we run cargo metadata N times, but the mikebom test suite has 1 Cargo workspace in public_corpus).
**Constraints**: (a) zero new Cargo deps; (b) fallback-on-cargo-absence-with-WARN per FR-004; (c) non-Cargo scans byte-identical per FR-005 / US3; (d) preserve m179's `mikebom:optional-derivation` annotation for TRULY-optional deps per FR-003; (e) deterministic emission per FR-007.
**Scale/Scope**: 2 source-file edits (cargo.rs + new integration test file). No changes to mikebom-common. No changes to mikebom-ebpf. No changes to any emitter (the fix is purely upstream of the classifier). No changes to the parity catalog (C122 `mikebom:optional-derivation` stays as-is — its emission gate tightens but its shape is stable).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. All new Rust code. Subprocess to `cargo` via `std::process::Command` — same pattern as m053 (`git describe`), m055 (`go mod graph`), m173 (`go mod download`), m203 (`helm template`). `cargo` is an external binary, not a C dependency; mikebom does not link against it.
- **II. eBPF-Only Observation** — ✅ N/A. Cargo reader is a manifest/lockfile-static discovery mechanism (Principle II §2 explicitly permits static lockfile parsing for enrichment; the classifier is not a discovery mechanism, it's a scope-refinement pass on already-discovered components).
- **III. Fail Closed** — ✅ PASS. FR-004 codifies the graceful-fallback path: when `cargo metadata` fails (binary absent, workspace parse error, network needed but --offline), the classifier falls back to treating optional deps as `Runtime` — the SAFE default (over-inclusion, vuln-scanners see the dep). WARN log names the workspace path + failure reason so operators can diagnose. Fail-closed spirit honored: operator gets a scan result AND transparent signal about reduced fidelity. Never silently under-reports vulnerabilities.
- **IV. Type-Driven Correctness** — ✅ PASS. New helper returns a typed `HashSet<String>` (activated dep names) or a typed `CargoMetadataResolveFailure` enum (naming the failure class). No stringly-typed boundaries. Existing `LifecycleScope` + `optional_names` types unchanged.
- **V. Specification Compliance** — ✅ PASS. Zero new `mikebom:*` annotations introduced. The existing `mikebom:optional-derivation` annotation (m179, catalog row C122) continues to fire only when the dep is genuinely Optional — its emission gate tightens but its wire shape is stable. No parity-catalog changes needed. No new native-field-audit obligation.
- **VI. Three-Crate Architecture** — ✅ PASS. All changes stay in `mikebom-cli`.
- **VII. Test Isolation** — ✅ PASS. US1 / US2 integration tests build synthetic workspaces via `tempfile::tempdir()` + shell out to real cargo (present in every mikebom dev / CI env; matches m087 / m173 precedents). US3 byte-identity via existing public_corpus fixtures. FR-004 fallback path tested via a `PATH=""` scrubbed subprocess environment (matches m203's US2 test pattern).
- **VIII. Completeness** — ✅ PASS. This IS a completeness fix — it restores vuln-scanner visibility to actually-shipped optional deps that alpha.63 was silently excluding.
- **IX. Accuracy** — ✅ PASS. This IS an accuracy fix — the classifier now reflects Cargo's actual resolution decisions instead of a manifest-flag heuristic that diverges from build reality.
- **X. Transparency** — ✅ PASS. FR-004 WARN log ensures operators see when the fallback path fires. Every optional-vs-runtime decision continues to carry the `mikebom:optional-derivation` annotation on TRULY-optional deps (so consumers can distinguish "mikebom classified this Optional" from "mikebom didn't touch this scope").
- **XI. Enrichment (DX)** — ✅ PASS. Zero operator-facing CLI surface changes. Zero new flags. Zero new env vars. Purely a classifier bug fix.
- **XII. External Data Source Enrichment** — ✅ N/A. `cargo` is a local binary invoked on local-only workspace state, not an external data source over the network.
- **Strict Boundary §5 (file-tier)** — ✅ N/A. Not touching file-tier plumbing.

**Result**: All principles PASS. No violations. No Complexity Tracking entries needed.

**Post-Phase-1 re-check**: N/A — Phase 1 introduces no new entities beyond what's above (1 new helper function + 1 new error enum). Constitution gate remains PASS post-design.

## Project Structure

### Documentation (this feature)

```text
specs/205-cargo-optional-feature-resolve/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 3 mechanical decisions
├── data-model.md        # Phase 1 output — resolver helper + error enum + classifier delta
├── quickstart.md        # Phase 1 output — 3 reproducers (reporter's case + synthetic activated + synthetic truly-optional)
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory — the fix is upstream of emission; no wire-format contracts change.

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/package_db/
└── cargo.rs                                            # MODIFIED — the entirety of m205:
                                                        #
                                                        # New items (~80 LOC total):
                                                        #   - CargoMetadataResolveFailure enum
                                                        #     (4 variants: BinaryNotFound,
                                                        #     NonZeroExit { code, stderr_head },
                                                        #     ParseError { source }, IoError)
                                                        #   - resolve_activated_deps_via_cargo_metadata(
                                                        #     workspace_root: &Path
                                                        #     ) -> Result<HashSet<String>, CargoMetadataResolveFailure>
                                                        #     — shell out to `cargo metadata
                                                        #     --format-version 1` at workspace
                                                        #     root; parse JSON; return the union
                                                        #     of resolve.nodes[].deps[].name
                                                        #     (dep NAMES actually pulled in per
                                                        #     the resolved feature set).
                                                        #     Follows the m055 subprocess-with-
                                                        #     timeout pattern (thread + mpsc +
                                                        #     recv_timeout).
                                                        #
                                                        # Modified items (~20 LOC):
                                                        #   - parse_lockfile signature gains
                                                        #     an `activated_names: &HashSet<String>`
                                                        #     parameter.
                                                        #   - Classifier check at line 1155:
                                                        #     `optional_names.contains(&pkg.name)`
                                                        #     becomes
                                                        #     `optional_names.contains(&pkg.name)
                                                        #      && !activated_names.contains(&pkg.name)`
                                                        #     — dep is TRULY optional iff declared
                                                        #     optional AND NOT in the actual-activated
                                                        #     set.
                                                        #   - Caller at line 1259 (parse_lockfile
                                                        #     invocation site) runs
                                                        #     `resolve_activated_deps_via_cargo_metadata`
                                                        #     for the workspace root before calling
                                                        #     parse_lockfile; on Err(_), emits
                                                        #     WARN log with `workspace = %ws`,
                                                        #     `reason = %failure_class`, then
                                                        #     populates `activated_names` with
                                                        #     ALL `optional_names` (safe over-
                                                        #     inclusion per FR-004 — every
                                                        #     manifest-declared optional dep flips
                                                        #     to Runtime).

mikebom-cli/tests/
└── cargo_optional_feature_resolve.rs                   # NEW — m205 integration tests:
                                                        #   - us1_default_feature_activated_optional_
                                                        #     dep_is_runtime — build synthetic
                                                        #     Cargo workspace with
                                                        #     `[dependencies] serde = { optional = true }`
                                                        #     + `[features] default = ["serde"]`;
                                                        #     run `cargo generate-lockfile` in
                                                        #     the tempdir; scan; assert `serde`'s
                                                        #     CDX `scope == "runtime"` (NOT
                                                        #     "excluded"); assert NO
                                                        #     `mikebom:optional-derivation`
                                                        #     property on `serde`.
                                                        #   - us2_truly_optional_dep_stays_optional —
                                                        #     build synthetic workspace with
                                                        #     `[dependencies] regex = { optional = true }`
                                                        #     + `[features] enable-regex = ["regex"]`
                                                        #     (NOT in `default`); scan; assert
                                                        #     `regex`'s CDX `scope == "excluded"` +
                                                        #     `mikebom:optional-derivation =
                                                        #     cargo-optional-true`.
                                                        #   - us3_non_cargo_scan_byte_identical —
                                                        #     scan an existing non-Cargo fixture
                                                        #     (e.g., mikebom-cli/tests/fixtures/
                                                        #     public_corpus/npm-express);
                                                        #     assert output byte-identical vs
                                                        #     pre-m205 golden.
                                                        #   - fr004_cargo_absent_warns_and_falls_back —
                                                        #     shell out to mikebom binary with
                                                        #     PATH="" (empty, forces
                                                        #     BinaryNotFound); assert (a) scan
                                                        #     exits 0, (b) stderr WARN log
                                                        #     contains the substring "cargo
                                                        #     metadata" AND "falling back", (c)
                                                        #     previously-Optional dep now appears
                                                        #     as Runtime (safe over-inclusion),
                                                        #     (d) NO `mikebom:optional-derivation`
                                                        #     property emitted for any component
                                                        #     (the classifier couldn't reach the
                                                        #     Optional branch).
                                                        #     `#[cfg(unix)]` per m203 precedent.
```

**Structure Decision**: 1 source file edit + 1 new integration test file + 0 fixture files (all tests build synthetic Cargo workspaces via `tempfile::tempdir()` per m087 / m173 / m203 precedent). Zero committed fixture regen needed for the fix itself; the reporter's `test-vaultwarden` case is verified manually via quickstart Reproducer 1 (external repo, not shipped with mikebom).

## Complexity Tracking

No constitution violations. All principles pass on first check. The subprocess pattern is precedented four times over (m053, m055, m173, m203); no new architectural choices needed. FR-004's graceful-fallback-with-WARN mirrors m203's helm-render-fallback pattern verbatim (Constitution Principle III + X).
