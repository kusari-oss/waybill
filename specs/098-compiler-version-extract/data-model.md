# Data Model — milestone 098

Per-file shape of every deliverable. Three new extractors (one per binary format) + four new properties + 4 new parity-catalog rows + per-format unit tests + one optional integration test.

## File inventory

| File | State | Owner FRs |
|------|-------|-----------|
| `mikebom-cli/src/scan_fs/binary/elf.rs` | EXTEND | FR-001 |
| `mikebom-cli/src/scan_fs/binary/macho.rs` | EXTEND | FR-002 |
| `mikebom-cli/src/scan_fs/binary/pe.rs` | EXTEND | FR-003 |
| `mikebom-cli/src/scan_fs/binary/entry.rs` | MODIFY | wire 4 new BinaryScan fields + emit via build_*_identity_annotations |
| `mikebom-cli/src/scan_fs/binary/scan.rs` | MODIFY | invoke the new parsers; populate BinaryScan |
| `mikebom-cli/src/parity/extractors/mod.rs` | MODIFY | 4 new catalog rows |
| `mikebom-cli/src/parity/extractors/cdx.rs` | MODIFY | 4 new `cdx_anno!` invocations |
| `mikebom-cli/src/parity/extractors/spdx2.rs` | MODIFY | 4 new `spdx23_anno!` invocations |
| `mikebom-cli/src/parity/extractors/spdx3.rs` | MODIFY | 4 new `spdx3_anno!` invocations |
| `docs/reference/sbom-format-mapping.md` | MODIFY | 4 new rows |
| `mikebom-cli/tests/binary_build_provenance.rs` | NEW (optional) | system-binary integration smoke test |

## `elf.rs` — extension

**New function** (per research §2):

```rust
/// Parse the ELF `.comment` section's NUL-delimited stamp list per
/// milestone-098 FR-001. Within-binary dedup; per-entry 4 KiB cap;
/// total emitted length 64 KiB cap with `"... (truncated)"` marker
/// appended when the cap is hit. Lossy-UTF-8 decode for non-UTF-8
/// bytes (replacement char `\u{FFFD}` per Edge Case US1#4).
pub fn parse_comment_section(data: &[u8]) -> Vec<String> { /* ... */ }
```

**Wired into `scan.rs`**: add a new line in the ELF-specific section after the existing build-id / runpath / debuglink extraction:

```rust
let comment_stamps = if class == "elf" {
    file.section_by_name_bytes(b".comment")
        .and_then(|s| s.data().ok())
        .map(elf::parse_comment_section)
        .unwrap_or_default()
} else {
    Vec::new()
};
```

## `macho.rs` — extension

**New struct** (per research §3):

```rust
/// Full LC_BUILD_VERSION record per milestone-098 FR-002. Returned by
/// `parse_build_version_full`; consumed by entry.rs::build_macho_identity_annotations.
pub struct MachoBuildVersion {
    pub platform: String,
    pub min_os: String,
    pub sdk: String,
    pub tools: Vec<(String, String)>, // (tool_name, version) pairs
}

pub fn parse_build_version_full(bytes: &[u8]) -> Option<MachoBuildVersion> { /* ... */ }

fn tool_name(id: u32) -> Option<&'static str> {
    Some(match id {
        1 => "clang",
        2 => "swift",
        3 => "ld",
        4 => "lld",
        1024 => "metal",
        1025 => "airlld",
        _ => return None,
    })
}
```

**Wired into `scan.rs`** for both flat-Mach-O (`scan_binary`) and fat-Mach-O (`scan_fat_macho` — first slice only, milestone-024 convention).

## `pe.rs` — extension

**Modify** the existing `parse_pe_identity` return tuple to add a 4th element (linker version) — or refactor to a named struct. Pragmatic minimal-diff: extend the tuple.

```rust
pub type PeIdentity = (
    Option<String>, // pdb_id
    Option<String>, // machine
    Option<String>, // subsystem
    String,         // linker_version — always-emit (FR-003); "0.0" when zeroed
);

pub fn parse_linker_version<'a, Pe: ImageNtHeaders>(
    file: &PeFile<'a, Pe, &'a [u8]>,
) -> String {
    let opt = file.nt_headers().optional_header();
    format!("{}.{}", opt.major_linker_version(), opt.minor_linker_version())
}

// parse_pe_identity wired to also call parse_linker_version, both arms
// (PE32 and PE32+) returning the 4-element tuple. Non-PE / parse-fail
// returns `(None, None, None, String::from("unknown"))`.
```

**Note on `unknown`**: when PE parse fails (corrupt optional header), there's no real linker version to emit; the existing scan-time filter already drops un-parseable PE binaries before reaching `parse_pe_identity`. The `"unknown"` fallback handles the theoretical case where some callers might invoke `parse_pe_identity` on uncertain input — defensive only.

## `entry.rs` — BinaryScan + emission helpers

**New `BinaryScan` fields**:

```rust
pub(crate) struct BinaryScan {
    // ... existing fields ...

    /// Milestone 098 FR-001 — ELF `.comment` stamps. Empty for non-ELF
    /// or for ELF binaries lacking the section.
    pub comment_stamps: Vec<String>,

    /// Milestone 098 FR-002 — Mach-O LC_BUILD_VERSION full record.
    /// `None` for non-Mach-O or for binaries lacking the command.
    pub macho_build_version: Option<macho::MachoBuildVersion>,

    /// Milestone 098 FR-003 — PE linker-version `<major>.<minor>`.
    /// Always-emit on PE binaries; `None` for non-PE.
    pub pe_linker_version: Option<String>,
}
```

**Extend `build_elf_identity_annotations`** to emit `mikebom:elf-compiler-stamps` when `comment_stamps` is non-empty:

```rust
if !scan.comment_stamps.is_empty() {
    bag.insert(
        "mikebom:elf-compiler-stamps".to_string(),
        serde_json::json!(scan.comment_stamps),
    );
}
```

**Extend `build_macho_identity_annotations`** to emit `mikebom:macho-build-version` + `mikebom:macho-build-tools` when `macho_build_version` is `Some(...)`:

```rust
if let Some(bv) = &scan.macho_build_version {
    bag.insert(
        "mikebom:macho-build-version".to_string(),
        serde_json::json!({
            "platform": bv.platform,
            "min_os": bv.min_os,
            "sdk": bv.sdk,
        }),
    );
    if !bv.tools.is_empty() {
        let tools_json: Vec<serde_json::Value> = bv.tools.iter()
            .map(|(tool, version)| serde_json::json!({"tool": tool, "version": version}))
            .collect();
        bag.insert(
            "mikebom:macho-build-tools".to_string(),
            serde_json::Value::Array(tools_json),
        );
    }
}
```

**Extend `build_pe_identity_annotations`** to emit `mikebom:pe-linker-version`:

```rust
if let Some(lv) = &scan.pe_linker_version {
    bag.insert(
        "mikebom:pe-linker-version".to_string(),
        serde_json::Value::String(lv.clone()),
    );
}
```

## Parity-catalog rows (4 new — likely C16-C19)

**`parity/extractors/mod.rs`** (append after C15):

```rust
ParityExtractor { row_id: "C16", label: "mikebom:elf-compiler-stamps",  cdx: c16_cdx, spdx23: c16_spdx23, spdx3: c16_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
ParityExtractor { row_id: "C17", label: "mikebom:macho-build-version",  cdx: c17_cdx, spdx23: c17_spdx23, spdx3: c17_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
ParityExtractor { row_id: "C18", label: "mikebom:macho-build-tools",    cdx: c18_cdx, spdx23: c18_spdx23, spdx3: c18_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
ParityExtractor { row_id: "C19", label: "mikebom:pe-linker-version",    cdx: c19_cdx, spdx23: c19_spdx23, spdx3: c19_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

**Note on `order_sensitive`**: C16 (`elf-compiler-stamps`) and C18 (`macho-build-tools`) are arrays whose ORDER matters (entries in `.comment` declaration order; tools-array in LC_BUILD_VERSION declaration order). C17 (`macho-build-version`) and C19 (`pe-linker-version`) are scalars — order N/A.

**Per-format extractors** (cdx.rs / spdx2.rs / spdx3.rs):

```rust
// cdx.rs
cdx_anno!(c16_cdx, "mikebom:elf-compiler-stamps", component);
cdx_anno!(c17_cdx, "mikebom:macho-build-version", component);
cdx_anno!(c18_cdx, "mikebom:macho-build-tools", component);
cdx_anno!(c19_cdx, "mikebom:pe-linker-version", component);

// spdx2.rs / spdx3.rs: same shape with spdx23_anno! / spdx3_anno!.
```

## `docs/reference/sbom-format-mapping.md`

Add 4 new rows mirroring C10/C11/C15's format:

| Row | Label | CDX 1.6 | SPDX 2.3 | SPDX 3 | Native equivalent? |
|-----|-------|---------|----------|--------|---------------------|
| C16 | `mikebom:elf-compiler-stamps` | property | Annotation on Package | Annotation on Package | No native field. |
| C17 | `mikebom:macho-build-version` | property | Annotation on Package | Annotation on Package | No native field. |
| C18 | `mikebom:macho-build-tools` | property | Annotation on Package | Annotation on Package | No native field. |
| C19 | `mikebom:pe-linker-version` | property | Annotation on Package | Annotation on Package | No native field. |

## Unit tests (per SC-006)

**`elf.rs::tests`**:
- `parse_comment_section_single_stamp` — one NUL-terminated string → one entry.
- `parse_comment_section_multi_toolchain_dedup` — GCC + clang stamps interleaved with duplicates → 2 unique entries in first-occurrence order.
- `parse_comment_section_oversize_truncation` — 5 KiB entry → truncated to 4 KiB + `"... (truncated)"` suffix.
- `parse_comment_section_total_cap` — 200 short entries each ~500 bytes → first N fit + final `"... (truncated)"` marker.
- `parse_comment_section_empty_section` — section data is all NULs → empty Vec.
- `parse_comment_section_non_utf8` — entry contains 0xFF byte → lossy decode emits replacement char.

**`macho.rs::tests`**:
- `parse_build_version_full_happy_path` — synthesized LC_BUILD_VERSION with platform=macos, minos=14.0, sdk=14.4, 2 tools → matches expected struct.
- `parse_build_version_full_unknown_platform` — synthesized LC_BUILD_VERSION with platform=9999 → platform string is `"unknown-9999"`.
- `parse_build_version_full_malformed_ntools` — synthesized record claims ntools=5 but cmdsize only fits 2 records → returns partial result with 2 tools.
- `parse_build_version_full_missing_command` — Mach-O bytes without LC_BUILD_VERSION → returns `None`.

**`pe.rs::tests`**:
- `parse_pe_identity_emits_linker_version` — extend existing happy-path test to also assert `linker_version == "14.36"` (or whatever bytes the synthetic PE carries).
- `parse_pe_identity_zeroed_linker_version` — synthesized PE with MajorLinkerVersion=0, MinorLinkerVersion=0 → `linker_version == "0.0"` (always-emit).

## Integration test (optional)

`mikebom-cli/tests/binary_build_provenance.rs`:
- Scan the mikebom binary itself (always available during `cargo test`).
- Per host OS:
  - Linux: assert `mikebom:elf-compiler-stamps` property exists with ≥1 entry. Don't lock the prefix (rustc's stamp format varies; mikebom binary may carry GCC stamps from linked C deps too).
  - macOS: assert `mikebom:macho-build-version` exists with `platform == "macos"` and `mikebom:macho-build-tools` exists with ≥1 entry.
  - Windows: assert `mikebom:pe-linker-version` exists with format `^\d+\.\d+$`.
- Graceful-skip on unsupported hosts.

## Compatibility

- **No `Cargo.lock` change** — pure in-source parsing; `object` crate already in workspace.
- **Goldens regen forecast** — at most ≤1 component per existing golden gains the new properties (matches milestone-096 SC-007 ≤1-component bound). Likely zero — existing ecosystem fixtures don't include file-level binary components from real OS binaries; the `mikebom:binary-class` always-emit from milestone 096 didn't cause regen, so this milestone shouldn't either.
- **Backward compatibility** — 100% additive. The existing `mikebom:macho-min-os` (milestone 024) continues to emit unchanged; the new `mikebom:macho-build-version` is a SEPARATE structured property.

## No JSON / no YAML schema additions

Zero new fields in any output schema. The new properties flow through the existing `component.properties[]` (CDX) / `Package.annotations[]` (SPDX 2.3) / `Annotation` elements (SPDX 3) mechanisms — same path as every other `mikebom:*` annotation since milestone 004.
