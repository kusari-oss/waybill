# Implementation Plan: Skip GOROOT stdlib as Go main-module

**Branch**: `217-goroot-stdlib-skip` | **Date**: 2026-07-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/217-goroot-stdlib-skip/spec.md`

## Summary

Add a single filter to the Go rootfs walker (`candidate_project_roots` at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2487`): read each candidate `go.mod`, parse its `module` line via the existing `parse_go_mod` helper, and skip the directory if the declared module is exactly `"std"` or `"cmd"`. Kills the waybill#631 bug at its source — no `go list` preflight runs against GOROOT/src, no stderr flood, no false-positive `pkg:golang/std@...` main-module in the SBOM.

Optional (P2 story): when the walker skips a toolchain-internal go.mod, record the parent directory path (typically `$GOROOT`) for a document-level `waybill:go-toolchain-detected` annotation. Aggregation follows the C121 `waybill:workspaces-detected` shape (JSON-encoded sorted-deduplicated path array).

**Zero new Cargo dependencies. Zero new files in the production tree.** One production-file edit (`legacy.rs` for the walker filter + accumulator) + three touch-ups for the P2 annotation (CDX metadata builder, SPDX 2.3 doc builder, SPDX 3 doc builder) + parity-catalog entry + one new integration test file + one new fixture directory + one doc row.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–216; no nightly required).

**Primary Dependencies**: Existing only. **Zero new Cargo dependencies.**
- `std::fs` for reading `go.mod` at walker time.
- `parse_go_mod` at `legacy.rs:188` (existing) — extracts `module_path: Option<String>` from `go.mod` text. Reused verbatim.
- `tracing` for a `debug!` log line when a toolchain-internal go.mod is skipped (INFO would spam on Go-heavy repos; debug is the right level for "boring negative signal" — matches m053's precedent).
- The P2 transparency annotation reuses the existing document-scope annotation plumbing (same code path that emits `waybill:workspaces-detected` at m176) — no new emit infrastructure.

**Storage**: N/A — all state in-process for the lifetime of a scan.

**Testing**:
- Unit tests in `legacy.rs::tests` covering (a) walker skips `module std`, (b) walker skips `module cmd`, (c) walker keeps `module example.com/user-app` (backwards-compat), (d) walker's toolchain-observation accumulator collects paths correctly across multiple detected toolchains, (e) walker skip decision is independent of install path (test both `/usr/local/go` and `/opt/go` layouts under a tempdir).
- Integration test in `waybill-cli/tests/goroot_skip.rs` (new file) exercising the fix end-to-end against a minimal GOROOT-shape fixture.
- Regression fixture: `waybill-cli/tests/fixtures/goroot_stub/` — mimics `$GOROOT/src/go.mod` (declares `module std`) + `$GOROOT/src/cmd/go.mod` (declares `module cmd`) + a user-project sibling to prove FR-004 non-regression.

**Target Platform**: All existing platforms (linux-x86_64 default + ebpf-tracing, macOS, Windows). No platform-specific behavior — pure string comparison on parsed `go.mod` text.

**Project Type**: Same three-crate architecture (Constitution Principle VI) untouched — this feature edits ONE existing file (`waybill-cli/src/scan_fs/package_db/golang/legacy.rs`) plus SBOM emitter touch-ups + parity infra + one integration test + one fixture. `waybill-common`: no changes. `waybill-ebpf`: no changes.

**Performance Goals**:
- **Zero cost on non-Go scans**: the walker's `should_skip_descent` / `is_dir && join("go.mod").is_file()` predicate at `legacy.rs:2500` is unchanged; the new filter fires only INSIDE the existing `if path.join("go.mod").is_file()` branch, so only when a `go.mod` is actually observed.
- **Cost on Go scans**: one extra `std::fs::read_to_string` per candidate `go.mod` (previously read at emission time; the walker now reads it once up-front so the decision fires before downstream processing). Net: same total file-reads (moved earlier); one extra string comparison per go.mod.
- **Cost on Go-toolchain-carrying scans**: strongly negative — skips the entire downstream `go list all` preflight (which was hanging on stderr accumulation for 5+ seconds on the reproducer per the CI log timestamps in issue #631).

**Constraints**:
- **FR-004 backwards-compat**: user projects with any non-toolchain module path continue to emit main-modules byte-identically. Enforced via unit test (c) + SC-004 pre-feature regression tests.
- **FR-005 install-path independence**: detection is on the go.mod's `module` line ONLY. No hardcoded install-path list. Enforced via unit test (e) with both `/usr/local/go` and `/opt/go` synthetic layouts under a tempdir.
- **Constitution Principle I**: no subprocess calls to `go` or any external tool. Static parse only via existing `parse_go_mod` (already Pure Rust).
- **Constitution Principle V**: new `waybill:go-toolchain-detected` annotation is documented in `docs/reference/sbom-format-mapping.md` with a parity-bridging justification (no format-native "Go toolchain observation at document scope" field exists — same audit outcome as C121 `waybill:workspaces-detected` from m176).
- **Constitution Principle X**: the P2 annotation is exactly the mandated pattern — surfacing an observation waybill made that affects the emitted SBOM's shape, so downstream consumers know why the SBOM doesn't contain a Go main-module even though the scanned rootfs demonstrably had Go files.
- **Zero new Cargo dependencies** enforced by no `Cargo.toml` edits.

**Scale/Scope**:
- Modified: `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` (~25 LOC: filter block + accumulator threading + debug log). Threading the accumulator up to `read()` and out to `ScanArtifacts` adds ~15 LOC across the golang reader.
- Modified: CDX + SPDX-2.3 + SPDX-3 document metadata builders (~10 LOC total) to route the new annotation to each format's document-scope landing slot.
- New tests: 5 unit tests in `legacy.rs::tests` (~80 LOC) + 3 integration tests in `waybill-cli/tests/goroot_skip.rs` (~120 LOC).
- New fixture: `waybill-cli/tests/fixtures/goroot_stub/` (~6 files, total <100 lines).
- New doc row: `docs/reference/sbom-format-mapping.md` (~1 catalog row entry).
- New parity extractors: `c136_cdx` / `c136_spdx23` / `c136_spdx3` in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` + 1 row in `mod.rs::EXTRACTORS` + use-list additions (~5 LOC total).
- **Total estimated diff**: ~50 LOC production + ~200 LOC test + 1 fixture dir + 1 doc row + 3-4 parity-extractor rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No C. No subprocess calls to `go`. Pure static-parse filter using the existing `parse_go_mod` helper.
- **II. eBPF-Only Observation**: ✅ N/A. Filesystem-driven package-DB reader (enrichment layer per Principle XII, not discovery).
- **III. Fail Closed**: ✅ The filter is subtractive: when it fires, the walker skips a directory that would previously have caused a downstream failure. When it doesn't fire (any non-toolchain go.mod), pre-feature behavior is preserved verbatim.
- **IV. Type-Driven Correctness**: ✅ Reuses `parse_go_mod`'s existing typed output (`GoModDocument.module_path: Option<String>`). No new `.unwrap()` in production code.
- **V. Specification Compliance**: ✅
  - The P2 `waybill:go-toolchain-detected` annotation is parity-bridging (no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native construct for "a Go toolchain was observed in the scanned rootfs at path X"). Documented per Principle V.
  - The filter itself changes nothing about emitted SBOMs' spec-conformance — it only prevents an incorrect `pkg:golang/std@*` component from being emitted (which was itself dubious as an SBOM identity — `std` is a toolchain-internal boundary, not a distributable module identifier).
- **VI. Three-Crate Architecture**: ✅ Only `waybill-cli` changes. `waybill-common` + `waybill-ebpf` untouched.
- **VII. Test Isolation**: ✅ Pure-Rust unit + integration tests. No eBPF required. Cross-platform.
- **VIII. Completeness**: ✅ Positive impact — the filter removes a false-positive main-module (`pkg:golang/std` was a spurious component, not a real dependency). SBOM completeness of the user's actual Go project is unchanged.
- **IX. Accuracy**: ✅ Same positive-impact story: removing the `pkg:golang/std` false positive strictly improves signal-to-noise.
- **X. Transparency**: ✅ The P2 annotation is exactly the Principle-X pattern — surfacing an observation (waybill saw a Go toolchain at `<path>` and correctly skipped it) so downstream consumers can distinguish "waybill missed the toolchain" from "waybill saw the toolchain and made a design choice not to emit it as a main-module".
- **XI. Enrichment**: ✅ N/A.
- **XII. External Data Source Enrichment**: ✅ N/A. No new external calls.

**Strict Boundaries**:
1. No lockfile-based discovery — N/A (this reduces spurious discovery, doesn't add new sources).
2. No MITM proxy — N/A.
3. No C code — enforced.
4. No `.unwrap()` in production — enforced.
5. No file-tier duplicates in default mode — N/A (this feature doesn't touch the file-tier walker).

**Verdict**: ✅ Constitution check passes. Zero unjustified violations. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/217-goroot-stdlib-skip/
├── plan.md                    # This file (/speckit.plan output)
├── research.md                # Phase 0 — filter placement, module-path signal justification, annotation shape, fixture strategy
├── data-model.md              # Phase 1 — no new record types; the P2 accumulator is a Vec<PathBuf> local to the walker function, threaded to ScanArtifacts
├── quickstart.md              # Phase 1 — operator recipe: scan a Go-toolchain-carrying image; confirm the fix
├── contracts/
│   └── goroot-skip.md         # C1: filter predicate + annotation emission + backwards-compat guarantees
├── checklists/
│   └── requirements.md        # (already exists from /speckit.specify — all PASS)
└── tasks.md                   # Phase 2 output — NOT created here
```

### Source Code (repository root)

```text
waybill-cli/
└── src/
    ├── scan_fs/
    │   └── package_db/
    │       └── golang/
    │           └── legacy.rs   # +module-path filter in candidate_project_roots (~25 LOC)
    │                           # +Vec<PathBuf> toolchain-observation accumulator threaded
    │                           #  through read() → out to ScanArtifacts
    │                           # +unit tests in the existing `tests` module
    ├── generate/
    │   ├── mod.rs              # +new field on ScanArtifacts: go_toolchains_detected:
    │   │                       #  Option<&'a [PathBuf]> — matches the m173 go_cache_warming
    │   │                       #  pattern (borrowed slice, default None on non-Go scans)
    │   ├── cyclonedx/
    │   │   └── metadata.rs     # +propagation block for waybill:go-toolchain-detected
    │   │                       #  at document scope (mirrors m176 workspaces-detected)
    │   └── spdx/               # analogous propagation in the 2.3 + 3 document builders
    └── parity/
        └── extractors/
            ├── cdx.rs          # +c136_cdx (document-scope extractor)
            ├── spdx2.rs        # +c136_spdx23
            ├── spdx3.rs        # +c136_spdx3
            └── mod.rs          # +EXTRACTORS row + use-list additions

waybill-cli/tests/
├── goroot_skip.rs              # NEW — 3 integration test scenarios
├── fixtures/
│   └── goroot_stub/            # NEW — mini-GOROOT layout: src/go.mod (module std) +
│                               #  src/cmd/go.mod (module cmd) + a companion
│                               #  user-project sibling for FR-004 non-regression
└── (rest unchanged)

docs/reference/
└── sbom-format-mapping.md      # +C136 row for waybill:go-toolchain-detected
                                #  (parity-bridging annotation per Constitution V)

waybill-common/                 # UNTOUCHED
waybill-ebpf/                   # UNTOUCHED
xtask/                          # UNTOUCHED
```

**Structure Decision**: single-file production change (`legacy.rs`) for the walker filter + accumulator; three touch-ups to the SBOM emitters (metadata builders) + one new field on `ScanArtifacts` (mirrors the m173 `go_cache_warming` pattern) for the P2 annotation. One new integration test file + one new fixture directory. Zero new modules. Zero new crates.

## Complexity Tracking

> No Constitution violations. Complexity tracking section unused.
