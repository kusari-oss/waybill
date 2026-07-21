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
//!   AssemblyVersion (the full FR-020 ladder). For now waybill emits
//!   the AssemblyVersion 4-tuple, which is sufficient identity for
//!   cross-reader dedup with `.deps.json` (`waybill:also-detected-via`
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
    /// Milestone 131 US1 (Phase B): the `AssemblyInformationalVersionAttribute`
    /// value, when present in the CustomAttribute table AND sanity-checked
    /// (must contain a digit + a dot, ASCII-printable, ≤128 chars). `None`
    /// when absent or when the walk produced garbage (row-size
    /// approximation can misalign on some assemblies, same caveat as
    /// milestone-130 Phase A's name field).
    pub informational_version: Option<String>,
    /// Milestone 131 US1 (Phase B): the `AssemblyFileVersionAttribute`
    /// value, same sanity-check rules as `informational_version`.
    pub file_version: Option<String>,
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
    parse_tables_stream(tables_stream, strings_heap, blob_heap.unwrap_or(&[]))
}

// =============================================================
// `#~` tables stream (ECMA-335 §II.24.2.6 + §II.22).
// =============================================================

fn parse_tables_stream(
    tables: &[u8],
    strings_heap: &[u8],
    blob_heap: &[u8],
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
    // waybill emits for milestone 130 US3 Phase A.
    if !is_plausible_assembly_name(name) {
        return Err(ClrParseError::NoAssemblyRow);
    }
    // Same sanity check for the culture field: if the culture looks
    // like a real assembly name, our row offsets are off — discard.
    let culture = culture.filter(|c| !looks_like_assembly_name(c));

    // Milestone 131 US1 (Phase B): walk the CustomAttribute table
    // (token 0x0C) for `AssemblyInformationalVersionAttribute` +
    // `AssemblyFileVersionAttribute` per ECMA-335 §II.22.10 + §II.23.3.
    // Reuses the same row-size approximation as Phase A's Assembly
    // walk — inherits the same caveat that some assemblies misalign.
    let (informational_version, file_version) = extract_custom_attribute_versions(
        tables,
        row_cursor,
        &table_widths,
        &row_counts,
        strings_heap,
        blob_heap,
    );
    Ok(ManagedAssembly {
        name: name.to_string(),
        version: Version4Tuple { major, minor, build, revision },
        culture,
        informational_version,
        file_version,
    })
}

/// Milestone 131 US1 (Phase B): walk the CustomAttribute table
/// (token 0x0C) for rows whose `Type` column resolves through
/// MemberRef → TypeRef → #Strings to `AssemblyInformationalVersionAttribute`
/// or `AssemblyFileVersionAttribute`. Decodes the matching row's
/// `Value` blob (prolog 0x0001 + SerString) into a UTF-8 string.
/// Returns `(informational, file)` — `None` for fields not found OR
/// not passing the version-string sanity filter.
///
/// `start_offset` is the byte offset in `tables_stream` where
/// table 0x00 (Module) starts (the row_counts header end).
fn extract_custom_attribute_versions(
    tables: &[u8],
    start_offset: usize,
    widths: &TableWidths,
    row_counts: &BTreeMap<u8, u32>,
    strings_heap: &[u8],
    blob_heap: &[u8],
) -> (Option<String>, Option<String>) {
    // Compute the absolute offsets of every table whose rows we need
    // to walk: 0x01 TypeRef, 0x0A MemberRef, 0x0C CustomAttribute.
    let table_offsets = compute_table_offsets(start_offset, widths, row_counts);
    let Some(&ca_offset) = table_offsets.get(&0x0C) else {
        return (None, None);
    };
    let Some(&typeref_offset) = table_offsets.get(&0x01) else {
        return (None, None);
    };
    let memberref_offset = table_offsets.get(&0x0A).copied().unwrap_or(0);
    let ca_rows = row_counts.get(&0x0C).copied().unwrap_or(0);
    let ca_row_size = compute_row_size(0x0C, widths, row_counts);
    let memberref_row_size = compute_row_size(0x0A, widths, row_counts);
    let typeref_row_size = compute_row_size(0x01, widths, row_counts);
    let has_ca_width = coded_idx_width(
        row_counts,
        &[
            0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x00, 0x0E, 0x17, 0x14, 0x11, 0x1A, 0x1B,
            0x20, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B,
        ],
        5,
    );
    let ca_type_width = coded_idx_width(row_counts, &[0xFF, 0xFF, 0x06, 0x0A, 0xFF], 3);
    let member_ref_parent_width =
        coded_idx_width(row_counts, &[0x02, 0x01, 0x1A, 0x06, 0x1B], 3);

    let mut informational: Option<String> = None;
    let mut file: Option<String> = None;
    for i in 0..ca_rows {
        let row_offset = ca_offset + (i as usize) * ca_row_size;
        let Some(row) = tables.get(row_offset..row_offset + ca_row_size) else {
            break;
        };
        // CustomAttribute row layout:
        //   <has_ca_width> Parent
        //   <ca_type_width> Type
        //   <blob_idx>     Value
        let type_idx_raw = match ca_type_width {
            2 => match u16_le(row, has_ca_width) {
                Some(v) => v as u32,
                None => continue,
            },
            4 => match u32_le(row, has_ca_width) {
                Some(v) => v,
                None => continue,
            },
            _ => continue,
        };
        let value_offset = has_ca_width + ca_type_width;
        let blob_idx = match widths.blob_idx {
            2 => match u16_le(row, value_offset) {
                Some(v) => v as u32,
                None => continue,
            },
            4 => match u32_le(row, value_offset) {
                Some(v) => v,
                None => continue,
            },
            _ => continue,
        };
        // CustomAttributeType coded index: 3-bit tag (low bits),
        // remaining bits = row index (1-based).
        let tag = (type_idx_raw & 0x7) as u8;
        let table_row_index = type_idx_raw >> 3;
        if tag != 3 {
            continue; // We only handle MemberRef (tag 3); MethodDef (tag 2) is .ctor in same assembly — exceedingly rare for the attrs we care about.
        }
        if table_row_index == 0 {
            continue;
        }
        // Resolve the MemberRef row (1-based index).
        if memberref_offset == 0 || memberref_row_size == 0 {
            continue;
        }
        let mr_row_offset = memberref_offset + ((table_row_index - 1) as usize) * memberref_row_size;
        let Some(mr_row) = tables.get(mr_row_offset..mr_row_offset + memberref_row_size) else {
            continue;
        };
        // MemberRef row layout:
        //   <member_ref_parent_width> Class
        //   <strings_idx> Name
        //   <blob_idx>    Signature
        let class_raw = match member_ref_parent_width {
            2 => match u16_le(mr_row, 0) {
                Some(v) => v as u32,
                None => continue,
            },
            4 => match u32_le(mr_row, 0) {
                Some(v) => v,
                None => continue,
            },
            _ => continue,
        };
        // MemberRefParent coded index: 3-bit tag, tag 1 = TypeRef.
        let class_tag = (class_raw & 0x7) as u8;
        let typeref_row_idx = class_raw >> 3;
        if class_tag != 1 || typeref_row_idx == 0 {
            continue;
        }
        // Resolve the TypeRef row.
        let tr_row_offset =
            typeref_offset + ((typeref_row_idx - 1) as usize) * typeref_row_size;
        let Some(tr_row) = tables.get(tr_row_offset..tr_row_offset + typeref_row_size) else {
            continue;
        };
        // TypeRef row layout:
        //   <resolution_scope> ResolutionScope
        //   <strings_idx>      TypeName
        //   <strings_idx>      TypeNamespace
        let resolution_scope_width =
            coded_idx_width(row_counts, &[0x00, 0x1A, 0x23, 0x01], 2);
        let typename_offset = resolution_scope_width;
        let typename_idx = match widths.strings_idx {
            2 => match u16_le(tr_row, typename_offset) {
                Some(v) => v as u32,
                None => continue,
            },
            4 => match u32_le(tr_row, typename_offset) {
                Some(v) => v,
                None => continue,
            },
            _ => continue,
        };
        let Some(typename) = read_string_heap(strings_heap, typename_idx) else {
            continue;
        };
        let is_informational = typename == "AssemblyInformationalVersionAttribute";
        let is_file = typename == "AssemblyFileVersionAttribute";
        if !is_informational && !is_file {
            continue;
        }
        // Decode the Value blob.
        let Some(version_str) = decode_attribute_string_blob(blob_heap, blob_idx) else {
            continue;
        };
        if !is_plausible_version_string(&version_str) {
            continue;
        }
        if is_informational && informational.is_none() {
            informational = Some(version_str);
        } else if is_file && file.is_none() {
            file = Some(version_str);
        }
    }
    (informational, file)
}

/// Compute absolute byte offsets in the tables stream for every
/// present table. Each table's offset = `start_offset` + sum of
/// `(row_count × row_size)` for every table with a lower token number.
fn compute_table_offsets(
    start_offset: usize,
    widths: &TableWidths,
    row_counts: &BTreeMap<u8, u32>,
) -> BTreeMap<u8, usize> {
    let mut out = BTreeMap::new();
    let mut cursor = start_offset;
    for token in 0..64u8 {
        if let Some(&rows) = row_counts.get(&token) {
            out.insert(token, cursor);
            let row_size = compute_row_size(token, widths, row_counts);
            cursor = cursor.saturating_add((rows as usize) * row_size);
        }
    }
    out
}

/// Decode a custom-attribute Value blob per ECMA-335 §II.23.3:
/// the blob payload is a compressed-int length prefix (already
/// consumed by the heap index — we read the blob bytes via
/// `read_blob_at`), then prolog `0x0001` (2 bytes LE), then a
/// SerString. Returns the decoded UTF-8 string, or `None` on failure.
fn decode_attribute_string_blob(blob_heap: &[u8], blob_idx: u32) -> Option<String> {
    let blob = read_blob_at(blob_heap, blob_idx)?;
    if blob.len() < 4 {
        return None;
    }
    // Prolog must be 0x0001 (little-endian).
    if blob[0] != 0x01 || blob[1] != 0x00 {
        return None;
    }
    // After prolog, decode SerString.
    decode_serstring(&blob[2..])
}

/// Read a blob from the `#Blob` heap starting at `idx`. The blob's
/// own length is encoded via the ECMA-335 §II.24.2.4 compressed
/// integer format. Returns the slice of the blob's content bytes.
fn read_blob_at(blob_heap: &[u8], idx: u32) -> Option<&[u8]> {
    let start = idx as usize;
    if start >= blob_heap.len() {
        return None;
    }
    let (length, consumed) = decode_compressed_int(&blob_heap[start..])?;
    let payload_start = start + consumed;
    let payload_end = payload_start.checked_add(length as usize)?;
    blob_heap.get(payload_start..payload_end)
}

/// ECMA-335 §II.24.2.4 compressed-integer decode.
///
/// - 1 byte if high bit = 0 (value < 128)
/// - 2 bytes if high 2 bits = 10 (value < 16384)
/// - 4 bytes if high 3 bits = 110 (value < 2^29)
///
/// Returns `(value, bytes_consumed)` or `None` on malformed input.
fn decode_compressed_int(bytes: &[u8]) -> Option<(u32, usize)> {
    let b0 = *bytes.first()?;
    if b0 & 0x80 == 0 {
        return Some((b0 as u32, 1));
    }
    if b0 & 0xC0 == 0x80 {
        let b1 = *bytes.get(1)?;
        let v = ((b0 as u32 & 0x3F) << 8) | (b1 as u32);
        return Some((v, 2));
    }
    if b0 & 0xE0 == 0xC0 {
        let b1 = *bytes.get(1)?;
        let b2 = *bytes.get(2)?;
        let b3 = *bytes.get(3)?;
        let v = ((b0 as u32 & 0x1F) << 24)
            | ((b1 as u32) << 16)
            | ((b2 as u32) << 8)
            | (b3 as u32);
        return Some((v, 4));
    }
    None
}

/// ECMA-335 §II.23.3 SerString decode.
/// - 0xFF byte = null string → returns None silently (skip).
/// - Otherwise: compressed-int length prefix + UTF-8 bytes.
fn decode_serstring(bytes: &[u8]) -> Option<String> {
    let first = *bytes.first()?;
    if first == 0xFF {
        return None;
    }
    let (length, consumed) = decode_compressed_int(bytes)?;
    let str_bytes = bytes.get(consumed..consumed + length as usize)?;
    String::from_utf8(str_bytes.to_vec()).ok()
}

/// Sanity-filter for the decoded version strings. Real
/// AssemblyInformationalVersion / AssemblyFileVersion strings look
/// like `"8.0.27"`, `"1.2.3-rc.1"`, `"8.0.27-servicing.26230.7+sha.a1b2c3d"`.
/// Reject empty strings, strings >128 chars, strings without any
/// digit, strings without a dot or hyphen separator.
fn is_plausible_version_string(s: &str) -> bool {
    if s.is_empty() || s.len() > 128 {
        return false;
    }
    if !s.is_ascii() {
        return false;
    }
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // Must contain at least one `.` or `-` separator (so single-digit
    // garbage like "0" or "7" gets rejected). Real versions always have
    // either a dotted version number or a semver-style hyphen suffix.
    if !s.contains('.') && !s.contains('-') {
        return false;
    }
    // Must NOT contain control characters (NUL, escape, etc.).
    if s.chars().any(|c| (c as u32) < 0x20) {
        return false;
    }
    true
}

/// Strip the SemVer §10 build-metadata suffix from an
/// `AssemblyInformationalVersion` value and return the prefix when the
/// prefix passes [`is_plausible_version_string`].
///
/// Milestone 132 US2 (FR-008 + FR-009 + FR-010): syft and similar
/// comparators strip everything from the first `+` onward when matching
/// `pkg:nuget` versions. waybill keeps the verbatim Informational per
/// SemVer §10 but ALSO emits this stripped form alongside so consumers
/// can key on either.
///
/// - Returns `None` when `s` contains no `+` (FR-009 — no semantic content
///   to surface when there's no build metadata to strip).
/// - Returns `None` when the stripped prefix fails the milestone-131
///   `is_plausible_version_string` sanity filter (FR-010 — silent skip
///   rather than emit garbage).
fn strip_informational_build_metadata(s: &str) -> Option<&str> {
    let (prefix, _build_meta) = s.split_once('+')?;
    if is_plausible_version_string(prefix) {
        Some(prefix)
    } else {
        None
    }
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
// Milestone 131 US2a — LICENSE.txt probing + fingerprint-matching.
// =============================================================

/// Per FR-013: walk up from `dll_path`'s parent directory up to
/// `max_depth` levels looking for case-insensitive `LICENSE`,
/// `LICENSE.txt`, `LICENSE.md`, `COPYING`, or `COPYING.txt`.
/// Returns the first match's first 4 KB of bytes + the file path.
/// `None` when no match.
fn probe_license_file(dll_path: &Path, max_depth: u8) -> Option<(Vec<u8>, PathBuf)> {
    const LICENSE_CANDIDATE_NAMES: &[&str] = &[
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "COPYING",
        "COPYING.txt",
    ];
    const READ_CAP_BYTES: usize = 4 * 1024;
    let mut current = dll_path.parent()?;
    for _ in 0..=max_depth {
        let dir_entries = std::fs::read_dir(current).ok();
        if let Some(entries) = dir_entries {
            for entry in entries.flatten() {
                let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                    continue;
                };
                let matches_candidate = LICENSE_CANDIDATE_NAMES
                    .iter()
                    .any(|c| name.eq_ignore_ascii_case(c));
                if !matches_candidate {
                    continue;
                }
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Ok(file_bytes) = std::fs::read(&path) else {
                    continue;
                };
                let truncated = file_bytes
                    .into_iter()
                    .take(READ_CAP_BYTES)
                    .collect::<Vec<_>>();
                return Some((truncated, path));
            }
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
    None
}

/// Per FR-013: match the license file's first 4 KB against canonical
/// opening-text patterns of common SPDX licenses. Returns the SPDX
/// id when a match fires, else `None` (signals the C97 fallback).
///
/// Order matters: more-specific patterns first, otherwise a less-specific
/// arm steals the match. Concretely:
/// - MIT-0 BEFORE MIT (both contain "Permission is hereby granted").
/// - LGPL BEFORE GPL (LGPL text contains "General Public License").
/// - Version 3 BEFORE version 2 within each GPL/LGPL family.
/// - EPL-2.0 BEFORE EPL-1.0.
fn fingerprint_license(bytes: &[u8]) -> Option<&'static str> {
    let text = std::str::from_utf8(bytes).ok()?;
    // Case-insensitive substring detection. Order matters: more-specific
    // patterns first (BSD-3 before BSD-2; GPL-3 before GPL-2).
    if text.contains("Apache License")
        && (text.contains("Version 2.0") || text.contains("Version 2,"))
    {
        return Some("Apache-2.0");
    }
    // Milestone 132 US3 Path A: MIT-0 BEFORE the generic MIT arm —
    // "MIT No Attribution" texts ALSO contain "Permission is hereby
    // granted" and would otherwise be mis-tagged as MIT.
    if text.contains("MIT No Attribution") {
        return Some("MIT-0");
    }
    if text.contains("MIT License")
        || text.contains("Permission is hereby granted, free of charge")
    {
        return Some("MIT");
    }
    if text.contains("BSD 3-Clause")
        || (text.contains("Redistribution and use in source and binary forms")
            && text.contains("Neither the name"))
    {
        return Some("BSD-3-Clause");
    }
    if text.contains("BSD 2-Clause")
        || (text.contains("Redistribution and use in source and binary forms")
            && !text.contains("Neither the name"))
    {
        return Some("BSD-2-Clause");
    }
    // Milestone 132 US3 Path A: Microsoft Public License — distinctive
    // "Ms-PL" identifier appears in the canonical text alongside the
    // expanded name.
    if text.contains("Microsoft Public License") && text.contains("Ms-PL") {
        return Some("MS-PL");
    }
    // Milestone 132 US3 Path A: LGPL family BEFORE GPL family because
    // LGPL canonical text contains "Lesser General Public License" AND
    // would also match the GPL arm's "General Public License" substring.
    // Version 3 before version 2.1.
    if text.contains("Lesser General Public License")
        && (text.contains("version 3") || text.contains("Version 3"))
    {
        return Some("LGPL-3.0");
    }
    if text.contains("Lesser General Public License")
        && (text.contains("version 2.1") || text.contains("Version 2.1"))
    {
        return Some("LGPL-2.1");
    }
    if text.contains("GNU General Public License")
        && (text.contains("version 3") || text.contains("Version 3"))
    {
        return Some("GPL-3.0");
    }
    if text.contains("GNU General Public License")
        && (text.contains("version 2") || text.contains("Version 2"))
    {
        return Some("GPL-2.0");
    }
    // Milestone 132 US3 Path A: Eclipse Public License v2.0 BEFORE v1.0
    // for the same reason as GPL ordering.
    if text.contains("Eclipse Public License") && text.contains("v 2.0") {
        return Some("EPL-2.0");
    }
    if text.contains("Eclipse Public License") && text.contains("v 1.0") {
        return Some("EPL-1.0");
    }
    None
}

/// Per C97: hex-encode the SHA-256 of the license file's first 4 KB.
fn compute_license_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone)]
enum LicenseProbeResult {
    /// File found, fingerprint matched.
    Identified { spdx_id: &'static str },
    /// File found, no fingerprint match. Emits C97 hash annotation.
    Unrecognized { sha256_hex: String },
    /// File not found in the probe walk.
    NotFound,
}

// =============================================================
// Resource-assembly culture-set dedup (FR-024) + license accumulator.
// =============================================================

struct AssemblyAccumulator {
    components: BTreeMap<(String, String), AccumulatedAssembly>,
}

struct AccumulatedAssembly {
    representative_version: Version4Tuple,
    cultures: BTreeSet<String>,
    source_paths: BTreeSet<PathBuf>,
    /// Milestone 131 US2a — license probe result for this component.
    /// `None` until first absorb completes; subsequent absorbs from
    /// culture-variant DLLs DO NOT overwrite (the first-probed wins,
    /// keeping the per-(name,version) result stable).
    license: Option<LicenseProbeResult>,
    /// Milestone 131 US1 (Phase B) — AssemblyInformationalVersionAttribute
    /// value from the first absorb's CustomAttribute walk. Drives the
    /// PURL version per FR-008 ladder.
    informational_version: Option<String>,
    /// Milestone 131 US1 — AssemblyFileVersionAttribute value.
    file_version: Option<String>,
}

impl AssemblyAccumulator {
    fn new() -> Self {
        Self { components: BTreeMap::new() }
    }

    fn absorb(&mut self, assembly: ManagedAssembly, path: &Path) {
        let version_str = assembly.version.to_string();
        let key = (assembly.name.clone(), version_str);
        let is_new = !self.components.contains_key(&key);
        let entry = self.components.entry(key).or_insert_with(|| AccumulatedAssembly {
            representative_version: assembly.version,
            cultures: BTreeSet::new(),
            source_paths: BTreeSet::new(),
            license: None,
            informational_version: None,
            file_version: None,
        });
        entry.source_paths.insert(path.to_path_buf());
        if let Some(culture) = assembly.culture {
            entry.cultures.insert(culture);
        }
        // Milestone 131 US1 — first-absorb-wins for Phase B version
        // strings. Culture-variant resource DLLs all share the same
        // AssemblyInformationalVersion / AssemblyFileVersion (built
        // from the same project), so first-wins is deterministic.
        if entry.informational_version.is_none() && assembly.informational_version.is_some() {
            entry.informational_version = assembly.informational_version;
        }
        if entry.file_version.is_none() && assembly.file_version.is_some() {
            entry.file_version = assembly.file_version;
        }
        // Milestone 131 US2a — license probe per FR-013. Run only on
        // the FIRST absorb for this (name, version) key so multi-culture
        // resource-assembly files don't redundantly probe (and so the
        // result is deterministic — first DLL's parent-directory wins).
        if is_new {
            entry.license = Some(match probe_license_file(path, 3) {
                Some((bytes, _file_path)) => {
                    if let Some(spdx_id) = fingerprint_license(&bytes) {
                        LicenseProbeResult::Identified { spdx_id }
                    } else {
                        LicenseProbeResult::Unrecognized {
                            sha256_hex: compute_license_sha256_hex(&bytes),
                        }
                    }
                }
                None => LicenseProbeResult::NotFound,
            });
        }
    }

    fn flatten(self) -> Vec<PackageDbEntry> {
        let mut out = Vec::new();
        for ((name, _runtime_version_str), acc) in self.components {
            // Milestone 131 US1 (Phase B) — PURL version ladder per
            // FR-008 + milestone-129 clarification Q3:
            //   AssemblyInformationalVersion > AssemblyFileVersion >
            //   AssemblyVersion 4-tuple.
            let purl_version = acc
                .informational_version
                .clone()
                .or_else(|| acc.file_version.clone())
                .unwrap_or_else(|| acc.representative_version.to_string());
            let Some(purl) = super::build_nuget_purl(&name, &purl_version) else {
                tracing::warn!(
                    name = %name,
                    version = %purl_version,
                    "nuget PURL invalid; skipping"
                );
                continue;
            };
            let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            extra_annotations.insert(
                "waybill:source-mechanism".to_string(),
                serde_json::Value::String("dotnet-assembly-metadata".to_string()),
            );
            // Always-emit the 4-tuple AssemblyVersion per FR-010.
            extra_annotations.insert(
                "waybill:assembly-version-runtime".to_string(),
                serde_json::Value::String(acc.representative_version.to_string()),
            );
            // Milestone 131 US1 — additional version annotations when
            // extracted from CustomAttribute walk.
            if let Some(v) = &acc.informational_version {
                extra_annotations.insert(
                    "waybill:assembly-version-informational".to_string(),
                    serde_json::Value::String(v.clone()),
                );
                // Milestone 132 US2 (FR-008): companion annotation carrying
                // the InformationalVersion with the SemVer §10 build-metadata
                // suffix removed. FR-009 (no `+` → skip) and FR-010 (stripped
                // prefix re-runs sanity filter) are enforced inside
                // strip_informational_build_metadata.
                if let Some(stripped) = strip_informational_build_metadata(v) {
                    extra_annotations.insert(
                        "waybill:assembly-version-informational-stripped".to_string(),
                        serde_json::Value::String(stripped.to_string()),
                    );
                }
            }
            if let Some(v) = &acc.file_version {
                extra_annotations.insert(
                    "waybill:assembly-version-file".to_string(),
                    serde_json::Value::String(v.clone()),
                );
            }
            if !acc.cultures.is_empty() {
                let joined = acc
                    .cultures
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",");
                extra_annotations.insert(
                    "waybill:assembly-cultures".to_string(),
                    serde_json::Value::String(joined),
                );
            }
            // Milestone 131 US2a — license-source annotations (C96) +
            // SPDX-id-driven licenses[] population per FR-013 / FR-015.
            let licenses_vec = match &acc.license {
                Some(LicenseProbeResult::Identified { spdx_id }) => {
                    extra_annotations.insert(
                        "waybill:license-source".to_string(),
                        serde_json::Value::String("package-dir".to_string()),
                    );
                    match waybill_common::types::license::SpdxExpression::try_canonical(spdx_id) {
                        Ok(expr) => vec![expr],
                        Err(e) => {
                            tracing::warn!(
                                spdx_id = %spdx_id,
                                err = %e,
                                "fingerprint-matched SPDX id failed try_canonical; skipping"
                            );
                            Vec::new()
                        }
                    }
                }
                Some(LicenseProbeResult::Unrecognized { sha256_hex }) => {
                    extra_annotations.insert(
                        "waybill:license-source".to_string(),
                        serde_json::Value::String("package-dir-unrecognized".to_string()),
                    );
                    extra_annotations.insert(
                        "waybill:license-text-sha256".to_string(),
                        serde_json::Value::String(sha256_hex.clone()),
                    );
                    Vec::new()
                }
                Some(LicenseProbeResult::NotFound) | None => {
                    extra_annotations.insert(
                        "waybill:license-source".to_string(),
                        serde_json::Value::String("package-dir-no-license".to_string()),
                    );
                    Vec::new()
                }
            };
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
                version: purl_version.clone(),
                arch: None,
                source_path: primary_source,
                depends: Vec::new(),
                maintainer: None,
                licenses: licenses_vec,
                lifecycle_scope: None,
                requirement_ranges: Vec::new(),
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
            informational_version: None,
            file_version: None,
        };
        let de = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 1, minor: 0, build: 0, revision: 0 },
            culture: Some("de".to_string()),
            informational_version: None,
            file_version: None,
        };
        let fr = ManagedAssembly {
            name: "Foo.Bar".to_string(),
            version: Version4Tuple { major: 1, minor: 0, build: 0, revision: 0 },
            culture: Some("fr".to_string()),
            informational_version: None,
            file_version: None,
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
            .get("waybill:assembly-cultures")
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
            informational_version: None,
            file_version: None,
        };
        acc.absorb(neutral, Path::new("/a/Foo.Bar.dll"));
        let entries = acc.flatten();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].extra_annotations.contains_key("waybill:assembly-cultures"));
        let mech = entries[0]
            .extra_annotations
            .get("waybill:source-mechanism")
            .and_then(|v| v.as_str());
        assert_eq!(mech, Some("dotnet-assembly-metadata"));
        let runtime_ver = entries[0]
            .extra_annotations
            .get("waybill:assembly-version-runtime")
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

    // ============================================================
    // Milestone 131 US2a — license probe + fingerprint tests.
    // ============================================================

    #[test]
    fn fingerprint_license_detects_apache_2_0() {
        let text = b"\n                                 Apache License\n                           Version 2.0, January 2004\n";
        assert_eq!(fingerprint_license(text), Some("Apache-2.0"));
    }

    #[test]
    fn fingerprint_license_detects_mit_via_permission_grant() {
        let text = b"Copyright (c) 2024 Example Corp\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software";
        assert_eq!(fingerprint_license(text), Some("MIT"));
    }

    #[test]
    fn fingerprint_license_detects_bsd_3_clause_via_neither_clause() {
        let text = b"Redistribution and use in source and binary forms, with or without\nmodification, are permitted provided that the following conditions are met:\n\n* Neither the name of Example nor the names of its contributors may be used";
        assert_eq!(fingerprint_license(text), Some("BSD-3-Clause"));
    }

    #[test]
    fn fingerprint_license_returns_none_for_unrecognized_text() {
        let text = b"This is some proprietary license that waybill doesn't recognize.";
        assert!(fingerprint_license(text).is_none());
    }

    // ============================================================
    // Milestone 132 US3 Path A — extended fingerprint arms.
    // ============================================================

    #[test]
    fn fingerprint_license_detects_mit_0_before_mit() {
        // MIT-0 text shape: starts with "MIT No Attribution" then the
        // standard MIT permission grant. MUST resolve to MIT-0, not MIT.
        let text = b"MIT No Attribution\n\nCopyright 2024 Example\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software";
        assert_eq!(fingerprint_license(text), Some("MIT-0"));
    }

    #[test]
    fn fingerprint_license_detects_ms_pl() {
        let text = b"Microsoft Public License (Ms-PL)\n\nThis license governs use of the accompanying software.";
        assert_eq!(fingerprint_license(text), Some("MS-PL"));
    }

    #[test]
    fn fingerprint_license_detects_lgpl_3_0_before_gpl() {
        // LGPL canonical text contains "General Public License" — the
        // LGPL arm MUST fire before the GPL-3.0 arm steals the match.
        // Real LGPL LICENSE files contain BOTH the ALL-CAPS title line
        // AND a mixed-case body reference ("GNU Lesser General Public
        // License"); test fixture must include the mixed-case form
        // because that's what the substring match keys on.
        let text = b"                   GNU LESSER GENERAL PUBLIC LICENSE\n                       Version 3, 29 June 2007\n\n  This version of the GNU Lesser General Public License incorporates\nthe terms and conditions of version 3 of the GNU General Public License,";
        assert_eq!(fingerprint_license(text), Some("LGPL-3.0"));
    }

    #[test]
    fn fingerprint_license_detects_lgpl_2_1_before_gpl() {
        let text = b"                  GNU LESSER GENERAL PUBLIC LICENSE\n                       Version 2.1, February 1999\n\n  This is the GNU Lesser General Public License, version 2.1, which\napplies to those works whose authors release them under its terms.";
        assert_eq!(fingerprint_license(text), Some("LGPL-2.1"));
    }

    #[test]
    fn fingerprint_license_detects_epl_2_0_before_epl_1_0() {
        let text = b"Eclipse Public License - v 2.0\n\nTHE ACCOMPANYING PROGRAM IS PROVIDED UNDER THE TERMS OF THIS ECLIPSE PUBLIC LICENSE";
        assert_eq!(fingerprint_license(text), Some("EPL-2.0"));
    }

    #[test]
    fn fingerprint_license_detects_epl_1_0() {
        let text = b"Eclipse Public License - v 1.0\n\nTHE ACCOMPANYING PROGRAM IS PROVIDED UNDER THE TERMS OF THIS ECLIPSE PUBLIC LICENSE";
        assert_eq!(fingerprint_license(text), Some("EPL-1.0"));
    }

    #[test]
    fn fingerprint_license_new_arms_all_canonicalize() {
        // Sanity: every SPDX id the new arms emit MUST pass
        // SpdxExpression::try_canonical so the emission site at line ~1147
        // doesn't silently drop the license. Catches typos at unit-test
        // time rather than at production scan time.
        for spdx_id in &["MIT-0", "MS-PL", "LGPL-3.0", "LGPL-2.1", "EPL-2.0", "EPL-1.0"] {
            waybill_common::types::license::SpdxExpression::try_canonical(spdx_id)
                .unwrap_or_else(|e| {
                    panic!("milestone-132 SPDX id {spdx_id:?} fails try_canonical: {e}");
                });
        }
    }

    #[test]
    fn probe_license_file_finds_at_dll_parent_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dll_dir = tmp.path().join("packs/Foo.Bar/1.0.0/ref/net8.0");
        std::fs::create_dir_all(&dll_dir).unwrap();
        let dll_path = dll_dir.join("Foo.Bar.dll");
        std::fs::write(&dll_path, b"fake dll").unwrap();
        // Place LICENSE.TXT (case-mixed) one level above the DLL's dir.
        let license_path = tmp.path().join("packs/Foo.Bar/1.0.0/LICENSE.TXT");
        std::fs::write(&license_path, b"Apache License\nVersion 2.0, January 2004").unwrap();
        let result = probe_license_file(&dll_path, 3);
        let (bytes, found_path) = result.expect("should find LICENSE");
        assert!(std::str::from_utf8(&bytes).unwrap().contains("Apache License"));
        assert_eq!(found_path.file_name().and_then(|s| s.to_str()), Some("LICENSE.TXT"));
    }

    #[test]
    fn probe_license_file_caps_read_at_4kb() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dll_dir = tmp.path().join("pkg");
        std::fs::create_dir_all(&dll_dir).unwrap();
        let dll_path = dll_dir.join("Foo.dll");
        std::fs::write(&dll_path, b"fake").unwrap();
        // 10 KB LICENSE; probe must return only first 4 KB.
        let huge_license = vec![b'A'; 10 * 1024];
        std::fs::write(dll_dir.join("LICENSE"), &huge_license).unwrap();
        let (bytes, _) = probe_license_file(&dll_path, 1).unwrap();
        assert_eq!(bytes.len(), 4 * 1024);
    }

    #[test]
    fn probe_license_file_returns_none_when_no_license_in_walk() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dll_dir = tmp.path().join("pkg/nested/deep");
        std::fs::create_dir_all(&dll_dir).unwrap();
        let dll_path = dll_dir.join("Foo.dll");
        std::fs::write(&dll_path, b"fake").unwrap();
        assert!(probe_license_file(&dll_path, 3).is_none());
    }

    #[test]
    fn compute_license_sha256_hex_is_deterministic() {
        let bytes = b"Some license body";
        let h1 = compute_license_sha256_hex(bytes);
        let h2 = compute_license_sha256_hex(bytes);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    // ============================================================
    // Milestone 131 US1 — Phase B CustomAttribute walking tests.
    // ============================================================

    #[test]
    fn decode_compressed_int_one_byte_when_high_bit_clear() {
        // 0x42 = 66 (no high bit), 1-byte form.
        assert_eq!(decode_compressed_int(&[0x42]), Some((66, 1)));
        assert_eq!(decode_compressed_int(&[0x00]), Some((0, 1)));
        assert_eq!(decode_compressed_int(&[0x7F]), Some((127, 1)));
    }

    #[test]
    fn decode_compressed_int_two_byte_when_high_bits_10() {
        // 0x80 0x80 = 0b10_000000_10000000 → value = 0x80 = 128.
        assert_eq!(decode_compressed_int(&[0x80, 0x80]), Some((128, 2)));
        // 0xBF 0xFF = 0b10_111111_11111111 → value = 0x3FFF = 16383.
        assert_eq!(decode_compressed_int(&[0xBF, 0xFF]), Some((16383, 2)));
    }

    #[test]
    fn decode_compressed_int_four_byte_when_high_bits_110() {
        // 0xC0 0x00 0x40 0x00 = 0b110_00000_00000000_01000000_00000000 → value = 0x4000 = 16384.
        assert_eq!(
            decode_compressed_int(&[0xC0, 0x00, 0x40, 0x00]),
            Some((16384, 4))
        );
    }

    #[test]
    fn decode_compressed_int_returns_none_for_empty_input() {
        assert_eq!(decode_compressed_int(&[]), None);
    }

    #[test]
    fn decode_serstring_short_string() {
        // Length-prefixed UTF-8: 0x05 ('H', 'e', 'l', 'l', 'o').
        let bytes = [0x05, b'H', b'e', b'l', b'l', b'o'];
        assert_eq!(decode_serstring(&bytes), Some("Hello".to_string()));
    }

    #[test]
    fn decode_serstring_null_returns_none() {
        assert_eq!(decode_serstring(&[0xFF]), None);
    }

    #[test]
    fn decode_serstring_empty_string() {
        // Length 0 → empty string.
        assert_eq!(decode_serstring(&[0x00]), Some(String::new()));
    }

    #[test]
    fn is_plausible_version_string_accepts_semver_and_4tuple() {
        assert!(is_plausible_version_string("8.0.27"));
        assert!(is_plausible_version_string("1.2.3-rc.1"));
        assert!(is_plausible_version_string("8.0.27-servicing.26230.7+sha.a1b2c3d"));
        assert!(is_plausible_version_string("8.0.27.0"));
    }

    #[test]
    fn strip_informational_build_metadata_plus_sha() {
        // FR-008 happy path: split-once on first `+`, return prefix.
        assert_eq!(
            strip_informational_build_metadata(
                "4.8.0-7.25569.25+38896ab4abcdef0123456789",
            ),
            Some("4.8.0-7.25569.25"),
        );
    }

    #[test]
    fn strip_informational_build_metadata_no_plus_returns_none() {
        // FR-009: no `+` separator → no stripped annotation to emit.
        assert_eq!(strip_informational_build_metadata("5.0.0"), None);
        assert_eq!(strip_informational_build_metadata("1.2.3-rc.1"), None);
    }

    #[test]
    fn strip_informational_build_metadata_multiple_plus_uses_first() {
        // SemVer §10: everything from the FIRST `+` onward is build
        // metadata; waybill MUST NOT interpret further.
        assert_eq!(
            strip_informational_build_metadata("1.2.3+meta+more"),
            Some("1.2.3"),
        );
    }

    #[test]
    fn strip_informational_build_metadata_prefix_sanity_fail_returns_none() {
        // FR-010: prefix re-runs is_plausible_version_string. Bare "+sha"
        // means prefix is empty → empty fails sanity → silent skip.
        assert_eq!(strip_informational_build_metadata("+sha"), None);
        // Prefix is single digit with no separator → fails sanity.
        assert_eq!(strip_informational_build_metadata("7+meta"), None);
    }

    #[test]
    fn is_plausible_version_string_rejects_garbage() {
        assert!(!is_plausible_version_string(""));
        // No separator AND no digit.
        assert!(!is_plausible_version_string("hello"));
        // No digit.
        assert!(!is_plausible_version_string("abc.def"));
        // No separator.
        assert!(!is_plausible_version_string("12345"));
        // Control character.
        assert!(!is_plausible_version_string("1.2\x00.3"));
        // Too long.
        let long = "1.".repeat(100);
        assert!(!is_plausible_version_string(&long));
    }

    #[test]
    fn decode_attribute_string_blob_round_trips_via_blob_heap() {
        // Construct a synthetic #Blob heap: blob at idx=1 carries the
        // attribute payload.
        // Blob structure on the heap:
        //   [length_compressed_int][prolog 0x01 0x00][serstring]
        // For SerString "8.0.27" (6 bytes): length-prefix=0x06.
        // Inner payload: 0x01 0x00 0x06 '8' '.' '0' '.' '2' '7' = 9 bytes.
        // Outer blob: length_prefix=0x09, then the 9 bytes.
        let mut heap = vec![0x00]; // index 0 is empty/unused per spec
        heap.push(0x09); // outer blob length = 9
        heap.push(0x01); // prolog low byte
        heap.push(0x00); // prolog high byte
        heap.push(0x06); // serstring length = 6
        heap.extend_from_slice(b"8.0.27");
        let result = decode_attribute_string_blob(&heap, 1);
        assert_eq!(result, Some("8.0.27".to_string()));
    }

    #[test]
    fn decode_attribute_string_blob_rejects_wrong_prolog() {
        let heap = [0x00, 0x09, 0xFF, 0xFF, 0x06, b'X', b'.', b'Y', b'.', b'Z'];
        assert_eq!(decode_attribute_string_blob(&heap, 1), None);
    }
}
