# Contract — milestone 098 compiler/linker version extraction

Eight behavioral contracts. Each specifies (a) the invariant the parser / emitter holds, (b) a verification recipe — a unit test name or a `jq`-style grep on emitted SBOM JSON.

## Contract 1 — ELF `.comment` single-stamp extraction (FR-001 / SC-001)

**Path**: `mikebom-cli/src/scan_fs/binary/elf.rs::parse_comment_section`.

**Invariant**: when an ELF binary's `.comment` section contains exactly one NUL-terminated string (typical for default `gcc` / `clang` builds), the emitted file-level component carries property `mikebom:elf-compiler-stamps` with value being a JSON array of one element: the verbatim string (NUL stripped).

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::elf::tests::parse_comment_section_single_stamp 2>&1 | grep "test result:"
# Expected: ok. 1 passed.
```

## Contract 2 — ELF `.comment` multi-toolchain dedup (FR-001 / Edge Case)

**Path**: same as Contract 1.

**Invariant**: when `.comment` contains N entries with M duplicates (typical of `ld` concatenating from static libraries), the emitted property is a JSON array of (N - M) unique entries in first-occurrence order.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::elf::tests::parse_comment_section_multi_toolchain_dedup 2>&1 | grep "test result:"
# Expected: ok. 1 passed.
```

## Contract 3 — ELF `.comment` size caps (FR-001)

**Path**: same as Contract 1.

**Invariant**: per-entry 4 KiB cap (entries exceeding this are truncated with `" ... (truncated)"` suffix); total 64 KiB cap (subsequent entries replaced by a single `"... (truncated)"` marker).

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::elf::tests::parse_comment_section_oversize_truncation \
    scan_fs::binary::elf::tests::parse_comment_section_total_cap 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 4 — Mach-O `LC_BUILD_VERSION` full parse (FR-002 / SC-002)

**Path**: `mikebom-cli/src/scan_fs/binary/macho.rs::parse_build_version_full`.

**Invariant**: when a Mach-O binary has an `LC_BUILD_VERSION` load command, the emitted file-level component carries:
- `mikebom:macho-build-version` = JSON object `{"platform": <string>, "min_os": <string>, "sdk": <string>}`
- `mikebom:macho-build-tools` = JSON array `[{"tool": <string>, "version": <string>}, ...]` in declaration order (only when `ntools > 0`)

Unknown platform IDs pass through as `"unknown-<id>"`; unknown tool IDs as `"unknown-<id>"` (per Edge Case "unknown platform value").

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::macho::tests::parse_build_version_full_happy_path \
    scan_fs::binary::macho::tests::parse_build_version_full_unknown_platform 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 5 — Mach-O malformed-tools-array defensive parse (FR-008 / Edge Case)

**Path**: same as Contract 4.

**Invariant**: when `LC_BUILD_VERSION.ntools` claims more records than fit in the load-command's `cmdsize`, the parser stops at the first record that runs off the end, returns a partial `MachoBuildVersion` with the successfully-parsed tools, and emits a `tracing::warn!`. No panic, no `Err`-bubble-up.

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::macho::tests::parse_build_version_full_malformed_ntools 2>&1 | grep "test result:"
# Expected: ok. 1 passed.
```

## Contract 6 — PE linker-version always-emit (FR-003 / SC-003)

**Path**: `mikebom-cli/src/scan_fs/binary/pe.rs::parse_linker_version` (or extended `parse_pe_identity`).

**Invariant**: every PE binary that parses successfully carries property `mikebom:pe-linker-version` with value matching `^[0-9]+\.[0-9]+$`. The value `"0.0"` is valid (PE binaries with zeroed linker-version bytes, typically packed or obfuscated).

**Verification**:
```bash
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::pe::tests::parse_pe_identity_emits_linker_version \
    scan_fs::binary::pe::tests::parse_pe_identity_zeroed_linker_version 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 7 — Property omission when source absent (FR-007)

**Path**: `mikebom-cli/src/scan_fs/binary/entry.rs::build_elf_identity_annotations` + `build_macho_identity_annotations` + `build_pe_identity_annotations`.

**Invariant**: when the underlying ELF `.comment` section / Mach-O `LC_BUILD_VERSION` command is absent, the corresponding `mikebom:elf-compiler-stamps` / `mikebom:macho-build-version` / `mikebom:macho-build-tools` properties are NOT emitted (no empty array, no null, no placeholder). The exception is `mikebom:pe-linker-version` which is always-emit per Contract 6.

**Verification**:
```bash
# Build a small ELF binary without `.comment` (via `cc -fno-ident`) and scan it.
# Or use the unit-test path:
cargo +stable test -p mikebom --bin mikebom \
    scan_fs::binary::elf::tests::parse_comment_section_empty_section \
    scan_fs::binary::macho::tests::parse_build_version_full_missing_command 2>&1 | grep "test result:"
# Expected: ok. 2 passed.
```

## Contract 8 — Parity-catalog rows registered (FR-010 / Constitution V)

**Path**: `mikebom-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`.

**Invariant**: the `EXTRACTORS` table contains 4 new rows (C16-C19 or next-available) for `mikebom:elf-compiler-stamps`, `mikebom:macho-build-version`, `mikebom:macho-build-tools`, `mikebom:pe-linker-version`. Each row registers symmetric-equal directionality. C16 and C18 (array values) are `order_sensitive: true`; C17 and C19 (scalar values) are `order_sensitive: false`. Per-format extractor stubs exist in `cdx.rs` / `spdx2.rs` / `spdx3.rs` mirroring the C10/C11/C15 macro-invocation pattern.

**Verification**:
```bash
# 4 row registrations in mod.rs:
grep -nE 'C1[6789].*mikebom:(elf-compiler-stamps|macho-build-version|macho-build-tools|pe-linker-version)' \
    mikebom-cli/src/parity/extractors/mod.rs | wc -l
# Expected: 4

# 4 stubs per format file:
grep -nE 'c1[6789]_(cdx|spdx23|spdx3)' mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs | wc -l
# Expected: 12 (4 stubs × 3 files)

# 4 doc rows:
grep -cE '^\| C1[6789] \|' docs/reference/sbom-format-mapping.md
# Expected: 4

# Parity coverage test passes:
cargo +stable test -p mikebom --test sbom_format_mapping_coverage 2>&1 | grep "test result:"
# Expected: ok. N passed; 0 failed.
```

## Contract 9 — Diff scope guardrails (FR-005, FR-006, FR-009, SC-007, SC-008)

**Verification**:
```bash
# No new Cargo deps (FR-005 / SC-008):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
# Expected: 0

# Production code outside scan_fs/binary/ + parity/extractors/:
git diff --name-only main | grep -E '^mikebom-cli/src/' \
  | grep -vE '^mikebom-cli/src/scan_fs/binary/' \
  | grep -vE '^mikebom-cli/src/parity/extractors/' \
  | wc -l
# Expected: 0

# Golden regen scope (FR-009 / SC-007):
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty OR ≤ a handful of files changed (each with ≤1-component delta)

# Diff scope allowlist:
git diff --name-only main | sort
# Expected only:
#   docs/reference/sbom-format-mapping.md
#   mikebom-cli/src/parity/extractors/{cdx,mod,spdx2,spdx3}.rs
#   mikebom-cli/src/scan_fs/binary/{elf,entry,macho,pe,scan}.rs
#   mikebom-cli/tests/binary_build_provenance.rs   (NEW, optional)
#   specs/098-compiler-version-extract/...
#   CLAUDE.md  (auto-updated by /speckit-plan)
```

## Contract 10 — Pre-PR gate clean (SC-005)

**Verification**:
```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: prints `>>> all pre-PR checks passed.`; exit 0.
# Clippy: zero warnings.
# Test suite: every target `0 failed`.
```

The SPDX 3 validator (`spdx3-validate==0.0.5`) accepts the new annotations on Package elements; same emission path as every other `mikebom:*` annotation since milestone 004.
