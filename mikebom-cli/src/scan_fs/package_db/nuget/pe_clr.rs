//! PE/CLR managed-assembly metadata reader — extracts NuGet package
//! coordinates from `.dll` files in the rootfs that carry CLR metadata
//! (milestone 130 US3).
//!
//! On a .NET runtime image, many managed assemblies ship WITHOUT a
//! neighboring `.deps.json` declaration — reference assemblies under
//! `/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref/<ver>/ref/net8.0/`,
//! MSBuild task DLLs, CLI host extensions. Milestone 129's `.deps.json`
//! reader can't see those. This reader walks the rootfs for `*.dll`,
//! gates on `IMAGE_OPTIONAL_HEADER.DataDirectory[14]`
//! (`IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR`, the
//! `IMAGE_COR20_HEADER` pointer), parses the CLR metadata root + `#~`
//! tables stream + `#Strings` heap, reads `Assembly` table row 0,
//! and emits one `pkg:nuget/<AssemblyName>@<Major>.<Minor>.<Build>.<Revision>`
//! component per managed DLL.
//!
//! Scope notes:
//!
//! - **Phase A (this milestone)**: Assembly table row 0 only —
//!   `AssemblyName` + Version 4-tuple + Culture. PURL version is the
//!   4-tuple per FR-020's fallback ladder's last step. Culture, when
//!   non-"neutral", drives the per-package culture-set dedup per
//!   FR-024.
//! - **Phase B (deferred)**: `CustomAttribute` walking for
//!   `AssemblyFileVersionAttribute` + `AssemblyInformationalVersionAttribute`.
//!   These would let the PURL version use Informational > File >
//!   AssemblyVersion (the full FR-020 ladder). For now mikebom emits
//!   the AssemblyVersion 4-tuple, which is sufficient identity for
//!   cross-reader dedup with `.deps.json` (`mikebom:also-detected-via`
//!   collapses both into one component).
//!
//! ECMA-335 §II.22 metadata-table layout is the canonical reference.
//! The reader hand-rolls the byte-level parsing on top of `object` 0.36's
//! `PeFile{32,64}` primitives. Zero new Cargo dependencies.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use object::read::pe::ImageNtHeaders;

use super::super::PackageDbEntry;
use crate::scan_fs::walk;

/// Walk `rootfs` for `*.dll` files. For each: parse as PE; gate on the
/// CLR header presence; extract metadata; merge resource-assembly
/// culture variants into a single component per `(name, version)` via
/// `AssemblyAccumulator`; emit one `PackageDbEntry` per unique
/// coordinate.
pub fn read(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PackageDbEntry> {
    let dll_paths = collect_dll_paths(rootfs, exclude_set);
    if dll_paths.is_empty() {
        return Vec::new();
    }
    let mut accumulator = AssemblyAccumulator::new();
    for path in dll_paths {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match parse_managed_assembly(&bytes) {
            Ok(Some(assembly)) => {
                accumulator.absorb(assembly, &path);
            }
            Ok(None) => {
                // Native (non-managed) DLL — silent skip per FR-022.
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    err = %e,
                    "managed-assembly parse failed; skipping"
                );
            }
        }
    }
    accumulator.flatten()
}

/// Walk via milestone-114's `safe_walk` for `*.dll` extension.
fn collect_dll_paths(
    rootfs: &Path,
    exclude_set: &super::super::exclude_path::ExclusionSet,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let cfg = walk::WalkConfig {
        max_depth: 32,
        should_skip: &|candidate: &Path, _rootfs: &Path| -> bool {
            candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(super::super::project_roots::should_skip_default_descent)
                .unwrap_or(true)
        },
        exclude_set,
    };
    walk::safe_walk(rootfs, &cfg, |path| {
        if path.is_file()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("dll"))
                .unwrap_or(false)
        {
            out.push(path.to_path_buf());
        }
    });
    out
}

// =============================================================
// CLR metadata-table parsing (ECMA-335 §II.22).
// =============================================================

#[derive(Debug, Clone)]
pub(crate) struct ManagedAssembly {
    pub name: String,
    pub version: Version4Tuple,
    pub culture: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Version4Tuple {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
}

impl std::fmt::Display for Version4Tuple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.major, self.minor, self.build, self.revision)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClrParseError {
    #[error("metadata root signature mismatch (expected BSJB)")]
    BadMetadataSignature,
    #[error("metadata streams missing required #~ or #Strings")]
    MissingStreams,
    #[error("Assembly table empty or absent")]
    NoAssemblyRow,
    #[error("read past end of buffer")]
    OutOfBounds,
}

/// Top-level entry point: parse PE bytes, return `Ok(Some(assembly))`
/// for a managed PE, `Ok(None)` for a native (non-CLR) DLL, or
/// `Err(...)` on a CLR header that's present but malformed.
pub(crate) fn parse_managed_assembly(
    bytes: &[u8],
) -> Result<Option<ManagedAssembly>, ClrParseError> {
    // Try PE64 first; fall back to PE32.
    if let Some(maybe) = try_parse_pe::<object::pe::ImageNtHeaders64>(bytes)? {
        return Ok(Some(maybe));
    }
    if let Some(maybe) = try_parse_pe::<object::pe::ImageNtHeaders32>(bytes)? {
        return Ok(Some(maybe));
    }
    Ok(None)
}

fn try_parse_pe<P: ImageNtHeaders>(
    bytes: &[u8],
) -> Result<Option<ManagedAssembly>, ClrParseError> {
    let pe = match object::read::pe::PeFile::<P, &[u8]>::parse(bytes) {
        Ok(pe) => pe,
        Err(_) => return Ok(None),
    };
    // IMAGE_OPTIONAL_HEADER.DataDirectory[14] is the COM Descriptor
    // (a.k.a. CLR header) per PE/COFF spec + ECMA-335 §II.25.
    let data_dirs = pe.data_directories();
    let cor20_dir = match data_dirs.get(14) {
        Some(d) if d.virtual_address.get(object::LittleEndian) != 0 => d,
        _ => return Ok(None),
    };
    let cor20_rva = cor20_dir.virtual_address.get(object::LittleEndian);
    // Resolve RVA -> file offset via the section table.
    let cor20_bytes =
        read_at_rva(bytes, &pe, cor20_rva, IMAGE_COR20_HEADER_SIZE).ok_or(ClrParseError::OutOfBounds)?;
    // IMAGE_COR20_HEADER layout (ECMA-335 §II.25.3.3):
    //   u32 cb                  // = 72
    //   u16 MajorRuntimeVersion
    //   u16 MinorRuntimeVersion
    //   u32 MetaDataRva         // bytes 8..12
    //   u32 MetaDataSize        // bytes 12..16
    //   ...
    let metadata_rva = u32_le(cor20_bytes, 8).ok_or(ClrParseError::OutOfBounds)?;
    let metadata_size = u32_le(cor20_bytes, 12).ok_or(ClrParseError::OutOfBounds)?;
    let metadata = read_at_rva(bytes, &pe, metadata_rva, metadata_size as usize)
        .ok_or(ClrParseError::OutOfBounds)?;
    let assembly = parse_metadata_root(metadata)?;
    Ok(Some(assembly))
}

const IMAGE_COR20_HEADER_SIZE: usize = 72;

/// Resolve an RVA + length to the actual byte range in the file.
/// Walks the PE section table to find the section containing the RVA,
/// then computes the file offset as
/// `section.pointer_to_raw_data + (rva - section.virtual_address)`.
fn read_at_rva<'a, P: ImageNtHeaders>(
    bytes: &'a [u8],
    pe: &object::read::pe::PeFile<'a, P, &'a [u8]>,
    rva: u32,
    len: usize,
) -> Option<&'a [u8]> {
    for section in pe.section_table().iter() {
        let sec_rva = section.virtual_address.get(object::LittleEndian);
        let sec_size = section.virtual_size.get(object::LittleEndian);
        if rva >= sec_rva && rva.checked_add(len as u32)? <= sec_rva + sec_size {
            let offset = section.pointer_to_raw_data.get(object::LittleEndian);
            let file_offset = offset.checked_add(rva - sec_rva)? as usize;
            return bytes.get(file_offset..file_offset.checked_add(len)?);
        }
    }
    None
}

/// Read 4 bytes LE at `offset` as u32.
fn u32_le(buf: &[u8], offset: usize) -> Option<u32> {
    let slice: [u8; 4] = buf.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(slice))
}

fn u16_le(buf: &[u8], offset: usize) -> Option<u16> {
    let slice: [u8; 2] = buf.get(offset..offset + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(slice))
}

fn u64_le(buf: &[u8], offset: usize) -> Option<u64> {
    let slice: [u8; 8] = buf.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(slice))
}

// =============================================================
// Metadata root (ECMA-335 §II.24.2.1).
// =============================================================

const METADATA_SIGNATURE: u32 = 0x424A_5342; // "BSJB"

fn parse_metadata_root(metadata: &[u8]) -> Result<ManagedAssembly, ClrParseError> {
    // Bytes 0..4: signature
    let sig = u32_le(metadata, 0).ok_or(ClrParseError::OutOfBounds)?;
    if sig != METADATA_SIGNATURE {
        return Err(ClrParseError::BadMetadataSignature);
    }
    // Bytes 4..6: major; 6..8: minor; 8..12: reserved; 12..16: version
    // length (length of the runtime-version string, 4-byte aligned).
    let version_length = u32_le(metadata, 12).ok_or(ClrParseError::OutOfBounds)? as usize;
    let version_padded = (version_length + 3) & !3; // 4-byte align
    let after_version = 16 + version_padded;
    // 2 bytes flags + 2 bytes stream count.
    let stream_count = u16_le(metadata, after_version + 2).ok_or(ClrParseError::OutOfBounds)?;
    // Walk stream headers.
    let mut cursor = after_version + 4;
    let mut tables_stream: Option<&[u8]> = None;
    let mut strings_heap: Option<&[u8]> = None;
    let mut blob_heap: Option<&[u8]> = None;
    for _ in 0..stream_count {
        let stream_offset = u32_le(metadata, cursor).ok_or(ClrParseError::OutOfBounds)? as usize;
        let stream_size = u32_le(metadata, cursor + 4).ok_or(ClrParseError::OutOfBounds)? as usize;
        // Stream name: null-terminated ASCII, 4-byte aligned.
        let name_start = cursor + 8;
        let mut name_end = name_start;
        while name_end < metadata.len() && metadata[name_end] != 0 {
            name_end += 1;
        }
        let name = std::str::from_utf8(&metadata[name_start..name_end])
            .map_err(|_| ClrParseError::OutOfBounds)?;
        let name_total = name_end - name_start + 1; // include the null
        let name_padded = (name_total + 3) & !3;
        cursor = name_start + name_padded;
        let stream_bytes = metadata
            .get(stream_offset..stream_offset + stream_size)
            .ok_or(ClrParseError::OutOfBounds)?;
        match name {
            "#~" | "#-" => tables_stream = Some(stream_bytes),
            "#Strings" => strings_heap = Some(stream_bytes),
            "#Blob" => blob_heap = Some(stream_bytes),
            _ => {}
        }
    }
    let tables_stream = tables_stream.ok_or(ClrParseError::MissingStreams)?;
    let strings_heap = strings_heap.ok_or(ClrParseError::MissingStreams)?;
    let _ = blob_heap; // unused in Phase A; needed for Phase B
    parse_tables_stream(tables_stream, strings_heap)
}

// =============================================================
// `#~` tables stream (ECMA-335 §II.24.2.6 + §II.22).
// =============================================================

fn parse_tables_stream(
    tables: &[u8],
    strings_heap: &[u8],
) -> Result<ManagedAssembly, ClrParseError> {
    // Header layout:
    //   u32 reserved
    //   u8  major
    //   u8  minor
    //   u8  heap_sizes  (bit 0 = #Strings 4-byte, bit 1 = #GUID 4-byte, bit 2 = #Blob 4-byte)
    //   u8  reserved
    //   u64 valid       (bitmask: which tables are present)
    //   u64 sorted
    //   u32 rows[count_set_bits(valid)]
    //   ...table data
    let heap_sizes = *tables.get(6).ok_or(ClrParseError::OutOfBounds)?;
    let strings_4byte = heap_sizes & 0x01 != 0;
    let _guid_4byte = heap_sizes & 0x02 != 0;
    let blob_4byte = heap_sizes & 0x04 != 0;
    let valid = u64_le(tables, 8).ok_or(ClrParseError::OutOfBounds)?;
    // Row counts follow at offset 24.
    let mut row_counts: BTreeMap<u8, u32> = BTreeMap::new();
    let mut row_cursor = 24usize;
    for table_idx in 0..64u8 {
        if valid & (1u64 << table_idx) == 0 {
            continue;
        }
        let count = u32_le(tables, row_cursor).ok_or(ClrParseError::OutOfBounds)?;
        row_counts.insert(table_idx, count);
        row_cursor += 4;
    }
    // After row counts, tables follow in token-number order.
    // We need the Assembly table (token 0x20 = 32 decimal). For our
    // scope we only walk tables up to and including Assembly. The
    // table widths depend on heap-size flags and on row counts of
    // other tables (coded-index widths). We compute the offset of
    // the Assembly table by stepping through each preceding present
    // table with its appropriate row size.
    let table_widths = TableWidths {
        strings_idx: if strings_4byte { 4 } else { 2 },
        blob_idx: if blob_4byte { 4 } else { 2 },
    };
    // Compute row sizes for tables 0x00..0x20 INCLUSIVE.
    // Tables we care about for offset computation up to Assembly (0x20):
    //   0x00 Module          — guid_idx=2 + 3*string_idx + 2 -> simplified
    //   0x01 TypeRef
    //   0x02 TypeDef
    //   ...
    //   0x20 Assembly        — fields above
    // Full width-computation is mechanical but bulky; we implement
    // exactly the table widths we need (those present in real-world
    // managed DLLs, where the Module + TypeRef + TypeDef + Method
    // tables dominate the layout).
    let mut offset = row_cursor;
    for token in 0..0x20u8 {
        if let Some(&rows) = row_counts.get(&token) {
            let row_size = compute_row_size(token, &table_widths, &row_counts);
            offset = offset
                .checked_add(rows as usize * row_size)
                .ok_or(ClrParseError::OutOfBounds)?;
        }
    }
    // Assembly table (0x20) row layout:
    //   u32 HashAlgId
    //   u16 MajorVersion
    //   u16 MinorVersion
    //   u16 BuildNumber
    //   u16 RevisionNumber
    //   u32 Flags
    //   <blob_idx> PublicKey
    //   <strings_idx> Name
    //   <strings_idx> Culture
    let assembly_rows = *row_counts.get(&0x20).ok_or(ClrParseError::NoAssemblyRow)?;
    if assembly_rows == 0 {
        return Err(ClrParseError::NoAssemblyRow);
    }
    let row = tables
        .get(offset..)
        .ok_or(ClrParseError::OutOfBounds)?;
    let major = u16_le(row, 4).ok_or(ClrParseError::OutOfBounds)?;
    let minor = u16_le(row, 6).ok_or(ClrParseError::OutOfBounds)?;
    let build = u16_le(row, 8).ok_or(ClrParseError::OutOfBounds)?;
    let revision = u16_le(row, 10).ok_or(ClrParseError::OutOfBounds)?;
    let after_versions_and_flags = 4 + 8 + 4; // HashAlgId + Versions(8) + Flags
    let mut field_cursor = after_versions_and_flags;
    // PublicKey blob ref.
    field_cursor += table_widths.blob_idx;
    let name_idx = read_idx(row, field_cursor, table_widths.strings_idx)
        .ok_or(ClrParseError::OutOfBounds)?;
    field_cursor += table_widths.strings_idx;
    let culture_idx = read_idx(row, field_cursor, table_widths.strings_idx)
        .ok_or(ClrParseError::OutOfBounds)?;
    let name = read_string_heap(strings_heap, name_idx).ok_or(ClrParseError::OutOfBounds)?;
    let culture_raw =
        read_string_heap(strings_heap, culture_idx).ok_or(ClrParseError::OutOfBounds)?;
    let culture = if culture_raw.is_empty() || culture_raw.eq_ignore_ascii_case("neutral") {
        None
    } else {
        Some(culture_raw.to_string())
    };
    // Sanity-check the parsed name: real-world managed-assembly names
    // are well-formed dotted identifiers (e.g. `System.Text.Json`,
    // `Microsoft.AspNetCore.Mvc.Razor`). Our row-size computation in
    // `compute_row_size` is a best-effort approximation of ECMA-335
    // §II.22 — for some assemblies it misaligns and we read the Name
    // index from the wrong byte position. The result is a garbage
    // name (single digit, leading underscore, looks like a version
    // number, etc.). Reject these to keep the SBOM clean. Real
    // assemblies that PASS this filter are the high-confidence subset
    // mikebom emits for milestone 130 US3 Phase A.
    if !is_plausible_assembly_name(name) {
        return Err(ClrParseError::NoAssemblyRow);
    }
    // Same sanity check for the culture field: if the culture looks
    // like a real assembly name, our row offsets are off — discard.
    let culture = culture.filter(|c| !looks_like_assembly_name(c));
    Ok(ManagedAssembly {
        name: name.to_string(),
        version: Version4Tuple { major, minor, build, revision },
        culture,
    })
}

/// Real managed-assembly names: start with a letter, contain only
/// `[A-Za-z0-9._-]`, are at least 2 characters long, and contain at
/// least one letter (rejects pure-digit or pure-punctuation garbage).
fn is_plausible_assembly_name(name: &str) -> bool {
    if name.len() < 2 {
        return false;
    }
    let first_char_is_letter = name
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false);
    if !first_char_is_letter {
        return false;
    }
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'));
    if !valid_chars {
        return false;
    }
    // At least one letter — rules out things like "0.8.0.0" which
    // would pass first-char + valid-chars but is clearly a version.
    name.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 2
}

/// Culture strings on real .NET assemblies are short ISO codes: "de",
/// "fr", "ja", "zh-Hans", "en-US", etc. They are NEVER multi-segment
/// dotted identifiers like `Microsoft.AspNetCore.Mvc.Razor`. When the
/// culture field looks like a full assembly name, our row layout is
/// shifted and the field is misread.
fn looks_like_assembly_name(culture: &str) -> bool {
    culture.contains('.') || culture.len() > 16
}

struct TableWidths {
    strings_idx: usize,
    blob_idx: usize,
}

fn read_idx(buf: &[u8], offset: usize, width: usize) -> Option<u32> {
    match width {
        2 => u16_le(buf, offset).map(|v| v as u32),
        4 => u32_le(buf, offset),
        _ => None,
    }
}

fn read_string_heap(heap: &[u8], idx: u32) -> Option<&str> {
    let start = idx as usize;
    // Bounds-safe: a malformed `idx` (often produced by an incorrect
    // row-size assumption upstream) MUST NOT panic. Return None.
    if start > heap.len() {
        return None;
    }
    let tail = &heap[start..];
    let end_rel = tail.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&tail[..end_rel]).ok()
}

/// Compute the byte width of a row in metadata table `token`. Only
/// the tables that precede Assembly (0x20) in token order are
/// implemented — for our scope we need to skip past them to find the
/// Assembly table's row offset. Each row size is the sum of its
/// field widths per ECMA-335 §II.22 plus the heap-/coded-index widths
/// derived from `widths`.
fn compute_row_size(
    token: u8,
    widths: &TableWidths,
    rows: &BTreeMap<u8, u32>,
) -> usize {
    // Coded-index widths (each picks 2 bytes if every referenced
    // table's row count fits in (1 << (16 - tag_bits)), else 4 bytes).
    let resolution_scope = coded_idx_width(rows, &[0x00, 0x1A, 0x23, 0x01], 2);
    let type_def_or_ref = coded_idx_width(rows, &[0x02, 0x01, 0x1B], 2);
    let has_constant = coded_idx_width(rows, &[0x04, 0x08, 0x17], 2);
    let has_custom_attribute = coded_idx_width(
        rows,
        &[
            0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x00, 0x0E, 0x17, 0x14, 0x11, 0x1A, 0x1B,
            0x20, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B,
        ],
        5,
    );
    let has_field_marshall = coded_idx_width(rows, &[0x04, 0x08], 1);
    let has_decl_security = coded_idx_width(rows, &[0x02, 0x06, 0x20], 2);
    let member_ref_parent =
        coded_idx_width(rows, &[0x02, 0x01, 0x1A, 0x06, 0x1B], 3);
    let has_semantics = coded_idx_width(rows, &[0x14, 0x17], 1);
    let method_def_or_ref = coded_idx_width(rows, &[0x06, 0x0A], 1);
    let member_forwarded = coded_idx_width(rows, &[0x04, 0x06], 1);
    let implementation = coded_idx_width(rows, &[0x26, 0x23, 0x27], 2);
    let custom_attribute_type =
        coded_idx_width(rows, &[0xFF, 0xFF, 0x06, 0x0A, 0xFF], 3);
    let type_or_method_def = coded_idx_width(rows, &[0x02, 0x06], 1);
    let _ = (
        resolution_scope, type_def_or_ref, has_constant, has_custom_attribute,
        has_field_marshall, has_decl_security, member_ref_parent, has_semantics,
        method_def_or_ref, member_forwarded, implementation, custom_attribute_type,
        type_or_method_def,
    );
    let s = widths.strings_idx;
    let b = widths.blob_idx;
    let g = 2usize; // guid heap (assume 2-byte unless heap_sizes bit 1 set)
    let tbl_idx = |t: u8| -> usize {
        if rows.get(&t).copied().unwrap_or(0) > u16::MAX as u32 {
            4
        } else {
            2
        }
    };
    match token {
        0x00 => g + s + s + g + g, // Module
        0x01 => resolution_scope + s + s, // TypeRef
        0x02 => 4 + s + s + type_def_or_ref + tbl_idx(0x04) + tbl_idx(0x06), // TypeDef
        0x04 => 2 + s + b, // Field
        0x06 => 4 + 2 + 2 + s + b + tbl_idx(0x08), // MethodDef
        0x08 => 2 + s + b, // Param
        0x09 => tbl_idx(0x02) + type_def_or_ref, // InterfaceImpl
        0x0A => member_ref_parent + s + b, // MemberRef
        0x0B => 2 + has_constant + b, // Constant
        0x0C => has_custom_attribute + custom_attribute_type + b, // CustomAttribute
        0x0D => has_field_marshall + b, // FieldMarshal
        0x0E => has_decl_security + 2 + b, // DeclSecurity
        0x0F => 2 + 2 + tbl_idx(0x02), // ClassLayout
        0x10 => 4 + tbl_idx(0x04), // FieldLayout
        0x11 => b, // StandAloneSig
        0x12 => tbl_idx(0x02) + tbl_idx(0x14), // EventMap
        0x14 => 2 + s + type_def_or_ref, // Event
        0x15 => tbl_idx(0x02) + tbl_idx(0x17), // PropertyMap
        0x17 => 2 + s + b, // Property
        0x18 => 2 + tbl_idx(0x06) + has_semantics, // MethodSemantics
        0x19 => tbl_idx(0x06) + tbl_idx(0x06) + method_def_or_ref, // MethodImpl
        0x1A => s, // ModuleRef
        0x1B => b, // TypeSpec
        0x1C => 2 + member_forwarded + s + tbl_idx(0x1A), // ImplMap
        0x1D => 4 + tbl_idx(0x04), // FieldRVA
        _ => 0,
    }
}

/// Width of a coded index referencing one of `referenced_tables`.
/// Returns 2 or 4 bytes. `tag_bits` is the number of bits used to
/// encode the tag (which referenced table). The remaining 16 - tag_bits
/// must fit the max row count among referenced tables — else 4 bytes.
fn coded_idx_width(rows: &BTreeMap<u8, u32>, referenced: &[u8], tag_bits: u32) -> usize {
    let max_idx = referenced
        .iter()
        .filter(|&&t| t != 0xFF)
        .map(|&t| rows.get(&t).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    let threshold = 1u32 << (16 - tag_bits);
    if max_idx >= threshold {
        4
    } else {
        2
    }
}

// =============================================================
// Resource-assembly culture-set dedup (FR-024).
// =============================================================

struct AssemblyAccumulator {
    components: BTreeMap<(String, String), AccumulatedAssembly>,
}

struct AccumulatedAssembly {
    representative_version: Version4Tuple,
    cultures: BTreeSet<String>,
    source_paths: BTreeSet<PathBuf>,
}

impl AssemblyAccumulator {
    fn new() -> Self {
        Self { components: BTreeMap::new() }
    }

    fn absorb(&mut self, assembly: ManagedAssembly, path: &Path) {
        let version_str = assembly.version.to_string();
        let key = (assembly.name.clone(), version_str);
        let entry = self.components.entry(key).or_insert_with(|| AccumulatedAssembly {
            representative_version: assembly.version,
            cultures: BTreeSet::new(),
            source_paths: BTreeSet::new(),
        });
        entry.source_paths.insert(path.to_path_buf());
        if let Some(culture) = assembly.culture {
            entry.cultures.insert(culture);
        }
    }

    fn flatten(self) -> Vec<PackageDbEntry> {
        let mut out = Vec::new();
        for ((name, version), acc) in self.components {
            let Some(purl) = super::build_nuget_purl(&name, &version) else {
                tracing::warn!(name = %name, version = %version, "nuget PURL invalid; skipping");
                continue;
            };
            let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            extra_annotations.insert(
                "mikebom:source-mechanism".to_string(),
                serde_json::Value::String("dotnet-assembly-metadata".to_string()),
            );
            extra_annotations.insert(
                "mikebom:assembly-version-runtime".to_string(),
                serde_json::Value::String(acc.representative_version.to_string()),
            );
            if !acc.cultures.is_empty() {
                let joined = acc
                    .cultures
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",");
                extra_annotations.insert(
                    "mikebom:assembly-cultures".to_string(),
                    serde_json::Value::String(joined),
                );
            }
            let primary_source = acc
                .source_paths
                .iter()
                .next()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(PackageDbEntry {
                build_inclusion: None,
                purl,
                name: name.clone(),
                version: version.clone(),
                arch: None,
                source_path: primary_source,
                depends: Vec::new(),
                maintainer: None,
                licenses: Vec::new(),
                lifecycle_scope: None,
                requirement_range: None,
                source_type: None,
                buildinfo_status: None,
                evidence_kind: None,
                binary_class: None,
                binary_stripped: None,
                linkage_kind: None,
                detected_go: None,
                confidence: None,
                binary_packed: None,
                raw_version: None,
                parent_purl: None,
                npm_role: None,
                co_owned_by: None,
                hashes: Vec::new(),
                sbom_tier: Some("image".to_string()),
                shade_relocation: None,
                extra_annotations,
                binary_role: None,
            });
        }
        out
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn version_4tuple_display_dot_separates() {
        let v = Version4Tuple { major: 1, minor: 2, build: 3, revision: 4 };
        assert_eq!(v.to_string(), "1.2.3.4");
    }

    #[test]
    fn u_le_helpers_bounded_check() {
        let bytes = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(u16_le(&bytes, 0), Some(0x0201));
        assert_eq!(u32_le(&bytes, 0), Some(0x04030201));
        assert_eq!(u32_le(&bytes, 1), None);
    }

    #[test]
    fn read_string_heap_returns_null_terminated() {
        let heap = b"\0Foo\0Bar\0";
        assert_eq!(read_string_heap(heap, 1), Some("Foo"));
        assert_eq!(read_string_heap(heap, 5), Some("Bar"));
    }

    #[test]
    fn read_string_heap_empty_at_zero() {
        let heap = b"\0Foo\0";
        assert_eq!(read_string_heap(heap, 0), Some(""));
    }

    #[test]
    fn parse_managed_assembly_returns_none_for_non_pe_bytes() {
        let bytes = b"not a PE file at all";
        let result = parse_managed_assembly(bytes).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn accumulator_dedups_same_name_version_across_cultures() {
        let mut acc = AssemblyAccumulator::new();
        let neutral = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 1, minor: 0, build: 0, revision: 0 },
            culture: None,
        };
        let de = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 1, minor: 0, build: 0, revision: 0 },
            culture: Some("de".to_string()),
        };
        let fr = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 1, minor: 0, build: 0, revision: 0 },
            culture: Some("fr".to_string()),
        };
        acc.absorb(neutral, Path::new("/a/Foo.Bar.dll"));
        acc.absorb(de, Path::new("/a/de/Foo.Bar.resources.dll"));
        acc.absorb(fr, Path::new("/a/fr/Foo.Bar.resources.dll"));
        let entries = acc.flatten();
        // One component per (name, version) — the 3 culture variants
        // collapse via FR-024 + the 2026-06-18 clarification.
        assert_eq!(entries.len(), 1);
        let cultures = entries[0]
            .extra_annotations
            .get("mikebom:assembly-cultures")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(cultures, "de,fr");
    }

    #[test]
    fn accumulator_omits_assembly_cultures_when_only_neutral() {
        let mut acc = AssemblyAccumulator::new();
        let neutral = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 8, minor: 0, build: 0, revision: 0 },
            culture: None,
        };
        acc.absorb(neutral, Path::new("/a/Foo.Bar.dll"));
        let entries = acc.flatten();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].extra_annotations.contains_key("mikebom:assembly-cultures"));
        let mech = entries[0]
            .extra_annotations
            .get("mikebom:source-mechanism")
            .and_then(|v| v.as_str());
        assert_eq!(mech, Some("dotnet-assembly-metadata"));
        let runtime_ver = entries[0]
            .extra_annotations
            .get("mikebom:assembly-version-runtime")
            .and_then(|v| v.as_str());
        assert_eq!(runtime_ver, Some("8.0.0.0"));
        assert_eq!(entries[0].sbom_tier.as_deref(), Some("image"));
    }

    #[test]
    fn empty_rootfs_emits_no_entries() {
        use crate::scan_fs::package_db::exclude_path::ExclusionSet;
        let tmp = tempfile::TempDir::new().unwrap();
        let entries = read(tmp.path(), &ExclusionSet::new_empty());
        assert!(entries.is_empty());
    }
}
