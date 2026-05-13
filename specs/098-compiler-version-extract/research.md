# Research — milestone 098 compiler/linker version extraction

Phase 0 investigation. Five decision points, all resolved with audit-of-existing-code outcomes plus per-format binary-layout references.

## §1 — Constitution V audit (FR-010 / parity-catalog row count)

**Decision**: emit four new `mikebom:*` annotation properties — `mikebom:elf-compiler-stamps`, `mikebom:macho-build-version`, `mikebom:macho-build-tools`, `mikebom:pe-linker-version`. Each needs a new parity-catalog row.

**Audit** (per Constitution V "standards-native fields take precedence"):

| Format | Existing native field for compiler/linker stamps? | Decision |
|--------|---------------------------------------------------|----------|
| CDX 1.6 | NO. `component.publisher` is the closest related field (a free-form supplier string) but it describes the publisher of the component, not the toolchain that built it. CDX 1.6's `component.evidence` is identification-tier (what is it?), not build-tier (how was it made?). | Use `component.properties[]` with `mikebom:*` name. |
| SPDX 2.3 | NO. `Package.builtDate` exists (release timestamp) but not a build-toolchain field. `Package.originator` is a supplier, not a toolchain. | Use `Package.annotations[].comment` with `mikebom:*` prefix. |
| SPDX 3 | NO. The `Build` profile defines `Build` elements with `buildType`, `configSrcUri`, `configFromFile`, etc. — broader build-instructions metadata, not per-binary toolchain identifiers. Adding a separate `Build` element per binary would over-model what is just a single string property. | Use `Annotation` elements with `mikebom:*` prefix on the `statement`. |

**Result**: `mikebom:*` namespace justified for all four properties, mirroring the milestone-023/024/028 identity-property audit conclusions for `mikebom:elf-build-id` etc.

**Parity-catalog row IDs**: existing catalog has rows up through C15 (`mikebom:binary-packed`). Milestone 098 takes the next available — likely C16, C17, C18, C19 (verified at implementation time via `grep -E '^\s*ParityExtractor.*row_id' mikebom-cli/src/parity/extractors/mod.rs`).

## §2 — ELF `.comment` section parser (FR-001)

**Decision**: simple NUL-delimited string splitter with within-section dedup, per-entry 4 KiB cap, total 64 KiB cap. Implement as a new `parse_comment_section(data: &[u8]) -> Vec<String>` helper in `elf.rs`.

**Binary layout**: `.comment` is a non-allocable `SHT_PROGBITS` section. Its data is a sequence of NUL-terminated UTF-8 strings. Most toolchains write a single string (e.g., `GCC: (Debian 12.2.0-14) 12.2.0\0`); some linkers concatenate `.comment` sections from input object files, producing multiple stamps (potentially with duplicates from static-library archives).

**Reference**: GNU `as` writes the stamp via the `.ident` pseudo-op; the assembler places it in `.comment`. ELF gABI permits but doesn't mandate the section. Behavior:
- `gcc` / `g++` / `clang`: always emit a stamp by default; suppressed by `-fno-ident`.
- `ld`: concatenates `.comment` from input objects, deduplicating identical consecutive entries (but not all duplicates).
- `strip --keep-section=.comment` (default): retains the section. `strip --remove-section=.comment` or `objcopy --remove-section=.comment` strip it.

**Implementation pseudo**:

```rust
pub fn parse_comment_section(data: &[u8]) -> Vec<String> {
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let mut total_bytes = 0usize;
    const PER_ENTRY_CAP: usize = 4096;
    const TOTAL_CAP: usize = 64 * 1024;
    for entry in data.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        if !seen.insert(entry) {
            continue; // within-binary dedup
        }
        if total_bytes >= TOTAL_CAP {
            out.push("... (truncated)".to_string());
            break;
        }
        let entry_str = if entry.len() <= PER_ENTRY_CAP {
            String::from_utf8_lossy(entry).into_owned()
        } else {
            let mut s = String::from_utf8_lossy(&entry[..PER_ENTRY_CAP]).into_owned();
            s.push_str(" ... (truncated)");
            s
        };
        total_bytes += entry_str.len();
        out.push(entry_str);
    }
    out
}
```

**Rationale**: linear-scan over a small section, `HashSet` for O(1) dedup, two caps to bound worst-case property size. UTF-8 lossy decode (per FR Edge Case) emits replacement characters rather than failing.

**Alternatives considered**:
- **No dedup**: rejected — `ld`'s naive concatenation can produce hundreds of identical GCC stamps from a static-library-heavy build.
- **External crate (e.g., `bstr`)**: rejected — `String::from_utf8_lossy` from std covers the same need with zero dep cost.

## §3 — Mach-O `LC_BUILD_VERSION` SDK + tools-array parser (FR-002)

**Decision**: new `parse_build_version_full(bytes: &[u8]) -> Option<MachoBuildVersion>` helper in `macho.rs` that mirrors `parse_min_os_version`'s load-command iteration pattern but extracts ALL fields (platform, min_os, sdk, tools-array).

**Binary layout** (per Apple's `<mach-o/loader.h>`):

```c
struct build_version_command {
    uint32_t cmd;        // LC_BUILD_VERSION = 0x32
    uint32_t cmdsize;    // sizeof(struct build_version_command) + ntools * sizeof(struct build_tool_version)
    uint32_t platform;   // PLATFORM_MACOS, _IOS, etc.
    uint32_t minos;      // X.Y.Z encoded as nibble-packed u32 (X << 16 | Y << 8 | Z)
    uint32_t sdk;        // X.Y.Z encoded same as minos
    uint32_t ntools;     // count of build_tool_version records that follow
};
struct build_tool_version {
    uint32_t tool;       // TOOL_CLANG=1, TOOL_SWIFT=2, TOOL_LD=3, TOOL_LLD=4, TOOL_METAL=1024, ...
    uint32_t version;    // X.Y.Z encoded same as minos
};
```

**Tool ID mapping** (per `<mach-o/loader.h>`, captured at planning time for the in-source enum):

| ID | Name |
|----|------|
| 1 | clang |
| 2 | swift |
| 3 | ld |
| 4 | lld |
| 1024 | metal |
| 1025 | airlld |

**Defensive parsing** (per FR-008 / Edge Case "malformed ntools"):
- Read `ntools` from the load-command header.
- Compute expected total command size: `24 + ntools * 8` bytes.
- If `cmdsize < expected_size`, parse min(`ntools`, actual_records_in_cmdsize) records — stop at the first record that runs off the end. Emit a `tracing::warn!` and continue with the partial result.

**Implementation pseudo**:

```rust
pub struct MachoBuildVersion {
    pub platform: String,
    pub min_os: String,
    pub sdk: String,
    pub tools: Vec<(String, String)>,  // (tool_name, version)
}

pub fn parse_build_version_full(bytes: &[u8]) -> Option<MachoBuildVersion> {
    let header = decode_header(bytes)?;
    for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        if cmd != LC_BUILD_VERSION {
            return None;
        }
        let platform_id = read_u32(cmd_bytes, 8, header.little_endian)?;
        let minos_packed = read_u32(cmd_bytes, 12, header.little_endian)?;
        let sdk_packed = read_u32(cmd_bytes, 16, header.little_endian)?;
        let ntools = read_u32(cmd_bytes, 20, header.little_endian)? as usize;
        let platform = platform_name(platform_id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("unknown-{platform_id}"));
        let mut tools = Vec::new();
        for i in 0..ntools {
            let base = 24 + i * 8;
            let Some(tool_id) = read_u32(cmd_bytes, base, header.little_endian) else { break };
            let Some(version_packed) = read_u32(cmd_bytes, base + 4, header.little_endian) else { break };
            let tool_name = tool_name(tool_id)
                .map(str::to_string)
                .unwrap_or_else(|| format!("unknown-{tool_id}"));
            tools.push((tool_name, decode_packed_version(version_packed)));
        }
        Some(MachoBuildVersion {
            platform,
            min_os: decode_packed_version(minos_packed),
            sdk: decode_packed_version(sdk_packed),
            tools,
        })
    })
}
```

**Fat-Mach-O handling**: extends `scan_fat_macho` at `scan.rs:280` to call `parse_build_version_full` against the first slice's bytes (matches the existing milestone-024 first-slice convention for `parse_lc_uuid` + `parse_min_os_version`).

**Backward compat with milestone 024** (per Edge Case US2#4): `parse_min_os_version` continues to emit `mikebom:macho-min-os` independently. When `LC_BUILD_VERSION` is present, both `mikebom:macho-min-os` (flat string) AND `mikebom:macho-build-version` (structured object) emit — back-compat preserved. When only `LC_VERSION_MIN_*` is present, only `mikebom:macho-min-os` emits (legacy command lacks SDK + tools fields). Operators keying on the old flat property continue to work; operators wanting structured access use the new property.

## §4 — PE linker-version extraction (FR-003)

**Decision**: extend `parse_pe_identity` at `pe.rs:54` to also return `MajorLinkerVersion.MinorLinkerVersion` as a `String`. Always-emit per FR-003.

**Binary layout** (per Microsoft's PE specification):

`IMAGE_OPTIONAL_HEADER` starts at offset `e_lfanew + 4 + sizeof(IMAGE_FILE_HEADER)` = `e_lfanew + 4 + 20` = `e_lfanew + 24`. Layout:

```c
typedef struct _IMAGE_OPTIONAL_HEADER {
    WORD  Magic;                  // +0  PE32=0x10b, PE32+=0x20b
    BYTE  MajorLinkerVersion;     // +2
    BYTE  MinorLinkerVersion;     // +3
    DWORD SizeOfCode;             // +4
    // ... (more fields follow)
};
```

**Object-crate accessor**: `object::pe::ImageOptionalHeader32::major_linker_version: u8` and `minor_linker_version: u8` are already public fields (verified at planning time via `~/.cargo/registry/src/*/object-*/src/pe.rs:398,436`). The same fields exist on `ImageOptionalHeader64`. Both `PeFile32::optional_header()` and `PeFile64::optional_header()` expose them.

**Implementation**: extend the `PeIdentity` tuple from `(Option<String>, Option<String>, Option<String>)` to `(Option<String>, Option<String>, Option<String>, String)` where the new 4th element is the linker-version. Always populated (zero-zero → `"0.0"`).

Or, more pragmatically, return `PeIdentity` as a struct rather than a tuple — clearer for callers and future extension. Decision deferred to implementation time; the existing tuple is fine if implementer prefers minimal churn.

## §5 — Test fixture strategy

**Decision**: avoid toolchain-dependent fixture binaries. Test the parsers directly via hand-crafted byte slices (synthesizing `.comment` content, `LC_BUILD_VERSION` records, PE optional headers) as unit tests within each format module.

**Rationale**: this is the established pattern in milestones 023 / 024 / 028 / 030. The existing tests at `elf.rs:262-380` (`parse_note_package_*` tests), `macho.rs:502-660` (`parse_min_os_version`, `parse_lc_uuid` tests), and `pe.rs:400-480` (`parse_pe_identity_*` tests) all hand-craft minimal binary fixtures inline. Reuse the same approach.

**Optional integration test** (`mikebom-cli/tests/binary_build_provenance.rs`): scans the mikebom binary itself (which is guaranteed to be available during `cargo test`) and asserts:
- ELF builds emit `mikebom:elf-compiler-stamps` with a stamp starting with `"GCC: "` or `"clang version "` or `"rustc "` (Rust's own stamp). Mikebom is Rust → expect `"rustc "` prefix.
- Mach-O builds emit `mikebom:macho-build-version` with `platform = "macos"` and a tools list containing `ld` (Mach-O's linker version always appears).
- All builds emit `mikebom:pe-linker-version`? No — PE only. macOS scans of mikebom-self won't trip the PE arm.

Wait — actually rustc emits its own `.comment` stamp on Linux ELF builds. Let me check. **At implementation time**, the integration test will need to handle whatever stamp prefix rustc emits (likely `rustc version <ver>`). Conservative test: just assert "at least one stamp" rather than a specific prefix.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (`mikebom:elf-compiler-stamps` with caps + dedup) | §2 → parser + per-entry 4 KiB / total 64 KiB caps |
| FR-002 (`mikebom:macho-build-version` + `mikebom:macho-build-tools`) | §3 → `parse_build_version_full` extending the existing pattern |
| FR-003 (always-emit `mikebom:pe-linker-version`) | §4 → extend `parse_pe_identity`; `object` crate already exposes the bytes |
| FR-004 (file-level component only) | by-construction: properties live in `extra_annotations` bag which only exists on file-level components |
| FR-005 (no new Cargo deps) | §2-§4 → `object` crate + std cover all parsing |
| FR-006 (production code in `scan_fs/binary/`) | §2-§4 → all three changes are per-format module extensions |
| FR-007 (omit when absent; pe-linker exception) | by-construction: `Option<...>` fields in `BinaryScan` → emission skips on `None`; pe-linker is `String` (always-emit) |
| FR-008 (defensive parsing) | §3 → malformed-tools-array stop-at-first-bad-record; §2 → caps prevent over-read |
| FR-009 (golden regen bound) | inherits milestone-096 SC-007 semantics |
| FR-010 (parity-catalog rows) | §1 → 4 new rows (C16-C19 or next-available) |
| SC-001 (ELF stamps present on `/bin/ls`) | §5 → integration test via mikebom-self-scan |
| SC-002 (Mach-O build-version on Xcode 14+ binaries) | §5 → same integration test |
| SC-003 (PE linker-version matches `^[0-9]+\.[0-9]+$`) | §4 → format guarantee |
| SC-005/SC-006/SC-007/SC-008 | inherits from CI / FR-005 / milestone-096 conventions |
| Constitution V audit | §1 → 4 properties × 3 formats; native equivalent search came up empty for compiler/linker provenance |
| Constitution X transparency | §4 → always-emit `"0.0"` on zeroed PE linker exposes packer/obfuscator behavior |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
