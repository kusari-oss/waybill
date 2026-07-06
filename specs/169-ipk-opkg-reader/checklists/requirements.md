# Specification Quality Checklist: ipk/opkg package-database reader

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-06
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details in FRs (spec cites `mikebom sbom scan` invocation + purl-spec `pkg:opkg/` PURL type — external contracts, not implementation choices)
- [X] Focused on user value (Yocto/OpenWrt operators get non-zero SBOM from ipk builds)
- [X] Written for mikebom maintainer + Yocto/OpenWrt operator audience
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous — each FR names a concrete deliverable (walker allowlist, PURL type, license routing, evidence-kind value, distro qualifier)
- [X] Success criteria are measurable (component count thresholds, byte-identity guards, coverage percentages, PASS/FAIL SPDX validation)
- [X] Success criteria are technology-agnostic where possible (SC-001 refers to component count; SC-006 refers to formats by name)
- [X] All acceptance scenarios are defined — each user story has 2-4 Given/When/Then
- [X] Edge cases are identified — 7 edge cases including malformed archive, empty dir, `Provides` virtual pkgs, non-ASCII bytes, archive-size cap, License variations
- [X] Scope is clearly bounded — 8 explicit out-of-scope items covering eBPF trace, Yocto SPDX consumption, OpenWrt online feeds, signing, `PACKAGE_CLASSES=package_ipk_rpm` hybrid, ipk creation, `.ipk`-in-image mode, multi-arch resolution
- [X] Dependencies and assumptions identified — no new Cargo deps, purl-spec type stability, control-file dialect compatibility with deb reader, m090 fixture layout notes

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — verified via 8 unit tests + 1 integration test + testbed reproduction
- [X] User scenarios cover primary flows — US1 P1 basic ipk-dir scan (MVP), US2 P2 filename fallback robustness, US3 P3 binary-walker dedup, US4 P4 distro-qualifier propagation
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification — FR-002 cites ar-archive parsing but frames as "reader MUST parse", not "reader MUST use crate `ar` version X"

## Notes

- **Empirically grounded**: root cause + reproduction anchored to issue #500's `shape_skipped=4584` on a real Yocto scarthgap build. Not speculative.
- **Small delta scope**: ipk format is a near-subset of `.deb`. Reuses `ar`/`flate2`/`tar` already in the workspace. Zero new Cargo dependencies per FR-010's constraint.
- **Follows m135 alpm-reader spec template**: same shape as the prior OS-package reader milestone. 4 user stories (MVP + 3 refinements), 12 FRs, 12 SCs.
- **License-work inheritance**: reader routes control-file License fields through the existing m152/153/154 SPDX pipeline. No separate license-code path.
- **Fixture strategy noted in Assumptions**: prefer vendored real-world ipks (~10-100 KB each) over synthetic; add to `mikebom-cli/tests/fixtures/ipk-files/` per m069 rpm-files precedent.
- **Related-work section links 7 prior rpm-side hardening fixes** (#468-487) whose license-expression + evidence-kind + PURL infrastructure the ipk reader inherits.
