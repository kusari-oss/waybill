# Feature Specification: Windows-host build + run support

**Feature Branch**: `100-windows-host-build`
**Created**: 2026-05-13
**Status**: Draft
**Input**: User description: "milestone 100 — Windows-host build + run support. Mikebom-cli should build, run, and produce valid SBOMs on Windows. Cross-platform ecosystem readers (cargo, npm, pip, gem, maven, go) and binary scanners (PE/ELF/Mach-O) should work unchanged. Linux-specific readers (dpkg, rpm, apk) are gracefully no-op on Windows. CI gains a Windows lane; release pipeline ships a Windows artifact. Out of scope: Windows-native package managers (winget, MSI, Chocolatey, Scoop) and Windows Registry scanning."

## Background

Mikebom-cli builds and runs on Linux + macOS today. The release pipeline ships pre-built binaries for `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `aarch64-unknown-linux-gnu`. CI tests on `ubuntu-latest` + `macos-latest` runners. A user has requested running mikebom on Windows for scanning Windows development environments and CI workflows.

The good news: most of mikebom is platform-independent Rust. The `object` crate's PE / ELF / Mach-O parsing works the same on any host. The ecosystem readers (cargo, npm, pip, gem, maven, go, npm) read filesystem files — `Cargo.toml`, `package.json`, `requirements.txt`, `Gemfile.lock`, `pom.xml`, `go.sum` — and those files have identical content regardless of which host produced them. The SBOM emitters (CDX 1.6 JSON, SPDX 2.3 JSON, SPDX 3 JSON-LD) are pure JSON serialization. None of this code is platform-specific in practice; it's just that we've never tried compiling it on Windows.

The complications: a small number of files use `#[cfg(unix)]` gates around POSIX-specific code paths — primarily the milestone-004 `dev/inode`-based path-claim dedup in `binary/mod.rs`, which falls back gracefully to the existing canonical-path layer on non-Unix. Linux-specific readers (dpkg, rpm, apk) detect their absence via path checks that naturally return empty results on Windows. The eBPF tracing path (milestones 020+) is already gated behind the `ebpf-tracing` feature flag (disabled by default, Linux-only).

**Scope framing**: enable mikebom-cli to *build, test, and run* on Windows for the cross-platform use cases (Rust / npm / Python / Ruby / Java / Go ecosystems + arbitrary binary scanning). Linux-specific readers continue to compile and silently produce no results on Windows hosts — their target files (`/var/lib/dpkg/status`, `/var/lib/rpm/`, `/lib/apk/db/installed`) simply don't exist there. CI gains a Windows lane; release.yml ships a `mikebom-x86_64-pc-windows-msvc.zip` artifact.

**What this is NOT**: this is not a Windows-native package-manager-reader milestone. Winget, MSI, Chocolatey, Scoop, Windows Registry-based installed-software detection — all out of scope. Those are individually larger features (each comparable in size to milestone 003's dpkg reader) and deserve their own milestones. This milestone is about *running mikebom on a Windows host* for the use cases it already supports.

Out of scope: Windows-native package managers (winget / MSI / Chocolatey / Scoop), Windows Registry scanning for installed software, ARM64 Windows artifact (x86_64-only in v1; ARM64 can follow), eBPF tracing on Windows (not feasible), Docker Desktop / WSL-specific integration paths (Windows-host scanning of WSL filesystems is the user's `\\wsl$\...` path — that just works through the existing FS walker).

## Clarifications

### Session 2026-05-13

- Q: Path-separator emission in SBOM output on Windows hosts — normalize to forward-slash universally OR emit native-host format? → A: A — Normalize to forward-slash in all emitted SBOM JSON strings regardless of host OS. Matches the de facto industry convention (syft + trivy both normalize this way) and the CDX 1.6 / SPDX 2.3 / SPDX 3 example conventions. Preserves cross-host byte-identity goldens (only one CI lane needs to regen them) and avoids noisy diffs when the same source tree is scanned from different hosts. Internal path operations (file opens, canonical-path lookups, walker traversal) continue to use native OS paths — only the strings emitted into SBOM JSON output are normalized.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Windows developer scans a Rust project and gets a valid CDX SBOM (Priority: P1)

A Windows developer downloads `mikebom-x86_64-pc-windows-msvc.zip` from a mikebom GitHub release, extracts `mikebom.exe`, and runs it against their local Rust project's directory. The output is a valid CDX 1.6 JSON SBOM enumerating every cargo dependency in the project's `Cargo.lock`. Same behavior as the existing Linux + macOS paths — no Windows-specific UX changes.

**Why this priority**: this is the headline user request — "I want to run mikebom on my Windows dev box". Cross-platform ecosystem readers cover the dominant developer workflows; Rust + npm + Python projects on Windows are the high-frequency cases.

**Independent Test**: build `mikebom.exe` for `x86_64-pc-windows-msvc`; run on a Windows host against a directory containing `Cargo.toml` + `Cargo.lock`; pipe the output to a JSON parser; confirm the parsed SBOM contains all expected cargo components with valid PURLs.

**Acceptance Scenarios**:

1. **Given** a Windows host with `mikebom.exe` extracted to a directory on PATH, **When** the user runs `mikebom.exe sbom scan --path C:\Users\dev\my-rust-project --output out.cdx.json`, **Then** `out.cdx.json` is a valid CDX 1.6 SBOM containing every `[package]` entry from the project's `Cargo.lock` with `pkg:cargo/...` PURLs.
2. **Given** the same scan target with mixed ecosystems (Rust + npm + Python — typical full-stack project), **When** mikebom runs, **Then** the SBOM emits cargo + npm + pypi components alongside each other (cross-ecosystem coverage works identically to Linux/macOS).
3. **Given** the same scan target, **When** mikebom runs with `--format spdx-2.3` and `--format spdx-3`, **Then** both formats emit valid documents with the same component set and the same identification evidence. Format parity is preserved across host OSes.

---

### User Story 2 — Windows developer scans cross-platform binaries (Priority: P2)

A Windows developer scans a directory containing a mix of binary formats: a Linux ELF binary copied for analysis, a macOS Mach-O fat binary, and the project's compiled `.exe` PE artifacts. Mikebom emits identification evidence for all three formats without complaint — the binary scanner is host-platform-agnostic.

**Why this priority**: P2 because binary scanning on Windows is the natural-extension use case after ecosystem-reader coverage. Developers who collect samples for triage or supply-chain audits often have heterogeneous binary directories; mikebom on Windows should handle them the same as on Linux/macOS.

**Independent Test**: copy known ELF + Mach-O + PE binaries into a temp dir on a Windows host; scan with `mikebom.exe`; expect file-level binary components for all three with their respective `mikebom:binary-class` properties (`elf` / `macho` / `pe`).

**Acceptance Scenarios**:

1. **Given** a Windows directory containing one ELF binary, one Mach-O binary, and one PE binary, **When** mikebom scans it, **Then** the emitted SBOM contains three file-level binary components — one per format — with the milestone-096 + milestone-098 properties (`mikebom:binary-class`, `mikebom:binary-stripped`, `mikebom:binary-packed`, `mikebom:elf-compiler-stamps` for ELF, `mikebom:macho-build-version` for Mach-O, `mikebom:pe-linker-version` for PE) populated where applicable.
2. **Given** the same directory, **When** mikebom scans on Windows AND on Linux against the same input files, **Then** the emitted SBOMs are byte-identical modulo workspace-path differences (the `source_path` will differ because of `/` vs `\` path separators in the JSON `evidence.occurrences[].location` field, but the component set + identification evidence is identical).

---

### User Story 3 — CI + release pipeline ship Windows artifacts (Priority: P2)

Mikebom's CI gains a Windows runner (`windows-latest`) that runs the same `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace` gates as the existing Linux + macOS lanes. The release pipeline gains a Windows build job that produces `mikebom-v<version>-x86_64-pc-windows-msvc.zip` alongside the existing Linux + macOS tarballs.

**Why this priority**: P2 because the user-facing artifact + the regression gate are both required for the Windows port to be durable. Without the CI lane, Windows breakage from future PRs goes undetected; without the release artifact, users have no way to obtain a working `mikebom.exe`.

**Independent Test**: trigger a PR against main with a Windows-incompatible change (e.g., add a `#[cfg(unix)]`-only function that's called unconditionally); confirm the new Windows CI lane fails with a clear error message. Cut a release tag; confirm `release.yml` produces a `mikebom-*-x86_64-pc-windows-msvc.zip` asset attached to the GitHub release.

**Acceptance Scenarios**:

1. **Given** the milestone-100 changes are merged, **When** a future PR introduces Windows-incompatible code, **Then** the Windows CI lane fails with a Rust compile error or test failure clearly attributable to the change. No false positives (no flakes from Windows-specific test timing or path differences).
2. **Given** a `v0.1.0-alpha.X` tag is pushed after milestone 100 lands, **When** `release.yml` runs, **Then** the resulting GitHub release contains a `mikebom-v0.1.0-alpha.X-x86_64-pc-windows-msvc.zip` asset alongside the existing Linux + macOS artifacts.
3. **Given** the published Windows zip is downloaded and extracted on a Windows 10/11 host, **When** `mikebom.exe sbom scan --path .` is run in a directory containing a `Cargo.toml`, **Then** the SBOM is emitted successfully with the same content shape as the equivalent Linux invocation.

---

### Edge Cases

- **`#[cfg(unix)]`-gated code paths**: the existing `binary/mod.rs::is_path_claimed` uses `dev/inode` matching under `#[cfg(unix)]` as a 3rd-layer fallback after raw path + canonical path checks. On Windows the layers 1+2 still work; layer 3 is unreachable but the function compiles and returns the right answer. Other `#[cfg(unix)]` gates exist in `oci_pull/auth.rs`, `docker_image.rs`, `package_db/{rpm,dpkg,pip/dist_info}.rs`, `binary/{linkage,go_binary}.rs`, `package_db/maven.rs` — audit at planning time to confirm each is either compile-clean on Windows or has a matching `#[cfg(not(unix))]` fallback.
- **Path separators**: Rust's `Path` is cross-platform but mikebom may use string operations (`split('/')`, etc.) in some readers. Walking a Windows path like `C:\Users\dev\project\Cargo.toml` produces `pkg:cargo/...` PURLs the same as `/home/dev/project/Cargo.toml` would. **Per the Clarifications path-normalization decision: emitted SBOM JSON strings (`evidence.occurrences[].location`, `source_path`) are forward-slash-normalized regardless of host** — so a scan of `C:\Users\dev\project\Cargo.toml` on Windows emits a `source_path` like `"C:/Users/dev/project/Cargo.toml"` (forward-slash even after the drive letter). The drive-letter prefix is preserved verbatim; only the directory-separator character is normalized.
- **Symlink handling**: Windows symlinks require admin privileges or developer mode to create; `std::fs::canonicalize` works on Windows. Walker behavior should be equivalent to Unix walker behavior; loop-detection (milestone 054) uses platform-independent `HashSet<PathBuf>` keyed on canonical paths.
- **Linux-only readers (dpkg, rpm, apk)**: their entry points check for the existence of `/var/lib/dpkg/status` / `/var/lib/rpm/` / `/lib/apk/db/installed` — paths that don't exist on Windows. Each reader silently returns empty results on Windows hosts. No code changes needed; verified by acceptance scenario US1#1 emitting only cargo components.
- **ELF `.note.package` reader on Windows host**: the section parser is pure-byte-slice logic; works on any host. An ELF binary stored on a Windows filesystem and scanned by Windows-host mikebom produces the same `mikebom:elf-build-id` etc. that Linux-host mikebom would.
- **`MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` on Windows**: the SPDX 3 conformance validator is a Python tool (`spdx3-validate==0.0.5` per milestone 078). Python is cross-platform but the milestone-078 setup script may need Windows-shell adjustments. The validator itself works on any host with Python 3.10+ installed. Document as a soft requirement for Windows-host developers.
- **WSL paths from Windows-host mikebom**: scanning `\\wsl$\Ubuntu\home\dev\project` should "just work" — Windows file APIs make WSL filesystems accessible as UNC paths. No code changes needed.
- **CRLF vs LF line endings in fixture files**: some readers parse text files line-by-line. The cargo / npm / pip readers should accept either line-ending convention; verified by reading content into a String and using `lines()` (which handles both). If any reader is line-ending-sensitive, it's a bug that should fail the new Windows CI lane and get fixed inline.
- **Test fixtures with Unix-style paths**: `mikebom-cli/tests/scan_binary.rs::find_system_binary()` looks for `/bin/ls` and `/usr/bin/ls`. On Windows neither exists; the test gracefully skips per its existing skip clause (`eprintln!("skipping: no /bin/ls found")` + `return`). Other host-binary-finding tests should follow the same pattern; audit at planning time.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `cargo +stable build --target x86_64-pc-windows-msvc -p mikebom` MUST succeed on a Windows host with the workspace's stable toolchain. Zero new Cargo dependencies — the existing dep graph already supports Windows (all major crates the workspace pulls — `object`, `serde`, `clap`, `tokio`, `reqwest`-rustls, etc. — are cross-platform).
- **FR-002**: `cargo +stable test --workspace` on a Windows host MUST report every target's result as `0 failed`. Tests that depend on POSIX-specific fixtures (`/bin/ls`, dpkg directory layout, etc.) gracefully skip on Windows via their existing skip clauses or a new `#[cfg(unix)]` gate; the skip is informative (`eprintln!("skipping on Windows host: <reason>")`).
- **FR-003**: `cargo +stable clippy --workspace --all-targets -- -D warnings` MUST report zero errors and zero warnings on a Windows host.
- **FR-004**: Running `mikebom.exe sbom scan --path <dir>` on a Windows host MUST produce a valid CDX 1.6 JSON SBOM equivalent (component set + identification evidence) to the equivalent Linux invocation against the same input files. Per the milestone-100 path-normalization rule (see Clarifications): emitted JSON path strings (`evidence.occurrences[].location`, `source_path`, etc.) are forward-slash-normalized on every host, so SBOMs are byte-identical across hosts modulo workspace-root prefixes (the per-host scan root differs but is already stripped by the existing cross-host normalize helper).
- **FR-005**: The same `mikebom.exe sbom scan` invocation MUST also produce valid SPDX 2.3 + SPDX 3 SBOMs when called with `--format spdx-2.3` / `--format spdx-3`. Format-parity is preserved across host OSes.
- **FR-006**: The binary scanner (`scan_fs::binary`) MUST identify ELF, Mach-O, and PE binaries on Windows hosts with the same fidelity as Linux/macOS hosts. All milestone-096 / -098 binary-tier annotations (`mikebom:binary-class`, `mikebom:binary-stripped`, `mikebom:elf-*`, `mikebom:macho-*`, `mikebom:pe-*`) emit unchanged.
- **FR-007**: Linux-only readers (`scan_fs::package_db::{dpkg,rpm,apk}` + `scan_fs::docker_image` + `scan_fs::oci_pull`) MUST continue to compile on Windows hosts. When their target files don't exist (the normal Windows-host case), they silently return empty results. Existing `#[cfg(unix)]` gates remain in place; planning-time audit confirms no Windows-incompatible code escapes the gates.
- **FR-008**: CI gains a `lint-and-test-windows` job in `.github/workflows/ci.yml` that runs the same `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace` gate as the existing `lint-and-test-macos` job, on `windows-latest` runners. Runs in parallel with Linux + macOS lanes.
- **FR-009**: `release.yml` gains a `build-windows-x86_64` job that produces `mikebom-v<version>-x86_64-pc-windows-msvc.zip` containing `mikebom.exe`. The job runs on `windows-latest`, uses the same Rust stable toolchain as the existing macOS / Linux jobs, and uploads its artifact to the GitHub pre-release alongside the existing tarballs. ARM64 Windows is deferred to a future milestone.
- **FR-010**: Any `#[cfg(unix)]`-only test that exercises Windows-incompatible behavior (POSIX-specific file modes, Unix sockets, `dev/inode` checks, etc.) MUST be gated `#[cfg(unix)]` at the test-fn level so the Windows CI lane doesn't try to run it. The gate is the existing convention; this milestone audits all tests and adds the gate where missing.
- **FR-011**: Documentation (`README.md`, install instructions, scan examples) MUST be updated to reflect Windows support: install paths (`mikebom.exe`), example commands (`mikebom.exe sbom scan --path C:\Users\...`), and a note that Linux-specific features (eBPF tracing, dpkg/rpm/apk readers) don't apply to the Windows host.

### Key Entities

- **Windows artifact**: the `mikebom-v<version>-x86_64-pc-windows-msvc.zip` release asset. Contains `mikebom.exe` + `SHA256SUMS` reference (same checksum convention as Linux/macOS artifacts).
- **`lint-and-test-windows` CI job**: new GitHub Actions job in `ci.yml` running on `windows-latest` runners. Mirrors the existing `lint-and-test-macos` job's shape.
- **`build-windows-x86_64` release job**: new job in `release.yml` running on `windows-latest`. Produces the zip artifact + uploads to the pre-release.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A Windows developer downloads `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from a GitHub pre-release and runs `mikebom.exe sbom scan --path <dir>` against a Rust project. The emitted CDX SBOM contains every cargo dependency from the project's `Cargo.lock`. Verified end-to-end via the new Windows CI lane's integration tests.
- **SC-002**: A Windows-host mikebom scan of an arbitrary directory containing PE / ELF / Mach-O binaries produces a SBOM with file-level binary components for all three formats. The `mikebom:binary-class` property correctly identifies each.
- **SC-003**: A side-by-side scan of the same input files on Windows + Linux + macOS hosts produces component sets that match exactly. Per the milestone-100 path-normalization rule, emitted JSON path strings are forward-slash-normalized on every host, so cross-host SBOM diff noise is bounded to workspace-root path prefixes (already stripped by the existing cross-host normalize helper). Cross-host byte-identity goldens stay portable.
- **SC-004**: `cargo +stable clippy --workspace --all-targets -- -D warnings` exits with code 0 on `windows-latest` CI runners.
- **SC-005**: `cargo +stable test --workspace` reports every target as `0 failed` on `windows-latest` CI runners. POSIX-specific tests gracefully skip with `eprintln!` skip reasons.
- **SC-006**: The release pipeline's first post-milestone-100 tag produces a `mikebom-*-x86_64-pc-windows-msvc.zip` asset alongside the existing 3 tarballs. The Windows asset's SHA-256 hash appears in the published `SHA256SUMS` file.
- **SC-007**: Zero new Cargo dependencies. The existing dep graph already supports Windows.
- **SC-008**: Zero changes to SBOM output schema (no new properties, no new parity-catalog rows). Windows-host emission produces the same JSON shape as Linux/macOS-host emission for cross-platform inputs.

## Assumptions

- The Rust workspace's entire dep graph (`object`, `serde`, `clap`, `tokio`, `reqwest`-rustls, `chrono`, `regex`, `sha2`, etc.) already supports Windows. Verified at planning time via `cargo tree --target x86_64-pc-windows-msvc -p mikebom` — if any crate is Windows-incompatible (extremely unlikely given the workspace's care to use cross-platform crates), it's a planning-time finding.
- The existing `#[cfg(unix)]` gates correctly isolate POSIX-specific code. Planning-time audit (~10 files identified) confirms each gate has an appropriate `#[cfg(not(unix))]` fallback or is in a code path that's unreachable on Windows.
- Windows-latest CI runners (currently Windows Server 2022) ship with a Rust toolchain installer (`rustup`-based setup-rust-toolchain action). No special Windows-only Rust installation steps.
- The release pipeline's existing zip-artifact convention (`build-macos.yml` uses `tar.gz`; Windows convention is `.zip`) is the only format difference. Both use the same `SHA256SUMS` aggregation.
- Windows users obtain `mikebom.exe` via download — no `winget` / Chocolatey / Scoop package distribution in this milestone (defer to a packaging-focused follow-up).
- Pre-PR gate (`./scripts/pre-pr.sh`) is the same Bash script used on Linux + macOS. Windows developers running the gate locally use Git Bash, WSL, or run `cargo +stable clippy ...` + `cargo +stable test ...` directly. Document the WSL/Git-Bash recommendation in `CLAUDE.md`.

## Dependencies

- **Milestone 004** (binary scanner foundation) — the `object`-crate-based PE / ELF / Mach-O parsing this milestone keeps unchanged on Windows.
- **Milestone 020** (`ebpf-tracing` feature flag) — already gates the eBPF code path; Windows builds simply don't enable the feature. No new gate work needed.
- **Milestone 054** (filesystem walker symlink hang fix) — the platform-independent walker that this milestone keeps unchanged on Windows.
- The existing `lint-and-test-macos` CI job — provides the template for the new Windows job.
- The existing `build-macos-aarch64` release job — provides the template for the Windows build job (with `.zip` instead of `.tar.gz` packaging).

## Out of Scope

- **Windows-native package-manager readers** (winget, MSI, Chocolatey, Scoop). Each would be a separate milestone comparable in size to milestone 003's dpkg reader. Tracked as future enhancements.
- **Windows Registry scanning for installed software** (`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*`). Separate milestone if signal emerges.
- **DLL Side-by-Side (WinSxS) tracking**. Niche use case; defer until requested.
- **ARM64 Windows artifact** (`aarch64-pc-windows-msvc`). x86_64-only in v1; ARM64 can follow once the v1 Windows path is validated.
- **eBPF tracing on Windows**. Windows has no equivalent kernel-level observation framework usable from Rust. Out of scope indefinitely.
- **Linux-host emulation of Windows scanning behavior** (e.g., parsing `.msi` files on a Linux host). Not relevant to this milestone.
- **Docker Desktop / WSL-specific integration paths**. Windows-host mikebom scanning a `\\wsl$\...` UNC path uses the existing FS walker; no Windows-specific integration code needed.
- **Native Windows installer** (MSI / MSIX). Plain `.zip` extract-to-PATH is the v1 distribution mechanism.
- **Code signing / Authenticode for the `mikebom.exe` artifact**. Add separately when the project formalizes its release security posture.
- **PowerShell completion scripts** for `mikebom.exe`. The Bash + Zsh completions (if any) don't translate directly; defer to a separate UX-polish milestone.
- **Per-Windows-version testing matrix** (Windows 10 / 11 / Server 2019 / 2022 separately). v1 tests on `windows-latest` (Windows Server 2022 today); broad version coverage can follow if user signal emerges.
- **Windows-specific binary-scanner enrichments** (e.g., parsing `IMAGE_DIRECTORY_ENTRY_SECURITY` for Authenticode signing chains, reading `MANIFEST` from PE resource sections). These add Windows-specific code beyond what's needed for cross-platform mikebom to work — defer to a Windows-focused enrichment milestone.
- **Pre-PR gate Windows port**. `./scripts/pre-pr.sh` is Bash; Windows developers either run via Git Bash / WSL / PowerShell-equivalent commands manually. A `pre-pr.ps1` could be added in a UX-polish milestone if signal emerges.
