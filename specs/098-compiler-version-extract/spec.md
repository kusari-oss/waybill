# Feature Specification: Compiler/linker version extraction for build provenance

**Feature Branch**: `098-compiler-version-extract`
**Created**: 2026-05-12
**Status**: Draft
**Input**: User description: "milestone 098 — extract compiler and linker version stamps from binaries: ELF .comment section, Mach-O LC_BUILD_VERSION, PE IMAGE_OPTIONAL_HEADER MajorLinkerVersion/MinorLinkerVersion. Build-tier provenance signal for unknown binaries: who built this? Survives stripping. Zero new Cargo deps."

## Background

Milestones 096 (binary-id enrichment) and 097 (CPE candidate emission) answered the question "what's INSIDE this random binary?" by extracting embedded version strings, exported-symbol fingerprints, and PURL-to-CPE mappings. The complementary question — "who BUILT this binary?" — has gone unanswered. Build-tier provenance is a different axis of trust than identification-tier: an unknown binary that was built by `clang 17.0.6` on a CI runner is materially different from one built by `gcc 7.5.0` on a developer's laptop, even if both contain the same statically-linked OpenSSL version.

Three signal channels are already present in nearly every binary and survive stripping (where ELF symbol tables and DWARF debug info don't):

1. **ELF `.comment` section**: a NUL-separated list of compiler-emitted stamps. A typical entry is `GCC: (Debian 12.2.0-14) 12.2.0` or `Linaro GCC 7.5-2019.12-rc1`. Multi-stage builds (mixed C + C++, mixed gcc + clang) accumulate multiple stamps; the section holds them all in declaration order.
2. **Mach-O `LC_BUILD_VERSION` load command**: a structured record carrying the target platform (`macos`, `ios`, `tvos`, `watchos`, `xros`), minimum-OS version, SDK version, and tools list with each tool's version (typically clang + ld). Standardized; well-documented; parseable from the load-commands list mikebom already iterates.
3. **PE `IMAGE_OPTIONAL_HEADER.MajorLinkerVersion`/`MinorLinkerVersion`**: two bytes in the optional header indicating which linker version produced the binary. Microsoft Visual Studio's linker writes its own version here (e.g., `14.36` → MSVC 2022 17.6); MinGW's `ld` writes its own (e.g., `2.41`).

All three are observable in the binary file itself (no external lookups, no DB downloads). All three are already accessible through the existing `object` crate (`.comment` via `section_by_name_bytes`, `LC_BUILD_VERSION` via the same load-command iteration mikebom already does for `LC_UUID` / `LC_RPATH`, optional-header bytes via the existing `ImageOptionalHeader` accessor at `pe.rs`). Zero new Cargo dependencies required.

**Scope framing**: this milestone is narrow — emit the parsed stamps as standards-native CDX / SPDX 2.3 / SPDX 3 properties on the file-level binary component. No CPE candidate emission for compilers (compiler/linker CVE matching is rare and benefits less from a one-shot CPE candidate). No DWARF debug-info parsing (deferred to a separate milestone if signal emerges). No `cargo auditable` decoder changes (already shipped in milestone 029).

**What this is NOT**: this is not vulnerability matching against compilers — the stamps are an evidence channel, not a CVE-trigger. Operators with concerns about toolchain-CVEs (e.g., GCC's `CVE-2024-...`) can use the emitted property to cross-reference downstream; mikebom emits the signal, doesn't classify it. Constitution X transparency: surface the data, let consumers decide what to do with it.

Out of scope: DWARF debug-info `.debug_info` / `.debug_line` extraction, `.comment` parsing into per-toolchain structured records (we emit the verbatim string; downstream tooling can sub-parse if it wants), Mach-O `LC_VERSION_MIN_MACOSX` (older load command — `LC_BUILD_VERSION` superseded it for macOS 10.14+; the older command is deferred to a follow-up milestone if signal emerges beyond the existing `mikebom:macho-min-os` extraction).

## Clarifications

### Session 2026-05-12

- Q: ELF `.comment` property name — follow the established format-prefix convention (`mikebom:elf-*` like milestone-023's `elf-build-id` / `elf-runpath` / `elf-debuglink`), or keep the unprefixed name? → A: A — rename to `mikebom:elf-compiler-stamps`. Matches the established convention; signals "this is ELF-specific" up front; consistent with v1 scope (ELF `.comment` only). Future Mach-O / PE equivalents (if signal emerges) can use their own format prefix without ambiguity.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator inspects a stripped ELF binary and sees the toolchain that built it (Priority: P1)

An operator has a stripped third-party ELF binary. Today: mikebom emits the file-level component (PURL + binary-class + linkage + binary-stripped), but no signal about the build environment. With this milestone: the binary's `.comment` section is parsed, and the file-level component carries a `mikebom:elf-compiler-stamps` property whose value is the JSON-encoded list of NUL-delimited entries from `.comment` (e.g., `["GCC: (Debian 12.2.0-14) 12.2.0", "clang version 14.0.6"]`). Operators can now ask "what compiler built this?" without disassembling.

**Why this priority**: build-tier provenance is the natural next-step after milestone 096's identification-tier work. The `.comment` extraction is the lowest-cost / highest-coverage source — every gcc/clang/icc-produced ELF carries it by default, and `strip` doesn't remove it (the section is non-allocable and not in the symbol-table chain).

**Independent Test**: take any compiled ELF binary (e.g., `/bin/ls` on Linux); scan with mikebom; expect a `mikebom:elf-compiler-stamps` property on the file-level binary component containing the parsed `.comment` entries.

**Acceptance Scenarios**:

1. **Given** a stripped ELF binary whose `.comment` section contains `GCC: (Debian 12.2.0-14) 12.2.0\0`, **When** mikebom scans the file, **Then** the file-level binary component carries property `mikebom:elf-compiler-stamps` with value `["GCC: (Debian 12.2.0-14) 12.2.0"]`.
2. **Given** a multi-toolchain ELF binary whose `.comment` contains both a GCC stamp AND a clang stamp (NUL-separated), **When** mikebom scans it, **Then** the property value is a JSON array with both entries in declaration order, no double-counting on within-section dedup.
3. **Given** an ELF binary with no `.comment` section (rare — typically a manually-stripped binary using `objcopy --remove-section .comment`), **When** mikebom scans it, **Then** no `mikebom:elf-compiler-stamps` property is emitted. The absence is itself informative: "we don't know who built it".
4. **Given** an ELF binary whose `.comment` section contains entries with non-UTF-8 bytes (corruption or hand-crafted), **When** mikebom scans it, **Then** invalid entries are lossy-decoded (`\u{FFFD}` replacement) and emitted alongside the valid ones — no scan failure, no warning at INFO level.

---

### User Story 2 — Operator inspects a Mach-O binary and sees its target platform + tool list (Priority: P2)

An operator has a Mach-O binary. The `LC_BUILD_VERSION` load command carries: platform name (e.g., `macos`), minimum OS version (e.g., `14.0`), SDK version (e.g., `14.4`), and a list of tools each tagged with its version (typically `clang` + `ld`). With this milestone: mikebom parses `LC_BUILD_VERSION` and emits two properties on the file-level binary component — `mikebom:macho-build-version` (the structured platform + sdk) and `mikebom:macho-build-tools` (the JSON-encoded tool list).

**Why this priority**: P2 because the existing `LC_BUILD_VERSION` partial parse (milestone 024 — extracts `min_os` via `parse_min_os_version`) already touches the bytes for one field; extending the same parser to also capture the SDK version + tools list is a small follow-on. Less universally informative than `.comment` (Mach-O is macOS-only territory), but high-quality when present.

**Independent Test**: take any Mach-O binary built with a recent Xcode (e.g., `/bin/ls` on macOS 14); scan with mikebom; expect `mikebom:macho-build-version` and `mikebom:macho-build-tools` properties on the file-level component.

**Acceptance Scenarios**:

1. **Given** a Mach-O binary with `LC_BUILD_VERSION` declaring `platform = macos, minos = 14.0, sdk = 14.4`, **When** mikebom scans it, **Then** the file-level component carries property `mikebom:macho-build-version = {"platform": "macos", "min_os": "14.0", "sdk": "14.4"}`.
2. **Given** the same binary's `LC_BUILD_VERSION` lists tools `[{ tool: clang, version: 1500.0.40.1 }, { tool: ld, version: 1015.0.0 }]`, **When** mikebom scans it, **Then** the component carries property `mikebom:macho-build-tools` with the JSON-encoded tool list in declaration order.
3. **Given** a fat (universal) Mach-O binary where each slice has its own `LC_BUILD_VERSION`, **When** mikebom scans it, **Then** the property value reflects the FIRST slice's record (matches the existing milestone-024 `parse_min_os_version` first-slice convention).
4. **Given** a Mach-O binary without `LC_BUILD_VERSION` (legacy build, or `LC_VERSION_MIN_*` only), **When** mikebom scans it, **Then** no `mikebom:macho-build-version` / `mikebom:macho-build-tools` properties are emitted. The `min_os` property from milestone 024 still emits if `LC_VERSION_MIN_*` is present.

---

### User Story 3 — Operator inspects a PE binary and sees the linker version (Priority: P2)

An operator has a Windows PE binary. `IMAGE_OPTIONAL_HEADER.MajorLinkerVersion`/`MinorLinkerVersion` is two bytes (e.g., `14.36` → MSVC 2022 17.6, `2.41` → MinGW ld). With this milestone: mikebom emits a `mikebom:pe-linker-version` property whose value is the lowercase string `<major>.<minor>` (e.g., `"14.36"`).

**Why this priority**: P2 because the existing `pe.rs::parse_pe_identity` already touches the optional header for `Machine` + `Subsystem`; reading two more bytes for linker-version is trivial. Less rich than `.comment` (PE has no analog of `.comment`'s string list), but the linker-version is the only build-tier signal PE binaries reliably retain.

**Independent Test**: take any Windows PE binary (e.g., `notepad.exe` on Windows); scan with mikebom; expect `mikebom:pe-linker-version` property on the file-level component.

**Acceptance Scenarios**:

1. **Given** a PE binary with `MajorLinkerVersion = 14, MinorLinkerVersion = 36`, **When** mikebom scans it, **Then** the file-level component carries property `mikebom:pe-linker-version = "14.36"`.
2. **Given** a PE binary with `MajorLinkerVersion = 0, MinorLinkerVersion = 0` (unusual but valid — some packers zero this out), **When** mikebom scans it, **Then** the property is still emitted with value `"0.0"`. The zero value is informative: "linker version was redacted or unknown" — operators correlate with `mikebom:binary-packed` to interpret.
3. **Given** a non-PE binary (ELF or Mach-O), **When** mikebom scans it, **Then** no `mikebom:pe-linker-version` property is emitted. Format-appropriate extraction only.

---

### Edge Cases

- **`.comment` section containing only NUL padding** (some binaries declare the section but leave it empty): no property emission. The absence is informative.
- **`.comment` with extremely long entries** (some build systems concatenate compile flags into the stamp — e.g., `clang version 14.0.6 (https://github.com/llvm-mirror/clang.git ...)`): emit verbatim. Cap individual entry length at 4 KiB to bound the property's worst-case size; truncation marker `"... (truncated)"` appended if cap hit.
- **`.comment` with hundreds of duplicate entries** (some linkers concatenate `.comment` from every input object file without dedup): mikebom MUST dedup within-binary so the emitted array has each unique entry exactly once, in first-occurrence order. Bounds the property size on legacy build-systems.
- **`LC_BUILD_VERSION` with unknown platform value** (e.g., a hypothetical future platform `xros2`): emit the platform name verbatim — the binary's truth is whatever Apple emitted. Don't gate on a platform allowlist.
- **`LC_BUILD_VERSION` with malformed tools-array length** (e.g., `ntools=5` but only 3 tool records present): parse defensively — stop at the first malformed record, emit the records we successfully parsed, no scan failure.
- **PE optional-header missing** (some packed or obfuscated binaries strip or zero the optional header): the existing `object::read::pe::PeFile` parse fails, mikebom already drops the binary at scan time. No new path through this milestone's code.
- **Fat Mach-O slice disagreement** (the `LC_BUILD_VERSION` records differ across slices for ARM64 vs x86_64): emit the FIRST slice's record per milestone-024 precedent. Cross-slice variation is theoretically a build error; pragmatically, mikebom's caller cares about identity-tier consistency, not slice-by-slice toolchain differences.
- **Spurious `.comment` content from object-file linkage** (e.g., a static library's `.comment` ends up in the binary because the build system concatenated `.comment` sections naively): we emit what's there; the operator interprets. No filtering by toolchain heuristics — that would be policy, not data.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For every scanned ELF binary that has a `.comment` section, mikebom MUST emit a `mikebom:elf-compiler-stamps` property on the file-level binary component. Value is a JSON array of NUL-delimited entries from the section, lossy-UTF-8-decoded, within-binary deduped (each unique entry exactly once in first-occurrence order), with per-entry length capped at 4 KiB. Total property value capped at 64 KiB; if the dedup'd list exceeds the cap, emit the first N entries that fit plus a final `"... (truncated)"` marker.
- **FR-002**: For every scanned Mach-O binary that has an `LC_BUILD_VERSION` load command, mikebom MUST emit:
  - `mikebom:macho-build-version` property: JSON object `{"platform": <string>, "min_os": <string>, "sdk": <string>}`. The `platform` value follows Apple's enum (`macos`, `ios`, `tvos`, `watchos`, `xros`, etc.). Unknown platform IDs (the source is a `u32` enum) are stringified as `unknown-<numeric-id>` (e.g., `unknown-9999`) so emission remains deterministic without an allowlist gate.
  - `mikebom:macho-build-tools` property: JSON array `[{"tool": <string>, "version": <string>}, ...]` in declaration order. Tool names follow Apple's enum (`clang`, `ld`, `swift`, `metal`, etc.). Unknown tool IDs are stringified as `unknown-<numeric-id>` (same pattern as platform IDs above).
- **FR-003**: For every scanned PE binary, mikebom MUST emit a `mikebom:pe-linker-version` property whose value is the string `"<MajorLinkerVersion>.<MinorLinkerVersion>"` formatted from the optional header's two-byte field. Always-emit (matching the milestone-096 Q2 always-emit convention for `mikebom:binary-packed`) — value `"0.0"` is valid when the linker zeroed the field.
- **FR-004**: All three properties live on the file-level binary component only — not on the linkage-evidence components, not on the embedded-version-string components, not on the symbol-fingerprint components. The file-level component is the right scope for build-tier signals because they describe the BUILD, not the per-library identification.
- **FR-005**: No new Cargo dependencies. ELF `.comment` parsing uses `object::read::File::section_by_name_bytes(b".comment")` + std string splitting on NUL. Mach-O `LC_BUILD_VERSION` parsing extends the existing `parse_min_os_version` helper in `mikebom-cli/src/scan_fs/binary/macho.rs`. PE linker-version reads two bytes from the existing `ImageOptionalHeader` accessor at `pe.rs`.
- **FR-006**: Production code changes confined to `mikebom-cli/src/scan_fs/binary/`. The three readers extend the existing per-format modules (`elf.rs`, `macho.rs`, `pe.rs`); the new fields land on `BinaryScan`; the file-level component-builder in `entry.rs` emits the properties via the existing `extra_annotations` bag (the same pattern the milestone-023/024/028 identity helpers use). No changes to `mikebom-common/`, no changes to `generate/`, no parity-catalog row needed (these are `mikebom:*` annotations — but per Constitution V audit at planning time, no format-native equivalent exists for any of the three).
- **FR-007**: Properties MUST be omitted (NOT emit empty/null/default values) when the underlying section / load command / field is absent. The absence of the property is the signal — operators can filter for "binaries with no build-tier provenance" by querying for components lacking `mikebom:elf-compiler-stamps`. Exception: `mikebom:pe-linker-version` is always-emit per FR-003 (PE binaries have this field by definition).
- **FR-008**: The new readers MUST be defensive against malformed input. Per Edge Cases: oversized entries truncate; unknown platform/tool enum IDs are stringified as `unknown-<numeric-id>` (per FR-002); malformed `tools` array length stops at the first bad record. Never panic; never `Err` upward — return a partial result and log via the existing `tracing::warn!` channel mikebom already uses for malformed-binary signals.
- **FR-009**: Goldens may regenerate for fixtures containing binary components. The new properties are additive; FR-009 (milestone-096) ≤1-spurious-bound semantics apply here too — at most ≤1 spurious component change per existing golden, and ≤3 new properties per affected file-level component.
- **FR-010**: Parity catalog rows (Constitution V audit at planning time): three new `mikebom:*` annotation properties need parity-catalog rows registering them as symmetric-equal across CDX 1.6 / SPDX 2.3 / SPDX 3 annotation emission. Mirror the existing C10/C11/C15 pattern (`mikebom:binary-class`, `mikebom:binary-stripped`, `mikebom:binary-packed`). New row IDs assigned at planning time based on the next available C-row number in the catalog.

### Key Entities

- **Compiler stamp**: a single string from the ELF `.comment` section's NUL-delimited list. Typically of the form `<toolchain-id>: <description> <version>` — e.g., `GCC: (Debian 12.2.0-14) 12.2.0`. mikebom does NOT sub-parse; the verbatim string is the data.
- **Mach-O build-version record**: a `(platform, min_os, sdk)` triple from `LC_BUILD_VERSION`. Strings are formatted via the same `version_string_from_packed` helper milestone-024 uses (3-component `<major>.<minor>.<patch>`).
- **Mach-O build-tool entry**: a `(tool, version)` pair from the `LC_BUILD_VERSION` tools-array, one per linker-recorded tool.
- **PE linker-version**: a `<major>.<minor>` decimal-dotted string formatted from the two-byte field in `IMAGE_OPTIONAL_HEADER`. Always-emit.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator scanning any modern Linux distro's `/bin/ls` (compiled by GCC or clang) sees a `mikebom:elf-compiler-stamps` property on the file-level binary component containing at least one stamp string starting with `"GCC: "` or `"clang version "` or `"icc "`.
- **SC-002**: An operator scanning any recent (Xcode 14+) macOS binary sees `mikebom:macho-build-version` and `mikebom:macho-build-tools` properties on the file-level binary component. The build-version's `platform` value is one of `macos / ios / tvos / watchos / xros`; the tools list contains at least one entry whose `tool` is `clang` or `ld`.
- **SC-003**: An operator scanning any PE binary (MSVC- or MinGW-built) sees a `mikebom:pe-linker-version` property whose value matches `^[0-9]+\.[0-9]+$`. Always-emit guarantee — even packed binaries with zeroed optional headers emit `"0.0"`.
- **SC-004**: 100% of pre-098 milestone test suites pass post-implementation. No regressions in any existing reader.
- **SC-005**: `./scripts/pre-pr.sh` clean post-implementation — zero clippy warnings, every test target reports `0 failed`. `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` opt-in also passes.
- **SC-006**: New unit tests in `elf.rs`, `macho.rs`, `pe.rs` cover: ELF single-stamp, ELF multi-toolchain dedup, ELF size-cap truncation, Mach-O `LC_BUILD_VERSION` happy path, Mach-O missing `LC_BUILD_VERSION` (silent skip), Mach-O malformed-tools-array defensive parse, PE linker-version happy path, PE zeroed-linker-version always-emit.
- **SC-007**: Existing-ecosystem golden regen scope bounded — at most 1 component per existing golden gains the new properties (matches milestone-096 SC-007 ≤1-component bound semantics). No PURLs change, no relationships change.
- **SC-008**: Zero new Cargo dependencies (FR-005).

## Assumptions

- The `object` crate already provides everything needed for parsing. ELF: `section_by_name_bytes(b".comment")` is the existing accessor (used at `scan_fs/binary/scan.rs:122` for `.note.package`). Mach-O: load-command iteration via the same primitives `parse_min_os_version` already uses (`mikebom-cli/src/scan_fs/binary/macho.rs`). PE: `ImageOptionalHeader` accessor at `pe.rs` (extends the existing `parse_pe_identity` flow).
- The `.comment` section is non-allocable (`SHT_PROGBITS` / `SHF_GROUP` typically without `SHF_ALLOC`) and isn't removed by `strip` by default. Manual `objcopy --remove-section .comment` does remove it; that case is the Edge Case "no `.comment` section" and emits no property.
- `LC_BUILD_VERSION` is the modern Mach-O build-stamp load command. The older `LC_VERSION_MIN_MACOSX` / `LC_VERSION_MIN_IPHONEOS` / etc. command family is already partially parsed via milestone-024's `parse_min_os_version` (mikebom emits `mikebom:macho-min-os`). This milestone narrows to `LC_BUILD_VERSION` specifically because it's the post-2018 standard; legacy `LC_VERSION_MIN_*` extraction is out of scope (the existing `min_os` extraction continues to work).
- PE's `MajorLinkerVersion`/`MinorLinkerVersion` are well-defined fields in `IMAGE_OPTIONAL_HEADER` (offset 26-27 in PE32, same offset in PE32+). Both linkers (MSVC and MinGW/binutils) write meaningful values. Some packers and obfuscators zero the field; the always-emit convention exposes that as a transparency signal rather than hiding it.
- Property values follow the existing `extra_annotations` bag pattern (BTreeMap<String, serde_json::Value>). The new properties flow through the existing milestone-023/024/028 identity-emission code path — no new wiring outside the readers.

## Dependencies

- **Milestone 004** (binary scanner foundation) — the existing per-format readers this milestone extends.
- **Milestone 023** (ELF identity signals) — establishes the pattern for emitting `mikebom:elf-*` properties from `BinaryScan` fields. `.comment` is a fourth ELF signal joining `build_id` / `runpath` / `debuglink`.
- **Milestone 024** (Mach-O identity signals) — establishes the `parse_min_os_version` helper this milestone extends to also capture SDK + tools.
- **Milestone 028** (PE identity signals) — establishes the `parse_pe_identity` helper this milestone extends to also capture linker version.
- **Milestone 096** (binary-id enrichment) — the always-emit convention for `mikebom:binary-packed` from Q2 is the precedent for FR-003's always-emit `pe-linker-version`.

## Out of Scope

- **DWARF debug-info parsing** (`.debug_info`, `.debug_line` sections). Higher cost (needs `gimli` crate = new dep with ~50 transitive crates), narrow utility (most unknown binaries are stripped). Could be a follow-up milestone if signal emerges.
- **`cargo-auditable` decoder changes**. Already shipped in milestone 029; this milestone doesn't touch the cargo-auditable extraction path.
- **CPE candidate emission for compilers/linkers** (e.g., `cpe:2.3:a:gnu:gcc:12.2.0:*:*:*:*:*:*:*`). Compiler CVEs are rare and the candidate-generation logic is more involved than the milestone-097 pattern (vendor-product mapping for compilers has fewer canonical conventions in NVD). Defer.
- **Sub-parsing `.comment` into structured records** (e.g., extracting `version = 12.2.0`, `vendor = Debian` from `GCC: (Debian 12.2.0-14) 12.2.0`). The verbatim string is the data; downstream tooling can sub-parse if it wants. Mikebom emits, doesn't classify.
- **Legacy Mach-O `LC_VERSION_MIN_MACOSX` / `LC_VERSION_MIN_IPHONEOS` extraction beyond `min_os`**. Milestone 024 covers `min_os` extraction from those commands; the SDK + tools fields don't exist in the legacy command family.
- **Per-input-object `.comment` provenance** (mapping individual stamps back to which static library / object file contributed them). Requires symbol/debug-info correlation that we deliberately don't carry. Out of scope.
- **PE `IMAGE_FILE_HEADER.TimeDateStamp`** — already covered in milestone 028's identity emission (not this one's surface).
- **Build-host identity extraction** (e.g., `CFLAGS`, `-fdebug-prefix-map` source-path hints embedded in DWARF). Out of scope; orthogonal to compiler/linker version stamps. May overlap with the deferred DWARF milestone.
- **Yara/binwalk-style heuristic toolchain fingerprinting** (matching code-emission patterns to specific compiler versions). High-value for stripped binaries with no `.comment` but adds a maintained-rule corpus. Out of scope for v1.
- **OS-distro detection from compiler-stamp vendor strings** (e.g., parsing `GCC: (Debian 12.2.0-14) 12.2.0` to extract `Debian` as the distro). Out of scope; the milestone-003 `.note.package` extraction is the authoritative distro signal.
- **Differential CFG-vs-stamp consistency checks** (verify the stamp matches the actual code-emission style). Beyond scope; mikebom emits the stamps as evidence, not as ground-truth.
