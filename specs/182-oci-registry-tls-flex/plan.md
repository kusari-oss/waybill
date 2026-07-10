# Implementation Plan: OCI registry TLS + transport flexibility

**Branch**: `182-oci-registry-tls-flex` | **Date**: 2026-07-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/182-oci-registry-tls-flex/spec.md`

## Summary

Add three CLI flags to `mikebom sbom scan` that unblock the Harbor plugin work:
- `--insecure-registry <host[:port]>` — repeatable per-host; enables `http://` instead of `https://` for matching manifest/blob URLs (Harbor devenv target).
- `--registry-ca-cert <path>` — repeatable; loads additional PEM certs into the trust store additively on top of webpki-roots (Harbor prod with company CA).
- `--insecure-tls-skip-verify` — boolean; disables cert chain/hostname/expiry verification (CI/dev with self-signed certs). Emits a WARN log per Constitution Principle X.

All three flags thread through a single `RegistryTlsConfig` struct via `pull_to_tarball` → `RegistryClient::new`, matching the existing `creds_dir` pattern (m034/#66). The `RegistryClient::new` receives the config and consults it (a) when constructing the `reqwest::Client` (`.add_root_certificate` per CA + `.danger_accept_invalid_certs(true)` if skip-verify) and (b) inside `manifest_url` / `blob_url` when picking the scheme. Bearer + Basic auth (already implemented) is untouched.

Zero SBOM emission changes (Principle V N/A). Zero new Cargo dependencies for the shipped binary — `reqwest`'s existing `rustls-tls` feature already exposes both `.add_root_certificate` and `.danger_accept_invalid_certs`. Test dev-deps may add `rcgen` for throwaway-CA generation, contingent on Phase 0 research (fallback: shell out to `openssl req` inside test setup).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–181; no nightly required).

**Primary Dependencies**: Existing only for the shipped binary — `reqwest = "0.12"` with `rustls-tls` feature (workspace-level, already at `default-features = false, features = ["json", "rustls-tls", "blocking"]`), `rustls`/`rustls-pemfile` (transitive via `rustls-tls`; already in the dep graph), `clap` for CLI parsing (workspace), `tracing` for the FR-007 WARN log, `anyhow`/`thiserror` (error propagation). **Dev-dep addition candidate**: `rcgen = "0.13"` for generating throwaway CAs in US2/US3 test fixtures — decided in Phase 0 research (fallback: shell-out to `openssl req` in test setup, avoiding the new dep).

**Storage**: N/A — all state in-process per scan. The three flags are per-invocation only per spec Assumption 1; no config file, no persistent state.

**Testing**: `cargo +stable test --workspace`. New tests: (a) clap-level unit tests for the three new flags (`ScanArgs` parsing); (b) unit tests for the host-matcher predicate + PEM parsing + URL-scheme decision; (c) integration tests using `wiremock` (dev-dep since m055) for the mock plain-HTTP registry + `rcgen`-generated CA for the mock private-CA registry, with end-to-end scans against each; (d) an integration test for FR-013 (URL scheme in `--image` does NOT implicitly enable insecure).

**Target Platform**: Same as every prior mikebom milestone — Linux + macOS user-space, no Windows-specific behavior.

**Project Type**: CLI + library (three-crate workspace: `mikebom-cli`, `mikebom-common`, `mikebom-ebpf` — last untouched).

**Performance Goals**: Zero perceptible regression. Custom CA loading is a one-time cost at scan startup (parse PEM, add each cert to builder). Insecure-registry match is O(1) hash lookup per URL. Skip-verify is a builder flag with no per-request cost.

**Constraints**: (1) SC-004 byte-identity gate — a public-registry scan with NONE of the m182 flags MUST produce byte-identical CDX/SPDX/SPDX3 output pre-vs-post m182. (2) FR-007 WARN log MUST fire when skip-verify is active (Principle X). (3) FR-014 error message quality — each of the three failure modes MUST name the specific fix flag(s). (4) Principle III — default behavior UNCHANGED; all three flags are opt-in escape hatches.

**Scale/Scope**: 4 user stories (3 P1 + 1 P2 composition guard), 14 FRs, 8 SCs. Estimated ~25-30 tasks across 7 phases (setup + foundational config struct + US1 plain-HTTP + US2 custom-CA + US3 skip-verify + US4 composition guard + polish).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)**: ✅ PASS. No new C dependencies. `rustls`/`rustls-pemfile` are pure Rust.
- **Principle II (eBPF-Only Observation)**: ✅ N/A. m182 is CLI + transport plumbing; no discovery-source changes.
- **Principle III (Fail Closed)**: ✅ PASS. Default (no flags set) is UNCHANGED — public-CA HTTPS only. Every degraded-security mode is opt-in via explicit CLI flag. FR-014 mandates actionable error messages when the operator hits a failure without the flag, so degraded modes are discoverable but never silent.
- **Principle IV (Type-Driven Correctness)**: ✅ PASS. `--insecure-registry` values parsed into a typed `HostMatcher` struct (host-only vs host:port variants — see data-model.md §2). `--registry-ca-cert` paths validated at parse time before the network call (FR-006 fail-fast). `--insecure-tls-skip-verify` is `bool`. No raw string threading through the OCI pull code path — the three flags collapse into a single `RegistryTlsConfig` struct passed to `RegistryClient::new`.
- **Principle V (Specification Compliance / Native-First)**: ✅ N/A — no SBOM emission changes. Zero new `mikebom:*` fields, zero SPDX/CDX/SPDX3 output modifications.
- **Principle VI (Three-Crate Architecture)**: ✅ PASS. All source changes localized to `mikebom-cli/src/cli/scan_cmd.rs` (flag definitions) + `mikebom-cli/src/scan_fs/oci_pull/{mod.rs, registry.rs}` (config threading + client construction). No new crates. `mikebom-common` untouched. `mikebom-ebpf` untouched.
- **Principle VII (Test Isolation)**: ✅ PASS. All new tests are unit + integration under `cargo test --workspace` — no privileged tests. `wiremock`-based fixtures run in-process; no network egress required for CI (satisfies air-gapped test environments).
- **Principle VIII (Completeness)** + **IX (Accuracy)**: ✅ N/A — m182 does not change what mikebom discovers or classifies, only HOW it reaches the registry.
- **Principle X (Transparency)**: ✅ PASS. FR-007 mandates a WARN-level structured log when `--insecure-tls-skip-verify` is active. FR-014 mandates actionable error messages for the three failure modes. The operator can always audit AFTER a scan to determine which m182 flags were in effect.
- **Principle XI (Enrichment)** + **XII (External Data Source Enrichment)**: ✅ N/A. m182 is CLI + transport only.
- **Strict Boundaries §1 (No lockfile-based discovery)**: ✅ N/A.
- **Strict Boundaries §4 (No `.unwrap()` in production)**: ✅ PASS. New code paths use `anyhow::Result` + `.context()` propagation.

**Result**: All gates PASS. Phase 0 authorized.

## Project Structure

### Documentation (this feature)

```text
specs/182-oci-registry-tls-flex/
├── plan.md              # This file
├── spec.md              # Feature spec (16/16 checklist PASS)
├── research.md          # Phase 0 output: design decisions + peer-tool audit
├── data-model.md        # Phase 1 output: type definitions + threading pattern
├── quickstart.md        # Phase 1 output: operator + developer flows
├── contracts/           # Phase 1 output
│   ├── cli-flags.md                    # The three flag signatures + parse semantics
│   └── registry-tls-config.md          # The internal config struct + threading contract
├── checklists/
│   └── requirements.md  # Spec quality checklist (16/16 PASS)
└── tasks.md             # Phase 2 output (populated by /speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/cli/
└── scan_cmd.rs           # 3 new clap arg fields added to ScanArgs (next to
                          # `registry_credentials_dir` at line 172); plumbed
                          # into a new `RegistryTlsConfig` struct that flows to
                          # pull_to_tarball → RegistryClient::new. New clap-parse
                          # unit tests in the existing test module (~line 3500).

mikebom-cli/src/scan_fs/oci_pull/
├── mod.rs                # `pull_to_tarball` signature gains
│                         # `tls_config: RegistryTlsConfig` alongside creds_dir
│                         # (matches m034 pattern). Threading only — no logic
│                         # change here.
├── registry.rs           # PRIMARY change site. RegistryClient::new consumes
│                         # RegistryTlsConfig: (a) builds reqwest::Client with
│                         # .add_root_certificate() per CA + .danger_accept_
│                         # invalid_certs() if skip-verify; (b) stores an
│                         # InsecureRegistryMatcher on the client instance;
│                         # (c) manifest_url + blob_url delegate scheme
│                         # selection to the matcher. New helper functions:
│                         # scheme_for_registry(), load_ca_bundle_from_path(),
│                         # emit_tls_warn_log_if_needed().
└── tls_config.rs         # NEW file — types + parsing:
                          #   - RegistryTlsConfig struct
                          #   - HostMatcher enum (HostOnly / HostPort)
                          #   - InsecureRegistryMatcher (Vec<HostMatcher>)
                          #   - parse_insecure_registry_flag() helper
                          #   - load_ca_bundle_from_paths() helper
                          #   - Unit tests for parsing + matching semantics

mikebom-cli/tests/
├── oci_pull_plain_http.rs                # NEW: US1 end-to-end via wiremock
├── oci_pull_custom_ca.rs                 # NEW: US2 end-to-end via rcgen+wiremock
├── oci_pull_skip_verify.rs               # NEW: US3 end-to-end via rcgen+wiremock
├── oci_pull_flag_composition.rs          # NEW: US4 composition regression guard
└── oci_pull_backward_compat.rs           # NEW: SC-004 byte-identity gate (no-flag
                                          # scan produces byte-identical SBOM)

docs/reference/reading-a-mikebom-sbom.md  # UNCHANGED — no SBOM emission change
docs/reference/                            # OPTIONAL: extend with a new
                                          # "Registry TLS options" reference page
                                          # (T031 polish task). Prior art:
                                          # docs/reference/identifiers.md already
                                          # covers the --registry-credentials-dir
                                          # credential-resolution chain.
```

**Structure Decision**: Single three-crate workspace (existing). No new crates. All source changes localized to `mikebom-cli/src/cli/scan_cmd.rs` + `mikebom-cli/src/scan_fs/oci_pull/` (2 existing files + 1 new file for the tls_config types). Test changes span 5 new integration test files. `mikebom-common` + `mikebom-ebpf` untouched.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

All gates PASS. No justification table needed. Complexity note (not a violation): the new file `tls_config.rs` introduces types that could arguably live directly in `registry.rs`. Choosing a separate file because (a) unit tests for the parser + matcher are cleanest colocated with the types, and (b) the flags are a distinct semantic layer (transport config) from the HTTP client (transport execution). Keeping them separated matches the existing `auth.rs` / `cache.rs` split.

## Phase 0: Design Decisions (Research)

**Output**: `research.md` covering 5 decisions:

### Decision 1: `RegistryTlsConfig` threading pattern

Match the existing `creds_dir: Option<&Path>` pattern by adding a new struct parameter to `pull_to_tarball` and `RegistryClient::new`. The struct is built at CLI-parse time from `ScanArgs` and immutable for the scan duration. Rejected alternatives: (a) global mutable state (violates Principle IV), (b) thread-local (adds test-isolation surface), (c) callback-based scheme resolver (over-engineered for a static config).

### Decision 2: PEM parsing library

Use `rustls-pemfile` (already in the dep graph transitively via `rustls-tls`). Verify at Phase 0 that `reqwest::Certificate::from_pem` handles multi-cert PEM bundles correctly per FR-006. Fallback: iterate `rustls_pemfile::certs(&mut BufReader::new(bytes))` and build a `Vec<reqwest::Certificate>`, adding each with a separate `.add_root_certificate()` call.

### Decision 3: `HostMatcher` semantic

Parse each `--insecure-registry <val>` occurrence into either:
- `HostMatcher::HostOnly(String)` when `val` has no `:` — matches any port on that host
- `HostMatcher::HostPort(String, u16)` when `val` has an explicit `:<port>`

The scheme decision (`http://` vs `https://`) inside `manifest_url`/`blob_url` consults the `InsecureRegistryMatcher::matches(host, port) -> bool` predicate. Matches podman's `[[registry]] location =` behavior per spec FR-004.

### Decision 4: URL-scheme-in-`--image` policy

Per spec FR-013: explicit m182 flag REQUIRED. `--image http://core:8080/…` alone does NOT enable insecure mode. Rationale: docker's mental model > podman's for surprise-free operator experience. The URL scheme is a hint mikebom already parses (via `reference::parse_reference` at oci_pull/mod.rs:93), but that parse feeds the reference struct — NOT the transport decision. If the operator wants HTTP, they type `--insecure-registry`.

### Decision 5: Testing infrastructure — `rcgen` vs `openssl` shell-out

Prefer `rcgen = "0.13"` as a dev-only Cargo dep. Rationale: pure-Rust, no C toolchain requirement in CI, generates test certs in-process (no tempfile juggling, no shell dependency). Fallback if the dep addition is contested at review: shell out to `openssl req -x509 -newkey ...` in test setup — reproducible on macOS + Linux CI runners which have openssl bundled. Decision confirmed in Phase 0 by checking for existing `rcgen` transitive presence.

## Phase 1: Design & Contracts

### Data Model (`data-model.md`)

- **`ScanArgs`** — extended with three new clap-derived fields matching FR-001/002/003 (see contracts/cli-flags.md for exact clap attributes).
- **`RegistryTlsConfig`** (NEW, in `tls_config.rs`) — the internal struct built from `ScanArgs`, passed to `pull_to_tarball` → `RegistryClient::new`. Fields: `insecure_matcher: InsecureRegistryMatcher`, `ca_bundle: Vec<reqwest::Certificate>` (empty when no flag), `skip_verify: bool`.
- **`HostMatcher`** (NEW enum) — `HostOnly(String) | HostPort(String, u16)`. Parsed from `--insecure-registry <val>`.
- **`InsecureRegistryMatcher`** (NEW) — wraps `Vec<HostMatcher>` with a `.matches(host: &str, port: Option<u16>) -> bool` method.
- **`RegistryClient`** — new field `tls_config: RegistryTlsConfig` (or the extracted matcher + skip-verify state). Consumed by `manifest_url`/`blob_url` to choose scheme.

### Contracts (`contracts/`)

1. **`cli-flags.md`** — exact `#[arg(...)]` clap attributes for the three flags, plus the parse-error diagnostics. Includes the FR-014 error-message templates.
2. **`registry-tls-config.md`** — `RegistryTlsConfig` type definition, threading contract from `ScanArgs` → `pull_to_tarball` → `RegistryClient::new`, and the URL-scheme decision rule.

### Quickstart (`quickstart.md`)

- **Operator flow**: three worked examples matching the SC-001/SC-002/SC-003 scenarios (Harbor devenv, private-CA prod, self-signed CI). Includes the WARN log output snippet from FR-007.
- **Developer flow**: how to add a fourth transport-config flag in a future milestone (mTLS, custom User-Agent, whatever) — the `RegistryTlsConfig` struct is the extension point.

### Agent context update

Run `.specify/scripts/bash/update-agent-context.sh claude` at end of Phase 1.

## Post-Design Constitution Re-check

After Phase 1 design artifacts are written, re-verify:

- **Principle III (Fail Closed)**: Confirmed. Default behavior (no flags) unchanged. All three flags are explicit opt-ins with actionable error diagnostics when the operator hits the failure without them.
- **Principle IV (Type-Driven)**: Confirmed. `RegistryTlsConfig` bundles the three configs into one typed value; `HostMatcher` enum enforces host-only vs host:port parsing at compile time; no raw string threading.
- **Principle X (Transparency)**: Confirmed. FR-007 WARN log + FR-014 error messages give the operator full visibility into which transport modes are active.
- **Backward-compat (SC-004)**: Confirmed via the `oci_pull_backward_compat.rs` integration test — scans with NO m182 flags produce byte-identical SBOM output.

**Post-check result**: All gates hold. Ready for `/speckit-tasks`.
