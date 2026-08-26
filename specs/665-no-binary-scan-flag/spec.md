# Feature Specification: `--no-binary-scan` flag to skip Go binary content probing

**Feature Branch**: `665-no-binary-scan-flag`
**Created**: 2026-08-23
**Status**: Draft
**Input**: m664 follow-up backlog item #1 (see `specs/664-single-pass-walker/perf-comparison.md §Follow-up backlog`). Add an opt-in CLI flag that gates the `go_binary` content-probe pipeline, letting operators trade Go-binary module attribution for scan wall-time. Identified as the largest single perf lever available after m664's walker consolidation: on the mongo fixture (55,190 files), waybill's warm-cache wall-time drops from 3.04s to a projected ~640ms — putting it on par with trivy (1.12s) and syft (1.72s).

## Clarifications

### Session 2026-08-23

- Q: Flag scope — one flag for all binary readers, or one per reader? → A: **Parameterized flag `--no-binary-scan=<MODE>`** — v1 ships with mode `go` (skips only go_binary, matching the perf lever identified in m664). Future modes (`all`, `elf`, `symbols`, etc.) can be added without new flag names. Enum-value CLI shape allows growth without CLI-surface churn.
- Q: Does the flag apply to `waybill trace ...` mode too, or only `waybill sbom scan`? → A: **`sbom scan` only.** Trace-mode's `go_binary` cost is negligible vs the source-tree scan case; adding trace-mode support doubles integration-test surface for marginal value. If demand emerges, add as a follow-up.
- Q: SC-005 Go-binary fixture — checked in, built at test time, or synthesized? → A: **Checked-in pre-built binary, sibling repo (matches m090 fixture-stay-set convention).** The Go binary lives in the existing `kusari-oss/waybill-test-fixtures` sibling repo (not in-tree per Q3 preference), fetched via the existing m090 `fixture_path()` helper with pinned SHA. Zero Go-toolchain dependency in waybill's CI; keeps the main repo lean.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Opt out of Go-binary probing for a fast scan (Priority: P1)

An operator scanning a large monorepo (kernel-of-file-count, e.g., mongo/pytorch) doesn't care about statically-linked Go binary identification for their use case. They pass `--no-binary-scan` on the command line and see a materially faster scan. The rest of the SBOM (source-tree readers, OS-package readers, dep-graph resolution, cross-tier reconciliation) is unaffected.

**Why this priority**: This is the reason the flag exists. Without it, operators who don't need Go-binary attribution are stuck paying its cost on every scan.

**Independent Test**: Run `waybill sbom scan --offline --file-inventory=off --no-binary-scan --path <mongo-checkout>` twice (first primes warm cache, second measures) — wall-time is materially lower than the equivalent run without the flag, and the emitted SBOM contains zero components attributable to Go binary probing.

**Acceptance Scenarios**:

1. **Given** a mongo checkout (~55k files) and warm OS cache, **When** the operator runs `waybill sbom scan --offline --file-inventory=off --no-binary-scan --path <checkout>`, **Then** the scan completes at least 60% faster than the same scan without the flag on the same environment.
2. **Given** any checkout containing a statically-linked Go binary with embedded BuildInfo, **When** the operator runs waybill with `--no-binary-scan`, **Then** that binary appears in the SBOM only via other readers (e.g., OS-package databases if the binary is dpkg-owned) — waybill does NOT emit a `pkg:golang/<module>` component derived from BuildInfo probing.
3. **Given** any checkout, **When** the operator runs waybill without `--no-binary-scan`, **Then** post-feature behavior is byte-identical to pre-feature behavior (backwards compatibility is preserved).

---

### User Story 2 - Discoverability: operator sees the flag as a first-class knob (Priority: P2)

An operator new to waybill runs `waybill sbom scan --help` and can find `--no-binary-scan` in the flag list with a one-line description that names the trade-off (fast scan vs. Go-binary attribution). They don't need to read `perf-comparison.md` to know the flag exists.

**Why this priority**: Perf-tuning flags are useless if operators don't know they exist. This is a documentation surface rather than a functional gap.

**Independent Test**: `waybill sbom scan --help` output includes the `--no-binary-scan` flag with a description that names both the perf benefit and the coverage trade-off.

**Acceptance Scenarios**:

1. **Given** any waybill installation, **When** the operator runs `waybill sbom scan --help`, **Then** the output lists `--no-binary-scan` alongside its description.
2. **Given** the docs site (`docs/user-guide/`), **When** the operator searches for "binary scan" or "performance", **Then** documentation references the flag with a link to its perf impact.

---

### User Story 3 - Diagnostic transparency in the emitted SBOM (Priority: P3)

An operator receiving an SBOM produced by waybill wants to know whether Go-binary probing ran or was suppressed via `--no-binary-scan`. If suppressed, they know NOT to trust any absence of `pkg:golang/*` components as evidence that no Go binaries exist in the scanned tree.

**Why this priority**: Downstream consumers need this signal to interpret SBOM completeness correctly. Without it, a `--no-binary-scan` SBOM looks indistinguishable from a full scan of a Go-binary-free tree — a silent completeness gap.

**Independent Test**: An SBOM produced with `--no-binary-scan` carries a document-scope annotation naming the suppression; an SBOM produced without the flag does NOT carry that annotation.

**Acceptance Scenarios**:

1. **Given** an SBOM produced with `--no-binary-scan`, **When** an operator inspects the document-scope metadata, **Then** an annotation names the flag (e.g., `waybill:go-binary-scan-suppressed: true`) so consumers know the scan intentionally omitted Go-binary probing.
2. **Given** an SBOM produced without `--no-binary-scan`, **When** an operator inspects the document-scope metadata, **Then** the suppression annotation is absent (byte-identity vs. pre-feature output).

---

### Edge Cases

- **What happens when a Go binary is claimed by an OS-package reader?** With `--no-binary-scan`, waybill emits ONLY the OS-package-derived component (dpkg / apk / rpm). Without the flag, both signals merge via the m191 reconciler. The suppression does not create dangling references — components claimed elsewhere are still emitted.
- **What happens with `--include-vendored`?** The flag is orthogonal — `--include-vendored` affects CMake and other source-tree readers, not go_binary. Both flags can coexist.
- **What happens with cross-tier reconciliation (m191)?** The reconciler operates over the emitted component set. With `--no-binary-scan`, fewer components enter the reconciler; downstream shape is unchanged.
- **What happens when the flag is set but the tree has zero binaries anyway?** Wall-time savings are proportional to file count (stat cost). Even a tree with zero Go binaries pays the ~55k stat cost per m664's post-pilot phase; the flag skips those stats entirely.
- **What happens with the FR-009 diagnostic log?** The shared walker's `per_reader_dispatch_counts` for `go_binary` shows zero when the flag is set (registration is skipped, not just the callback).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CLI MUST accept a `--no-binary-scan=<MODE>` parameterized flag on the `sbom scan` subcommand. Recognized modes for v1: `go` (skips the `go_binary` reader). The mode is REQUIRED — a bare `--no-binary-scan` without a value MUST error with an operator-visible message naming the recognized modes. Future modes (e.g., `all`, `elf`, `symbols`) MAY be added without renaming the flag.
- **FR-002**: When `--no-binary-scan=go` is set, the system MUST NOT register the `go_binary` reader in the shared-walker pilot. The reader's `on_file` callback, its post-pilot `finalize()` probe, and its emissions MUST all be skipped.
- **FR-003**: When `--no-binary-scan` is absent (default), post-feature behavior MUST be byte-identical to pre-feature behavior on the same input. No golden SBOM (existing or newly authored) MUST change output on the default path.
- **FR-004**: When `--no-binary-scan=<MODE>` is set, the emitted SBOM MUST carry a document-scope annotation (`waybill:binary-scan-suppressed`, value = the MODE string, e.g. `"go"`) so consumers can distinguish an intentional suppression from a genuinely-binary-free scan AND know which subset of binary content probing was skipped.
- **FR-005**: When `--no-binary-scan=<MODE>` is set, the FR-009 diagnostic log line from milestone 664 MUST show the corresponding gated reader(s) with a dispatch count of zero (for `go` mode: `go_binary` shows zero; other binary-adjacent readers unchanged).
- **FR-006**: The flag MUST be documented in `waybill sbom scan --help` output with a one-line description naming the perf benefit, the coverage trade-off, and the currently-recognized mode values.
- **FR-007**: In v1 (`--no-binary-scan=go`), the flag MUST NOT affect other binary-adjacent readers (m096 ELF section reader for `.dep-v0`, m099 symbol fingerprinting, m104 binary-role classification). Only the m216-vintage `go_binary::finalize` BuildInfo-probing pipeline is gated.
- **FR-008**: The flag SHOULD have an environment variable equivalent (`WAYBILL_NO_BINARY_SCAN=<MODE>`, same accepted-mode set as the CLI flag) so CI systems can set it without editing every scan invocation. Mirrors `WAYBILL_INCLUDE_VENDORED` / `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE` precedent.
- **FR-009**: An unrecognized mode value (e.g., `--no-binary-scan=xyz`) MUST fail fast with an operator-visible error listing the recognized modes. The scan MUST NOT proceed with the flag silently ignored.

### Key Entities

- **Binary-scan mode**: an enumerated value naming a subset of binary-adjacent readers to skip. v1 recognizes `go`. Future modes may extend the enum. Passed via `--no-binary-scan=<MODE>` or `WAYBILL_NO_BINARY_SCAN=<MODE>`.
- **Registration gate**: the decision-point where each mode-affected reader's `registration()` is either invoked (mode not set / mode doesn't gate this reader) or skipped (mode gates it) inside `run_shared_walker_pilot`. For v1 `go` mode: a single yes/no branch on the flag state for `go_binary::registration()`.
- **Suppression annotation**: a document-scope metadata entry `waybill:binary-scan-suppressed` whose value is the MODE string. Present iff the flag was set at scan time. Consumers use it to reason about SBOM completeness AND to know which subset was skipped.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the mongo fixture (55,190 files, reference macOS APFS warm-cache release build), a scan with `--no-binary-scan` completes at wall-clock ≤ 700 ms, down from 3.04s without the flag (≥ 4× improvement).
- **SC-002**: On the pytorch fixture (21,651 files), a scan with `--no-binary-scan` completes at wall-clock ≤ 400 ms, down from 1.12s (≥ 2.5× improvement).
- **SC-003**: On the ansible fixture (5,793 files, mostly non-binary Python source), a scan with `--no-binary-scan` completes in ≤ 300 ms (down from 777 ms). Ansible sees a smaller absolute win because its binary count is small; the flag's value on this fixture is primarily consistency-of-flag-surface rather than raw wall-time.
- **SC-004**: A scan WITHOUT `--no-binary-scan` produces byte-identical output to the pre-feature baseline (workspace golden suite `cargo +stable test --workspace` passes 5,183 / 0 — the exact count from m664 merge).
- **SC-005**: A scan WITH `--no-binary-scan=go` on a tree containing a statically-linked Go binary with embedded BuildInfo emits ZERO `pkg:golang/*` components derived from binary probing. Verified via a checked-in pre-built Go binary hosted in the `kusari-oss/waybill-test-fixtures` sibling repo (per m090 fixture-stay-set convention), accessed at test time via the existing `fixture_path()` helper with pinned SHA.
- **SC-006**: The suppression annotation (`waybill:binary-scan-suppressed=go`) is present in the SBOM iff the flag was set — verified via a two-fixture parity test (same input, one run with `--no-binary-scan=go`, one without; annotation diff isolates one line).
- **SC-007**: An unrecognized mode (`--no-binary-scan=xyz`) MUST fail with a non-zero exit code and an error message naming the recognized modes. Verified via a CLI integration test.

## Assumptions

- Default behavior is unchanged (backwards compatibility). The flag is opt-in; existing users see no output difference until they explicitly pass `--no-binary-scan`.
- v1 recognizes only mode `go`. Adding modes (e.g., `all`, `elf`, `symbols`) is a follow-up feature with its own perf validation. The parameterized flag shape means future modes don't require CLI renames.
- Within a mode (e.g., `go`), the suppression is all-or-nothing for the affected reader(s). A finer-grained knob (e.g., "probe only ELF, skip Mach-O") is out of scope — those splits would each land as new modes.
- The flag targets the m216-vintage `go_binary::finalize` BuildInfo probe pipeline specifically. Other binary-adjacent readers (m096 ELF section reader, m099 symbol fingerprinting, m104 binary-role classification) are NOT gated by this flag — they run on the OS-package-claimed subset of binaries and don't contribute to the mongo perf outlier documented in `perf-comparison.md §Mongo residual analysis`.
- The suppression annotation lands on the SBOM's document-scope metadata (i.e., under `metadata.properties` in CycloneDX / `creationInfo.annotations` or equivalent in SPDX). Downstream consumers already inspect this surface for other waybill annotations (`waybill:workspace-member`, `waybill:go-toolchain-detected`, etc.).
- Environment variable equivalent (`WAYBILL_NO_BINARY_SCAN=1`) is nice-to-have but not gating on P1. If prioritization pressure emerges, drop the env var to a follow-up.
- Existing waybill users have not built downstream tooling that DEPENDS on the presence of `pkg:golang/*` from binary probing — if such tooling exists, this flag is a no-op for those users (they simply won't set it).

## Out of Scope

- Splitting `go_binary` into finer-grained readers (per-format, per-arch, etc.).
- Gating other readers behind similar flags (e.g., `--no-cmake-scan`, `--no-yocto-scan`). Those can be follow-up features; each requires its own perf validation.
- Enabling `--no-binary-scan` as the default. That's a behavior change that would require its own m664-style byte-identity investigation across every waybill-consuming tool.
- Retroactively backfilling the flag onto pre-m664 waybill releases. The flag only makes sense in the post-m664 architecture where `go_binary` is a single registered reader that can be cleanly skipped at the pilot registration step.
