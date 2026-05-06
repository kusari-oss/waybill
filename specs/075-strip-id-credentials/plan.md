# Implementation Plan: Strip userinfo credentials from auto-detected git URLs

**Branch**: `075-strip-id-credentials` | **Date**: 2026-05-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/075-strip-id-credentials/spec.md`

## Summary

Closes a known information-disclosure path that pre-dated milestone 074: when an operator has a credentialed `git remote get-url` output (e.g., `https://USER:TOKEN@github.com/foo.git`), milestones 073/074 currently embed the full URL — including the secret — into emitted SBOMs. SBOMs are typically published artifacts; the secret leaks publicly as a side-effect.

This milestone adds a single sanitization step in the auto-detect pipeline that strips RFC 3986 userinfo from auto-detected URLs before the identifier is constructed. Manual operator-supplied values stay verbatim (operators who type credentials own that choice). One new opt-out flag `--keep-credentials-in-identifiers` exists for operators with non-sensitive credentials they want preserved.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–074; no nightly).
**Primary Dependencies**: Existing only — the `url = "2.5.8"` crate is already in the dependency closure (transitive via `reqwest`); this milestone promotes it to a direct workspace dependency. No new transitive deps. Plus `tracing` (info-level logs), `anyhow` (error propagation), `clap` (the new boolean flag — `Args`-derive picks it up). **No additions to the dependency tree at the lockfile level.**
**Storage**: N/A — sanitization is a pure-function transformation; no caches, no persistence.
**Testing**: `cargo +stable test --workspace`. New integration tests in `mikebom-cli/tests/identifiers_credential_strip.rs` reuse the tempdir-based git fixture pattern from milestones 073/074 and assert on emitted SBOM JSON content.
**Target Platform**: Linux (CI primary), macOS (developer workstations). Sanitization logic is OS-agnostic; depends only on RFC 3986 URL parsing and string manipulation.
**Project Type**: CLI tool — single workspace, three crates (`mikebom-cli` is the only one touched).
**Performance Goals**: Sanitization adds <1ms per identifier — bounded by `url::Url::parse` + two `set_*` calls. Negligible against the existing `git` subprocess invocation cost.
**Constraints**: Determinism per FR-010 (same input → byte-identical output). Soft-fail per FR-009 (parse failure routes to milestone 073's existing `UserDefined` path; never fails the scan). No regression on existing milestone 073/074 byte-identity goldens per SC-008 (none of the existing fixtures has a credentialed remote per milestone 074's T001 audit).
**Scale/Scope**: One new function (`sanitize_userinfo`) of estimated ~25 LOC; two call sites updated (`discover_repo_url`, `auto_detect_build_tier_identifiers`); two CLI flag additions (one each on `ScanArgs` and `RunArgs`); one new integration-test file (~250 LOC); zero golden regen.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.4.0 (last amended 2026-05-01). All twelve principles + four strict boundaries reviewed:

| Principle | Status | Justification |
|-----------|--------|---------------|
| I. Pure Rust, Zero C | ✅ Pass | Rust-only sanitization. The `url` crate is pure Rust. |
| II. eBPF-Only Observation | ✅ Pass / N/A | This milestone touches identifier metadata, not dependency discovery. The eBPF trace is unchanged. |
| III. Fail Closed | ✅ Pass | Sanitization parse-failure path soft-fails through milestone 073's existing `UserDefined` rule (FR-009). Scan integrity is unaffected. |
| IV. Type-Driven Correctness | ✅ Pass | The `SanitizedUrl` shape from data-model.md uses an explicit struct holding `(original, sanitized, was_sanitized)`. No raw `String` boundary crossings. Production code uses `anyhow::Result`; tests retain the established `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention. |
| V. Specification Compliance | ✅ Pass | **Native-first audit (constitution v1.4.0 5th bullet):** Sanitization changes the *value* of identifiers that already ride existing standards-native carriers from milestones 073/074. No new fields, no new annotations, no new `mikebom:*` properties. The `source_label` augmentation flows through the existing `Identifier::source_label: Option<String>` field. |
| VI. Three-Crate Architecture | ✅ Pass | All changes in `mikebom-cli`. |
| VII. Test Isolation | ✅ Pass | Sanitization is a pure function; tests need no privilege. Integration tests use tempdirs. |
| VIII. Completeness | ✅ Pass / N/A | Doesn't affect dependency discovery. |
| IX. Accuracy | ✅ Pass | Sanitization preserves the FR-010 soft-fail-to-`UserDefined` rule from milestone 073 — malformed URLs still classify correctly without falsely-`Builtin` emission. |
| X. Transparency | ✅ Pass | Sanitization fires audibly: info-level log line per affected identifier (FR-006), augmented `source_label` reflects the action (FR-008), opt-out is logged when used (FR-007). |
| XI. Enrichment | ✅ Pass / N/A | Not enrichment. |
| XII. External Data Source Enrichment | ✅ Pass / N/A | `git remote get-url` is local subprocess output, not an external data source. |

| Strict Boundary | Status |
|-----------------|--------|
| 1. No lockfile-based dependency discovery | ✅ Pass |
| 2. No MITM proxy | ✅ Pass |
| 3. No C code | ✅ Pass |
| 4. No `.unwrap()` in production | ✅ Pass — sanitization uses `Url::parse` which returns `Result`; production code propagates via `?` or maps to `None` per the existing soft-fail pattern. Tests use the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard. |

**Gate result: PASS.** No violations; no Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/075-strip-id-credentials/
├── plan.md                         # This file
├── spec.md                         # /speckit.specify output
├── research.md                     # Phase 0 output
├── data-model.md                   # Phase 1 output
├── quickstart.md                   # Phase 1 output
├── contracts/
│   └── credential-strip.md         # Phase 1 output — single CLI/lib contract
├── checklists/
│   └── requirements.md             # Already passing
└── tasks.md                        # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

The milestone touches four production files plus one new integration-test file. No new modules, no new crates.

```text
Cargo.toml                                          # MODIFY (small) — promote
                                                    # url = "2" to workspace dep
mikebom-cli/
├── Cargo.toml                                      # MODIFY — consume workspace.url
├── src/
│   ├── binding/
│   │   └── identifiers/
│   │       └── auto_detect.rs                      # MODIFY — add sanitize_userinfo
│   │                                               # helper; wire into both
│   │                                               # discover_repo_url and
│   │                                               # auto_detect_build_tier_
│   │                                               # identifiers; thread an
│   │                                               # opt-out boolean parameter.
│   └── cli/
│       ├── scan_cmd.rs                             # MODIFY — add the
│       │                                           # --keep-credentials-in-
│       │                                           # identifiers flag to
│       │                                           # ScanArgs; pass to auto-
│       │                                           # detect call site.
│       └── run.rs                                  # MODIFY — same flag on
│                                                   # RunArgs; pass to build-
│                                                   # tier auto-detect call.
└── tests/
    └── identifiers_credential_strip.rs             # NEW — integration tests
                                                    # for SC-001..SC-008. ~10
                                                    # tests covering: source-
                                                    # tier strip, build-tier
                                                    # strip, manual verbatim,
                                                    # opt-out preserves, SSH-
                                                    # form unchanged, log-
                                                    # line redaction, source_
                                                    # label augmentation,
                                                    # parse-failure soft-fail.

docs/reference/identifiers.md                       # MODIFY (small) — add
                                                    # subsection on credential
                                                    # sanitization + opt-out
                                                    # flag documentation.
```

**Structure Decision**: Single project. Extends `mikebom-cli` with no new modules. Smallest-possible-surface-change, matching the milestone-074 posture.

## Phase 0 — Research questions

Implementation-level decisions to pin in `research.md` before Phase 1 design.

1. **`url` crate API surface for userinfo manipulation** — `Url::set_username("")` plus `Url::set_password(None)` is the documented way to strip both halves. Confirm both calls return `Result`, document the failure mode (cannot-be-base URLs reject these calls), and pin the sanitize function's error-handling shape.
2. **Promotion strategy for the `url` crate** — `url = "2.5.8"` is in the lockfile transitively. Adding it to `[workspace.dependencies]` and `mikebom-cli` `[dependencies]` should be a one-line addition each; verify cargo's dedup works as expected and the lockfile doesn't churn.
3. **Sanitization sentinel for the source_label** — exact string to append: `(credentials stripped)`. Pin so goldens don't drift later.
4. **Log-line shape and redaction marker** — exact `tracing::info!` template. Decide between `<userinfo redacted>` vs `[redacted]` vs other forms; pick one that matches existing project conventions.
5. **Opt-out flag plumbing** — `clap`'s `bool` derive default-false pattern. The flag lives on `ScanArgs` and `RunArgs`; needs identical handling in both. Decide whether a workspace-level shared `IdentifierOpts` struct is worth creating now or deferred.
6. **Edge case: SSH form vs `Url::parse`** — `git@github.com:foo/bar.git` is not RFC 3986 compliant and `Url::parse` rejects it. Confirm the sanitize function returns the original string verbatim on parse failure (passthrough), not None — preserving the milestone-073 soft-fail semantics for downstream classification.

## Phase 1 — Design & contracts

### data-model.md

One new conceptual entity (`SanitizedUrl`, materialized as a small `(original, sanitized, was_sanitized)` return shape). One new param threading concept (`CredentialOptOut` boolean propagated through the auto-detect call chain). Both compose existing milestone-073/074 types.

### contracts/credential-strip.md

The milestone's only contract. Documents:
- The new public function signature `pub fn sanitize_userinfo(url: &str) -> SanitizedUrl`.
- The CLI flag contract for `--keep-credentials-in-identifiers` on both `mikebom sbom scan` and `mikebom trace run`.
- The integration boundary: where `sanitize_userinfo` is called within `discover_repo_url` and `auto_detect_build_tier_identifiers`, and how the opt-out boolean threads through.
- Observable contracts: log-line phrasing, `source_label` augmentation rules, identifier-emission shape (unchanged from 073/074 modulo the URL value).

### quickstart.md

Operator-facing recipes:
1. **Default behavior — credentials stripped** (the headline).
2. **Manual flag emits verbatim** (the symmetric rule).
3. **Opt-out flag preserves credentials** (for operators on internal networks).
4. **SSH-form URLs unchanged** (sanity-check the no-op path).
5. **What to look for in the emitted SBOM** (jq snippet showing `externalReferences[type:vcs].url` is sanitized, `comment` field has `(credentials stripped)` suffix).

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` after Phase 1 docs land. Expected: appends a `075-strip-id-credentials` row to the "Active Technologies" table in `CLAUDE.md`.

## Phase 2 — Out of scope for this command

`/speckit.plan` ends here. The next command (`/speckit.tasks`) consumes plan.md + spec.md + the Phase 1 docs and emits `tasks.md`. Estimated task count: ~10-12 (smaller than milestone 074's 14 because no refactor, no `resolve_identifiers` generalization, no new `Vec<Identifier>`-shape change).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**

Not applicable — Constitution Check passes on all twelve principles + four strict boundaries with zero violations.
