//! Milestone 104 — binary role classifier.
//!
//! Wraps `object::Object::kind()` and maps each `ObjectKind` variant
//! to the matching `waybill_common::resolution::BinaryRole` value
//! per the cross-format mapping at
//! `specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md`.
//!
//! ELF disambiguation: `object::Object::kind()` blindly maps
//! `ET_DYN` → `ObjectKind::Dynamic` for both PIE executables and
//! shared libraries (the upstream crate has a literal
//! `// TODO: check for DF_1_PIE?` comment at this line). US3 adds a
//! `PT_INTERP` program-header check to disambiguate; US1's classifier
//! does NOT — every `ET_DYN` ELF comes through as `SharedLibrary`
//! until US3 ships.

use waybill_common::resolution::BinaryRole;
use object::read::{File, Object, ObjectKind};

/// Classify a parsed binary file into a `BinaryRole`.
///
/// Mach-O `MH_EXECUTE` → Application; `MH_DYLIB` → SharedLibrary;
/// `MH_OBJECT` → Object; `MH_BUNDLE` and other variants → Other.
/// ELF `ET_EXEC` → Application; `ET_DYN` → Application if
/// `PT_INTERP` is present (PIE executables — the modern Linux
/// default), else SharedLibrary; `ET_REL` → Object; `ET_CORE` →
/// Other.
/// PE without `IMAGE_FILE_DLL` → Application; with `IMAGE_FILE_DLL`
/// → SharedLibrary; with `IMAGE_FILE_SYSTEM` → Other.
///
/// Universal/fat Mach-O binaries: `object::read::File::parse`
/// already returns the first slice (per FR-006), so this function
/// classifies based on the first slice's filetype without needing
/// to know it's a fat binary.
///
/// For ambiguous classifications (ELF `ET_DYN` disambiguation,
/// Mach-O bundles via `ObjectKind::Unknown`), a `tracing::info!`
/// audit log is emitted per FR-004 so operators investigating
/// unexpected role classifications have a trail.
pub fn classify(file: &File<'_>) -> BinaryRole {
    match file.kind() {
        ObjectKind::Executable => BinaryRole::Application,
        ObjectKind::Dynamic => {
            // Milestone 104 US3 — ET_DYN disambiguation for ELF.
            // `object::Object::kind()` maps both PIE executables AND
            // shared libraries to `Dynamic` (the upstream crate has
            // a literal `// TODO: check for DF_1_PIE?` comment).
            // PIE executables on every modern Linux distribution
            // carry a `PT_INTERP` segment (path to the dynamic
            // linker); shared libraries do not. This is the
            // empirically-strongest signal and matches the heuristic
            // used by GNU `file(1)` and `readelf -e`.
            //
            // Mach-O dylibs and PE DLLs also land here, but neither
            // has a PT_INTERP analog in `object::ObjectSegment` —
            // for those formats `has_interp` returns false and we
            // correctly emit SharedLibrary.
            if elf_has_interp(file) {
                tracing::info!(
                    role = "Application",
                    rule = "elf-et-dyn-with-pt-interp",
                    "ELF ET_DYN with PT_INTERP classified as Application (PIE executable)"
                );
                BinaryRole::Application
            } else {
                BinaryRole::SharedLibrary
            }
        }
        ObjectKind::Relocatable => BinaryRole::Object,
        ObjectKind::Core => BinaryRole::Other,
        ObjectKind::Unknown => {
            // Milestone 104 US3 — Mach-O MH_BUNDLE / MH_KEXT_BUNDLE
            // / etc. come through as Unknown (the `object` crate's
            // ObjectKind enum doesn't distinguish them). PE with
            // IMAGE_FILE_SYSTEM also lands here. The `Other` bucket
            // collapses these together; consumers wanting finer
            // detail read the existing `mikebom:binary-class`
            // annotation (which carries `elf`/`macho`/`pe`).
            tracing::info!(
                role = "Other",
                rule = "object-kind-unknown",
                format = ?file.format(),
                "binary classified as Other (format-specific fallback)"
            );
            BinaryRole::Other
        }
        _ => BinaryRole::Other,
    }
}

/// Returns true iff the file is ELF AND its program-header table
/// contains a `PT_INTERP` segment. The `object` crate's
/// `Object::segments()` iterator exposes `p_type` via the
/// `ObjectSegment` trait — we look for `elf::PT_INTERP` (3).
///
/// Non-ELF files (Mach-O / PE) return false: their `segments()`
/// iterators emit different segment kinds that never match
/// `PT_INTERP`, so the check is a safe no-op for them. We could
/// short-circuit on `file.format() != BinaryFormat::Elf` but the
/// per-segment check is faster than the format dispatch in practice
/// (small N).
fn elf_has_interp(file: &File<'_>) -> bool {
    if !matches!(file.format(), object::BinaryFormat::Elf) {
        return false;
    }
    // `ObjectSegment::flags()` exposes `SegmentFlags::Elf { p_flags
    // }` for ELF, but `p_type` isn't surfaced through the trait
    // directly — we have to downcast to the ELF-specific file type
    // to get at the program-header `p_type` field.
    use object::read::elf::{ElfFile, ProgramHeader};
    fn check<E: object::read::elf::FileHeader>(elf: &ElfFile<'_, E>) -> bool {
        let endian = elf.endian();
        let phdrs = match elf.elf_header().program_headers(endian, elf.data()) {
            Ok(ps) => ps,
            Err(_) => return false,
        };
        phdrs.iter().any(|p| p.p_type(endian) == object::elf::PT_INTERP)
    }
    match file {
        File::Elf32(elf) => check(elf),
        File::Elf64(elf) => check(elf),
        _ => false,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use object::read::File;

    // Each test builds a synthetic binary with raw bytes representing
    // the header form we want to test, then parses through the
    // `object` crate to assert the resulting `BinaryRole`. The
    // fixture bytes are hand-built rather than using the writer API
    // because `object::write` is oriented at relocatable-object
    // emission and doesn't trivially emit Mach-O `MH_EXECUTE` /
    // `MH_DYLIB` headers.

    /// Mach-O 64-bit little-endian header (the dominant on-disk
    /// shape for macOS x86_64 / arm64). 32-byte header. The
    /// returned bytes are NOT a complete executable — just enough
    /// for `object::read::File::parse` to identify the filetype.
    fn mk_macho64(filetype: u32) -> Vec<u8> {
        let mut out = Vec::new();
        // magic: MH_MAGIC_64 (0xfeedfacf) little-endian
        out.extend_from_slice(&0xfeedfacf_u32.to_le_bytes());
        // cputype: CPU_TYPE_X86_64 (0x01000007)
        out.extend_from_slice(&0x01000007_u32.to_le_bytes());
        // cpusubtype: 3 (CPU_SUBTYPE_X86_64_ALL)
        out.extend_from_slice(&3_u32.to_le_bytes());
        // filetype
        out.extend_from_slice(&filetype.to_le_bytes());
        // ncmds, sizeofcmds, flags, reserved — all zero
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out
    }

    #[test]
    fn macho_execute_is_application() {
        let bytes = mk_macho64(object::macho::MH_EXECUTE);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Application);
    }

    #[test]
    fn macho_dylib_is_shared_library() {
        let bytes = mk_macho64(object::macho::MH_DYLIB);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::SharedLibrary);
    }

    #[test]
    fn macho_object_is_object() {
        let bytes = mk_macho64(object::macho::MH_OBJECT);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Object);
    }

    #[test]
    fn macho_bundle_is_other() {
        let bytes = mk_macho64(object::macho::MH_BUNDLE);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Other);
    }

    /// Minimal ELF64 little-endian header. The `object` crate
    /// rejects parses below a certain size, so we pad to 64 bytes
    /// (the full ELF64 ehdr size).
    fn mk_elf64(e_type: u16) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        // e_ident[EI_MAG0..EI_MAG3] = "\x7fELF"
        out[0] = 0x7f;
        out[1] = b'E';
        out[2] = b'L';
        out[3] = b'F';
        // EI_CLASS = ELFCLASS64
        out[4] = 2;
        // EI_DATA = ELFDATA2LSB
        out[5] = 1;
        // EI_VERSION = EV_CURRENT
        out[6] = 1;
        // e_type at offset 16 (after the 16-byte e_ident)
        out[16] = (e_type & 0xff) as u8;
        out[17] = ((e_type >> 8) & 0xff) as u8;
        // e_machine = EM_X86_64 (62) at offset 18
        out[18] = 62;
        out[19] = 0;
        // e_version = EV_CURRENT
        out[20] = 1;
        // e_ehsize = 64 at offset 52
        out[52] = 64;
        // e_phentsize = 56 at offset 54 (ELF64 program-header size)
        out[54] = 56;
        // e_shentsize = 64 at offset 58 (ELF64 section-header size)
        out[58] = 64;
        out
    }

    #[test]
    fn elf_exec_is_application() {
        let bytes = mk_elf64(object::elf::ET_EXEC);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Application);
    }

    #[test]
    fn elf_rel_is_object() {
        let bytes = mk_elf64(object::elf::ET_REL);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Object);
    }

    #[test]
    fn elf_dyn_without_pt_interp_is_shared_library() {
        // ET_DYN ELF without a PT_INTERP segment → SharedLibrary
        // (the canonical shared-object case).
        let bytes = mk_elf64(object::elf::ET_DYN);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::SharedLibrary);
    }

    /// Build an ELF64 file with a PT_INTERP program-header segment.
    /// Layout: 64-byte ehdr → 56-byte PT_INTERP phdr at offset 64 →
    /// padding to 1024 bytes. The interp string is placed inside
    /// the phdr's `p_offset`/`p_filesz` region but content doesn't
    /// matter for classification — only the `p_type` byte counts.
    fn mk_elf64_dyn_with_interp() -> Vec<u8> {
        let mut out = vec![0u8; 1024];
        // ehdr (same as mk_elf64 for ET_DYN)
        out[0] = 0x7f; out[1] = b'E'; out[2] = b'L'; out[3] = b'F';
        out[4] = 2; out[5] = 1; out[6] = 1;
        out[16] = object::elf::ET_DYN as u8;
        out[17] = (object::elf::ET_DYN >> 8) as u8;
        out[18] = 62; // EM_X86_64
        out[20] = 1; // e_version
        out[52] = 64; // e_ehsize
        // e_phoff = 64 at offset 32 (u64 LE)
        out[32..40].copy_from_slice(&64_u64.to_le_bytes());
        out[54] = 56; // e_phentsize
        // e_phnum = 1 at offset 56
        out[56] = 1;
        out[58] = 64; // e_shentsize
        // ph[0]: PT_INTERP at offset 64
        let pt_interp = object::elf::PT_INTERP;
        out[64] = (pt_interp & 0xff) as u8;
        out[65] = ((pt_interp >> 8) & 0xff) as u8;
        out[66] = ((pt_interp >> 16) & 0xff) as u8;
        out[67] = ((pt_interp >> 24) & 0xff) as u8;
        // p_flags (4 bytes), p_offset (8 bytes), p_vaddr (8 bytes),
        // p_paddr (8 bytes), p_filesz (8 bytes), p_memsz (8 bytes),
        // p_align (8 bytes) — all zeroed is fine for our purposes
        out
    }

    #[test]
    fn elf_dyn_with_pt_interp_is_application_pie() {
        // ET_DYN ELF WITH a PT_INTERP segment → Application (PIE
        // executable). Modern Linux distros (Debian 11+, Ubuntu
        // 21+, Fedora 28+, etc.) ship most /bin/* and /usr/bin/*
        // as ET_DYN PIE binaries.
        let bytes = mk_elf64_dyn_with_interp();
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Application);
    }

    #[test]
    fn elf_core_is_other() {
        let bytes = mk_elf64(object::elf::ET_CORE);
        let file = File::parse(bytes.as_slice()).unwrap();
        assert_eq!(classify(&file), BinaryRole::Other);
    }

    // Fat-binary classification coverage (FR-006) lives in the
    // integration test `binary_role_disambiguation.rs` because the
    // fat-container parse happens in `scan_fs::binary::scan.rs`,
    // not in `classify` itself. The classifier function only ever
    // sees a thin Mach-O slice once `scan_fat_macho` has extracted
    // the first slice; testing fat-binary behavior at the unit
    // level here would test the wrong code path. The integration
    // test invokes the full scan pipeline against a fat-binary
    // fixture, which exercises both the slice extraction and the
    // classifier as a unit.
}
