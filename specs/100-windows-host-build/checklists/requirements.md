# Specification Quality Checklist: Windows-host build + run support

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-13
**Feature**: [Link to spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`
- Scope is tight: enable mikebom-cli to build + run + emit valid SBOMs on Windows for cross-platform use cases. No new readers, no schema changes, no Windows-native package-manager integration.
- 11 out-of-scope items enumerated explicitly — Windows-native package managers (winget/MSI/Chocolatey/Scoop), Registry scanning, ARM64 Windows, eBPF, code signing, MSI installer, PowerShell completions, etc.
- 8 edge cases enumerated covering the known Windows/POSIX boundary points: `#[cfg(unix)]` gates, path separators, symlinks, Linux-only readers' graceful no-op, ELF-on-Windows-host scanning, SPDX 3 validator, WSL paths, CRLF/LF line endings.
- 8 SCs all measurable + technology-agnostic.
- 11 FRs all testable + carry the proportional rationale.
- The implementation will require:
  - Audit of ~10 `#[cfg(unix)]`-gated files (binary/mod.rs, oci_pull/auth.rs, docker_image.rs, package_db/{rpm,dpkg,maven,pip/dist_info}.rs, binary/{linkage,go_binary}.rs).
  - Audit of ~5 files with hard-coded Unix paths (file_hashes.rs, binary/jdk_collapse.rs, docker_image.rs, binary/mod.rs, package_db/dpkg.rs).
  - New CI job `lint-and-test-windows` in `.github/workflows/ci.yml`.
  - New release job `build-windows-x86_64` in `.github/workflows/release.yml`.
  - Documentation refresh in README.md.
- Linux-specific readers (dpkg/rpm/apk) compile + silently no-op on Windows; they detect file-non-existence and return empty results. No code changes required to those readers themselves.
