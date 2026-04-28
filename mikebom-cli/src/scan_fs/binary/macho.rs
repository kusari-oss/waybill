//! Mach-O binary identity parsers — LC_UUID, LC_RPATH, and minimum-OS
//! version extraction (milestone 024).
//!
//! The companion file `scan.rs` already handles fat / universal slice
//! iteration (`scan_fat_macho`) + linkage extraction via the `object`
//! crate's high-level `imports()` API. This module fills the
//! identity-and-runtime-linkage gap that mikebom previously left at
//! defaults for Mach-O.
//!
//! Three signals (mirroring milestone 023's ELF identity work):
//!
//! - `LC_UUID` (cmd 0x1B): 16-byte binary identity, the Mach-O analog
//!   of ELF's NT_GNU_BUILD_ID. Used by dsymutil, the macOS crash
//!   reporter, xcrun symbolicatecrash, and every `*.dSYM` bundle.
//! - `LC_RPATH` (cmd 0x1C | LC_REQ_DYLD): runtime library search paths,
//!   the analog of ELF's DT_RPATH/DT_RUNPATH. `@executable_path`,
//!   `@loader_path`, `@rpath` recorded raw — substitution is
//!   runtime-context-dependent.
//! - Min-OS version: prefer `LC_BUILD_VERSION` (cmd 0x32), fall back
//!   to `LC_VERSION_MIN_MACOSX` (0x24) / `LC_VERSION_MIN_IPHONEOS`
//!   (0x25) / `LC_VERSION_MIN_TVOS` (0x2F) / `LC_VERSION_MIN_WATCHOS`
//!   (0x30). Format: `<platform>:<version>` (e.g. `macos:14.0`).
//!
//! The parsers operate on raw byte slices — same shape as ELF's
//! `parse_gnu_build_id` / `parse_debuglink` / `extract_runpath_entries`
//! in `binary/elf.rs`. Each parser returns an Option / Vec defensively
//! and never panics.

/// Mach-O magic bytes — distinguish 32/64-bit + LE/BE encoding.
const MH_MAGIC_64: u32 = 0xfeedfacf; // native-endian 64-bit
const MH_CIGAM_64: u32 = 0xcffaedfe; // byte-swapped 64-bit
const MH_MAGIC_32: u32 = 0xfeedface; // native-endian 32-bit
const MH_CIGAM_32: u32 = 0xcefaedfe; // byte-swapped 32-bit

const LC_REQ_DYLD: u32 = 0x80000000;
const LC_UUID: u32 = 0x1b;
const LC_RPATH: u32 = 0x1c | LC_REQ_DYLD;
const LC_VERSION_MIN_MACOSX: u32 = 0x24;
const LC_VERSION_MIN_IPHONEOS: u32 = 0x25;
const LC_VERSION_MIN_TVOS: u32 = 0x2f;
const LC_VERSION_MIN_WATCHOS: u32 = 0x30;
const LC_BUILD_VERSION: u32 = 0x32;
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
const LC_CODE_SIGNATURE: u32 = 0x1d;

/// SuperBlob magic — embedded codesign signature container.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xfade_0cc0;
/// CodeDirectory blob magic — the entry inside the SuperBlob carrying
/// identifier, flags, and team ID.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
const CSMAGIC_CODEDIRECTORY: u32 = 0xfade_0c02;

const PLATFORM_MACOS: u32 = 1;
const PLATFORM_IOS: u32 = 2;
const PLATFORM_TVOS: u32 = 3;
const PLATFORM_WATCHOS: u32 = 4;
const PLATFORM_BRIDGEOS: u32 = 5;
const PLATFORM_MACCATALYST: u32 = 6;
const PLATFORM_IOSSIMULATOR: u32 = 7;
const PLATFORM_TVOSSIMULATOR: u32 = 8;
const PLATFORM_WATCHOSSIMULATOR: u32 = 9;
const PLATFORM_DRIVERKIT: u32 = 10;
const PLATFORM_XROS: u32 = 11;

/// Detected Mach-O wire format. Returned by `decode_header`.
struct MachoHeader {
    /// True for little-endian encoding.
    little_endian: bool,
    /// Number of load commands.
    ncmds: u32,
    /// Total size of all load commands (concatenated).
    sizeofcmds: u32,
    /// Byte offset where the first load command starts (28 for 32-bit
    /// headers, 32 for 64-bit which has an extra `reserved` field).
    cmds_start: usize,
}

/// Read the Mach-O magic bytes + parse the header preamble.
/// Returns `None` for non-Mach-O bytes or truncated headers.
fn decode_header(bytes: &[u8]) -> Option<MachoHeader> {
    if bytes.len() < 32 {
        return None;
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let (is_64, little_endian) = match magic {
        MH_MAGIC_64 => (true, true),
        MH_CIGAM_64 => (true, false),
        MH_MAGIC_32 => (false, true),
        MH_CIGAM_32 => (false, false),
        _ => return None,
    };
    let read_u32 = |off: usize| -> Option<u32> {
        let arr: [u8; 4] = bytes.get(off..off + 4)?.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(arr)
        } else {
            u32::from_be_bytes(arr)
        })
    };
    let ncmds = read_u32(16)?;
    let sizeofcmds = read_u32(20)?;
    let cmds_start = if is_64 { 32 } else { 28 };
    if bytes.len() < cmds_start + sizeofcmds as usize {
        return None;
    }
    Some(MachoHeader {
        little_endian,
        ncmds,
        sizeofcmds,
        cmds_start,
    })
}

/// Helper: read u32 at `off` from `bytes` using the supplied endianness.
fn read_u32(bytes: &[u8], off: usize, little_endian: bool) -> Option<u32> {
    let arr: [u8; 4] = bytes.get(off..off + 4)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(arr)
    } else {
        u32::from_be_bytes(arr)
    })
}

/// Iterate load commands, calling `f` on each `(cmd, cmd_bytes)` pair.
/// `cmd_bytes` is the FULL command including the 8-byte header (cmd +
/// cmdsize). Stops when the iterator returns `Some(_)`.
fn for_each_load_command<F, T>(bytes: &[u8], header: &MachoHeader, mut f: F) -> Option<T>
where
    F: FnMut(u32, &[u8]) -> Option<T>,
{
    let mut cursor = header.cmds_start;
    let cmds_end = header.cmds_start + header.sizeofcmds as usize;
    if cmds_end > bytes.len() {
        return None;
    }
    for _ in 0..header.ncmds {
        if cursor + 8 > cmds_end {
            return None;
        }
        let cmd = read_u32(bytes, cursor, header.little_endian)?;
        let cmdsize = read_u32(bytes, cursor + 4, header.little_endian)? as usize;
        if cmdsize < 8 || cursor + cmdsize > cmds_end {
            return None;
        }
        if let Some(t) = f(cmd, &bytes[cursor..cursor + cmdsize]) {
            return Some(t);
        }
        cursor += cmdsize;
    }
    None
}

/// Parse a Mach-O byte slice's `LC_UUID` load command and return the
/// 16-byte UUID hex-encoded lowercase. Returns `None` for binaries
/// without LC_UUID (e.g. built with `ld -no_uuid`), non-Mach-O bytes,
/// or malformed headers.
pub fn parse_lc_uuid(bytes: &[u8]) -> Option<String> {
    let header = decode_header(bytes)?;
    for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        if cmd != LC_UUID {
            return None;
        }
        // LC_UUID payload: 8-byte header + 16 bytes of UUID.
        let uuid_bytes = cmd_bytes.get(8..24)?;
        let mut hex = String::with_capacity(32);
        for byte in uuid_bytes {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", byte);
        }
        Some(hex)
    })
}

/// Parse all `LC_RPATH` load commands and return their path strings in
/// declaration order, dedup'd. Each command's payload is an `LcStr`
/// (a 4-byte offset within the command pointing to a NUL-terminated
/// string). `$ORIGIN`-style substitutions are recorded raw — runtime
/// context-dependent expansion is the consumer's concern.
pub fn parse_lc_rpath(bytes: &[u8]) -> Vec<String> {
    let Some(header) = decode_header(bytes) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = Vec::new();
    let _: Option<()> = for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        if cmd != LC_RPATH {
            return None;
        }
        // RpathCommand layout: cmd(4) + cmdsize(4) + path_offset(4) + path bytes.
        let path_offset = read_u32(cmd_bytes, 8, header.little_endian)? as usize;
        if path_offset >= cmd_bytes.len() {
            return None;
        }
        // Read NUL-terminated string starting at path_offset within cmd.
        let str_bytes = &cmd_bytes[path_offset..];
        let nul_pos = str_bytes.iter().position(|&b| b == 0).unwrap_or(str_bytes.len());
        let path = std::str::from_utf8(&str_bytes[..nul_pos]).ok()?;
        let path = path.trim();
        if !path.is_empty() && !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
        None::<()> // continue iterating; never short-circuit
    });
    paths
}

/// Parse the minimum-OS version from `LC_BUILD_VERSION` (preferred)
/// or one of the legacy `LC_VERSION_MIN_*` commands. Returns
/// `<platform>:<version>` (e.g. `"macos:14.0"`, `"ios:17.5"`),
/// platform lowercase. Returns `None` if no version command is
/// present.
pub fn parse_min_os_version(bytes: &[u8]) -> Option<String> {
    let header = decode_header(bytes)?;

    // Pass 1: prefer LC_BUILD_VERSION (newer; carries platform enum).
    let from_build_version = for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        if cmd != LC_BUILD_VERSION {
            return None;
        }
        // BuildVersionCommand layout: cmd(4) + cmdsize(4) + platform(4) + minos(4) + sdk(4) + ntools(4).
        let platform_id = read_u32(cmd_bytes, 8, header.little_endian)?;
        let minos_packed = read_u32(cmd_bytes, 12, header.little_endian)?;
        let platform = platform_name(platform_id)?;
        Some(format!("{platform}:{}", decode_packed_version(minos_packed)))
    });
    if from_build_version.is_some() {
        return from_build_version;
    }

    // Pass 2: fall back to LC_VERSION_MIN_*. Synthesize platform from
    // the cmd value since these legacy commands don't carry an
    // explicit platform field.
    for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        let platform = match cmd {
            LC_VERSION_MIN_MACOSX => "macos",
            LC_VERSION_MIN_IPHONEOS => "ios",
            LC_VERSION_MIN_TVOS => "tvos",
            LC_VERSION_MIN_WATCHOS => "watchos",
            _ => return None,
        };
        // VersionMinCommand layout: cmd(4) + cmdsize(4) + version(4) + sdk(4).
        let version_packed = read_u32(cmd_bytes, 8, header.little_endian)?;
        Some(format!("{platform}:{}", decode_packed_version(version_packed)))
    })
}

/// Map an LC_BUILD_VERSION platform enum value to a lowercase string.
/// Unknown platform IDs return `None` (caller skips emission rather
/// than guess).
fn platform_name(id: u32) -> Option<&'static str> {
    Some(match id {
        PLATFORM_MACOS => "macos",
        PLATFORM_IOS => "ios",
        PLATFORM_TVOS => "tvos",
        PLATFORM_WATCHOS => "watchos",
        PLATFORM_BRIDGEOS => "bridgeos",
        PLATFORM_MACCATALYST => "maccatalyst",
        PLATFORM_IOSSIMULATOR => "iossimulator",
        PLATFORM_TVOSSIMULATOR => "tvossimulator",
        PLATFORM_WATCHOSSIMULATOR => "watchossimulator",
        PLATFORM_DRIVERKIT => "driverkit",
        PLATFORM_XROS => "xros",
        _ => return None,
    })
}

/// Decode Apple's nibble-packed version `xxxx.yy.zz` → "X.Y.Z".
/// The patch component is omitted when zero (matches `otool -l`'s
/// presentation of `14.0` rather than `14.0.0`).
fn decode_packed_version(packed: u32) -> String {
    let major = packed >> 16;
    let minor = (packed >> 8) & 0xff;
    let patch = packed & 0xff;
    if patch == 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

/// Parse a Mach-O byte slice's `LC_CODE_SIGNATURE` payload, walk the
/// embedded SuperBlob (Apple's all-big-endian cs_blobs format), and
/// return a slice into the binary's bytes covering the
/// `CSMAGIC_CODEDIRECTORY` blob's content (NOT including the
/// 8-byte SuperBlob index entry header). Returns `None` for binaries
/// without LC_CODE_SIGNATURE, malformed SuperBlob magic, or absence
/// of a CodeDirectory blob in the index.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
fn parse_codesign_codedirectory(bytes: &[u8]) -> Option<&[u8]> {
    let header = decode_header(bytes)?;
    // Find LC_CODE_SIGNATURE; payload is a LinkeditDataCommand —
    // 8-byte cmd/cmdsize header followed by 4-byte dataoff +
    // 4-byte datasize (both little-endian per the load-command
    // convention; the DATA they point at is BE).
    let (dataoff, datasize) = for_each_load_command(bytes, &header, |cmd, cmd_bytes| {
        if cmd != LC_CODE_SIGNATURE {
            return None;
        }
        if cmd_bytes.len() < 16 {
            return None;
        }
        let dataoff = read_u32(cmd_bytes, 8, header.little_endian)? as usize;
        let datasize = read_u32(cmd_bytes, 12, header.little_endian)? as usize;
        Some((dataoff, datasize))
    })?;
    if datasize < 12 {
        return None;
    }
    let sb_end = dataoff.checked_add(datasize)?;
    if sb_end > bytes.len() {
        return None;
    }
    let sb = &bytes[dataoff..sb_end];

    // SuperBlob preamble: BE u32 magic, BE u32 length, BE u32 count.
    let magic = read_u32(sb, 0, false)?;
    if magic != CSMAGIC_EMBEDDED_SIGNATURE {
        return None;
    }
    let count = read_u32(sb, 8, false)? as usize;
    // Each index entry: BE u32 type + BE u32 offset = 8 bytes.
    if 12 + count.checked_mul(8)? > sb.len() {
        return None;
    }
    for i in 0..count {
        let entry_off = 12 + i * 8;
        let blob_offset = read_u32(sb, entry_off + 4, false)? as usize;
        if blob_offset + 8 > sb.len() {
            continue;
        }
        let blob_magic = read_u32(sb, blob_offset, false)?;
        if blob_magic == CSMAGIC_CODEDIRECTORY {
            let blob_len = read_u32(sb, blob_offset + 4, false)? as usize;
            let blob_end = blob_offset.checked_add(blob_len)?;
            if blob_end > sb.len() || blob_len < 44 {
                return None;
            }
            return Some(&sb[blob_offset..blob_end]);
        }
    }
    None
}

/// Read a NUL-terminated UTF-8 string starting at `off` within a
/// CodeDirectory blob. Returns `None` for out-of-range offsets,
/// missing NUL terminator, or non-UTF-8 bytes.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
fn read_cd_cstring(cd: &[u8], off: usize) -> Option<String> {
    if off == 0 || off >= cd.len() {
        return None;
    }
    let tail = &cd[off..];
    let nul = tail.iter().position(|&b| b == 0)?;
    if nul == 0 {
        return None;
    }
    std::str::from_utf8(&tail[..nul]).ok().map(str::to_string)
}

/// Parse the codesign `CodeDirectory.identifier` — typically the
/// bundle ID (`com.apple.bash`) for app-signed binaries or the
/// basename for ad-hoc-signed binaries. Returns `None` when no
/// LC_CODE_SIGNATURE is present, the SuperBlob is malformed, or
/// the identifier offset doesn't resolve to a valid string.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
pub fn parse_codesign_identifier(bytes: &[u8]) -> Option<String> {
    let cd = parse_codesign_codedirectory(bytes)?;
    // CodeDirectory.identOffset is at byte offset 20 (BE u32).
    let ident_off = read_u32(cd, 20, false)? as usize;
    read_cd_cstring(cd, ident_off)
}

/// Parse the codesign `CodeDirectory.flags` u32 bitfield into a
/// human-readable JSON-array-of-names representation. Returns an
/// empty Vec when no LC_CODE_SIGNATURE / malformed SuperBlob /
/// flags == 0. Always alphabetically sorted for determinism.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
pub fn parse_codesign_flags(bytes: &[u8]) -> Vec<String> {
    let Some(cd) = parse_codesign_codedirectory(bytes) else {
        return Vec::new();
    };
    let Some(flags) = read_u32(cd, 12, false) else {
        return Vec::new();
    };
    if flags == 0 {
        return Vec::new();
    }
    decode_codesign_flags(flags)
}

/// Parse the codesign `CodeDirectory.teamOffset` → team-identifier
/// string (10-character alphanumeric Apple Team ID for cert-signed
/// binaries; absent for ad-hoc signatures). The team-id field
/// requires CodeDirectory version ≥ `0x20200`; older CDs return
/// `None` here.
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
pub fn parse_codesign_team_id(bytes: &[u8]) -> Option<String> {
    let cd = parse_codesign_codedirectory(bytes)?;
    let cd_version = read_u32(cd, 8, false)?;
    if cd_version < 0x0002_0200 {
        return None;
    }
    // CodeDirectory.teamOffset is at byte offset 48 (BE u32).
    let team_off = read_u32(cd, 48, false)? as usize;
    read_cd_cstring(cd, team_off)
}

/// Decode the CodeDirectory.flags u32 bitfield into a sorted Vec
/// of canonical Apple flag names. Unrecognized bits emit as
/// `unknown-0x<hex>` to preserve information without committing
/// to names that may shift across macOS versions.
///
/// Bit names sourced from Apple's Security project
/// (`cs_blobs.h` / `cscdefs.h.auto.html`).
#[allow(dead_code)] // wired in commit 2 (030/wire-up-bag)
fn decode_codesign_flags(value: u32) -> Vec<String> {
    let known: &[(u32, &str)] = &[
        (0x0000_0001, "host"),
        (0x0000_0002, "adhoc"),
        (0x0000_0004, "get-task-allow"),
        (0x0000_0008, "installer"),
        (0x0000_0010, "force-hard"),
        (0x0000_0020, "force-kill"),
        (0x0000_0040, "force-expiration"),
        (0x0000_0080, "restrict"),
        (0x0000_0100, "enforcement"),
        (0x0000_0200, "library-validation"),
        (0x0001_0000, "hardened-runtime"),
        (0x0002_0000, "linker-signed"),
    ];
    let mut out: Vec<String> = Vec::new();
    let mut seen: u32 = 0;
    for &(bit, name) in known {
        if value & bit != 0 {
            out.push(name.to_string());
            seen |= bit;
        }
    }
    let unknown = value & !seen;
    let mut bit: u32 = 1;
    while bit != 0 {
        if unknown & bit != 0 {
            out.push(format!("unknown-0x{bit:x}"));
        }
        bit = bit.checked_shl(1).unwrap_or(0);
    }
    out.sort();
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Helper: build a minimal 64-bit little-endian Mach-O binary
    /// header + concatenated load commands. The body bytes after
    /// `cmds` are zero-padded — sufficient for the parser's needs
    /// since we only walk the load-command region.
    fn build_macho_64_le(cmds: &[Vec<u8>]) -> Vec<u8> {
        let sizeofcmds: u32 = cmds.iter().map(|c| c.len() as u32).sum();
        let ncmds: u32 = cmds.len() as u32;
        let mut out = Vec::new();
        // mach_header_64: magic + cputype + cpusubtype + filetype + ncmds + sizeofcmds + flags + reserved.
        out.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // cputype (x86_64 placeholder)
        out.extend_from_slice(&0u32.to_le_bytes()); // cpusubtype
        out.extend_from_slice(&0u32.to_le_bytes()); // filetype (MH_EXECUTE etc.)
        out.extend_from_slice(&ncmds.to_le_bytes());
        out.extend_from_slice(&sizeofcmds.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        for cmd in cmds {
            out.extend_from_slice(cmd);
        }
        out
    }

    /// Build an LC_UUID load command with the given UUID bytes.
    fn build_lc_uuid(uuid: [u8; 16]) -> Vec<u8> {
        let mut cmd = Vec::with_capacity(24);
        cmd.extend_from_slice(&LC_UUID.to_le_bytes());
        cmd.extend_from_slice(&24u32.to_le_bytes()); // cmdsize
        cmd.extend_from_slice(&uuid);
        cmd
    }

    /// Build an LC_RPATH load command with the given path string.
    /// Layout: cmd(4) + cmdsize(4) + path_offset(4) + path NUL-terminated + 4-byte alignment pad.
    fn build_lc_rpath(path: &str) -> Vec<u8> {
        let path_offset: u32 = 12; // immediately after the 12-byte header
        let mut payload: Vec<u8> = Vec::new();
        payload.extend_from_slice(path.as_bytes());
        payload.push(0); // NUL
        // Pad to 4-byte alignment of the total cmd size.
        let header_size = 12;
        let total_unpadded = header_size + payload.len();
        let total = (total_unpadded + 3) & !3;
        let cmdsize = total as u32;
        let pad = total - total_unpadded;

        let mut cmd = Vec::with_capacity(total);
        cmd.extend_from_slice(&LC_RPATH.to_le_bytes());
        cmd.extend_from_slice(&cmdsize.to_le_bytes());
        cmd.extend_from_slice(&path_offset.to_le_bytes());
        cmd.extend_from_slice(&payload);
        cmd.extend(std::iter::repeat_n(0u8, pad));
        cmd
    }

    /// Build an LC_BUILD_VERSION load command with platform + minos.
    fn build_lc_build_version(platform: u32, packed_minos: u32) -> Vec<u8> {
        let mut cmd = Vec::with_capacity(24);
        cmd.extend_from_slice(&LC_BUILD_VERSION.to_le_bytes());
        cmd.extend_from_slice(&24u32.to_le_bytes()); // cmdsize (no tools)
        cmd.extend_from_slice(&platform.to_le_bytes());
        cmd.extend_from_slice(&packed_minos.to_le_bytes());
        cmd.extend_from_slice(&0u32.to_le_bytes()); // sdk (unused)
        cmd.extend_from_slice(&0u32.to_le_bytes()); // ntools = 0
        cmd
    }

    /// Build an LC_VERSION_MIN_* load command.
    fn build_lc_version_min(cmd_id: u32, packed_version: u32) -> Vec<u8> {
        let mut cmd = Vec::with_capacity(16);
        cmd.extend_from_slice(&cmd_id.to_le_bytes());
        cmd.extend_from_slice(&16u32.to_le_bytes()); // cmdsize
        cmd.extend_from_slice(&packed_version.to_le_bytes());
        cmd.extend_from_slice(&0u32.to_le_bytes()); // sdk
        cmd
    }

    #[test]
    fn parse_lc_uuid_from_synthetic_macho() {
        let uuid = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
        ];
        let bytes = build_macho_64_le(&[build_lc_uuid(uuid)]);
        assert_eq!(
            parse_lc_uuid(&bytes).as_deref(),
            Some("123456789abcdef01122334455667788"),
        );
    }

    #[test]
    fn parse_lc_uuid_returns_none_when_no_uuid_command() {
        // Mach-O with only LC_BUILD_VERSION; no LC_UUID.
        let bytes = build_macho_64_le(&[build_lc_build_version(
            PLATFORM_MACOS,
            packed_version(14, 0, 0),
        )]);
        assert!(parse_lc_uuid(&bytes).is_none());
    }

    #[test]
    fn parse_lc_uuid_returns_none_for_non_macho_bytes() {
        // ELF magic — not Mach-O.
        let bytes = b"\x7fELF\x02\x01\x01\x00";
        assert!(parse_lc_uuid(bytes).is_none());
    }

    #[test]
    fn parse_lc_rpath_collects_multiple_paths_dedup() {
        let bytes = build_macho_64_le(&[
            build_lc_rpath("@executable_path/../Frameworks"),
            build_lc_rpath("/usr/local/lib"),
            build_lc_rpath("@executable_path/../Frameworks"), // duplicate
        ]);
        let paths = parse_lc_rpath(&bytes);
        assert_eq!(
            paths,
            vec![
                "@executable_path/../Frameworks".to_string(),
                "/usr/local/lib".to_string(),
            ]
        );
    }

    #[test]
    fn parse_lc_rpath_empty_when_no_rpath_command() {
        // Only LC_UUID present.
        let bytes = build_macho_64_le(&[build_lc_uuid([0xaa; 16])]);
        assert!(parse_lc_rpath(&bytes).is_empty());
    }

    /// Helper: encode X.Y.Z into Apple's nibble-packed format.
    fn packed_version(major: u32, minor: u32, patch: u32) -> u32 {
        (major << 16) | ((minor & 0xff) << 8) | (patch & 0xff)
    }

    #[test]
    fn parse_min_os_version_prefers_lc_build_version() {
        // Both LC_BUILD_VERSION (macOS 14.0) and the legacy
        // LC_VERSION_MIN_MACOSX (10.13.0) present — the parser
        // should pick LC_BUILD_VERSION.
        let bytes = build_macho_64_le(&[
            build_lc_version_min(LC_VERSION_MIN_MACOSX, packed_version(10, 13, 0)),
            build_lc_build_version(PLATFORM_MACOS, packed_version(14, 0, 0)),
        ]);
        assert_eq!(
            parse_min_os_version(&bytes).as_deref(),
            Some("macos:14.0"),
        );
    }

    #[test]
    fn parse_min_os_version_falls_back_to_lc_version_min_macosx() {
        // No LC_BUILD_VERSION; only legacy LC_VERSION_MIN_MACOSX.
        let bytes = build_macho_64_le(&[build_lc_version_min(
            LC_VERSION_MIN_MACOSX,
            packed_version(10, 13, 4),
        )]);
        assert_eq!(
            parse_min_os_version(&bytes).as_deref(),
            Some("macos:10.13.4"),
        );
    }

    #[test]
    fn parse_min_os_version_handles_ios_platform() {
        let bytes = build_macho_64_le(&[build_lc_build_version(
            PLATFORM_IOS,
            packed_version(17, 5, 0),
        )]);
        assert_eq!(
            parse_min_os_version(&bytes).as_deref(),
            Some("ios:17.5"),
        );
    }

    #[test]
    fn parse_min_os_version_returns_none_when_no_version_command() {
        let bytes = build_macho_64_le(&[build_lc_uuid([0u8; 16])]);
        assert!(parse_min_os_version(&bytes).is_none());
    }

    #[test]
    fn decode_header_rejects_truncated() {
        // Magic OK, but header truncated below the 32-byte minimum.
        let bytes = &MH_MAGIC_64.to_le_bytes()[..];
        assert!(decode_header(bytes).is_none());
    }

    // ====================================================================
    // Milestone 030 — LC_CODE_SIGNATURE / SuperBlob / CodeDirectory tests
    // ====================================================================

    /// Build a synthetic CodeDirectory blob (Apple's BE format).
    /// `version` controls the layout (≥ 0x20200 includes teamOffset).
    /// Returns the full blob: magic + length + content. Identifier
    /// goes immediately after the fixed-size header; team_id (when
    /// applicable) goes immediately after the identifier.
    fn build_codedirectory_blob(
        version: u32,
        flags: u32,
        identifier: &str,
        team_id: Option<&str>,
    ) -> Vec<u8> {
        // CodeDirectory fixed header (we populate the fields we read):
        //   0  magic        u32 BE  (0xfade0c02)
        //   4  length       u32 BE  (total blob size)
        //   8  version      u32 BE
        //  12  flags        u32 BE
        //  16  hashOffset   u32 BE  (we don't read; set to 0)
        //  20  identOffset  u32 BE  → offset within blob to NUL-term identifier
        //  24  nSpecialSlots u32 BE
        //  28  nCodeSlots    u32 BE
        //  32  codeLimit     u32 BE
        //  36  hashSize      u8
        //  37  hashType      u8
        //  38  platform      u8
        //  39  pageSize      u8
        //  40  spare2        u32 BE
        // (v ≥ 0x20100):
        //  44  scatterOffset u32 BE
        // (v ≥ 0x20200):
        //  48  teamOffset    u32 BE  → offset within blob to NUL-term team-id
        // (v ≥ 0x20300):
        //  52  spare3        u32 BE
        //  56  codeLimit64   u64 BE
        // (v ≥ 0x20400):
        //  64  execSegBase   u64 BE
        //  72  execSegLimit  u64 BE
        //  80  execSegFlags  u64 BE
        let header_size: u32 = if version >= 0x0002_0400 {
            88
        } else if version >= 0x0002_0300 {
            64
        } else if version >= 0x0002_0200 {
            52
        } else if version >= 0x0002_0100 {
            48
        } else {
            44
        };

        // Lay out strings after the header.
        let ident_offset = header_size;
        let mut strings: Vec<u8> = Vec::new();
        strings.extend_from_slice(identifier.as_bytes());
        strings.push(0);
        let team_offset_value: u32 = if version >= 0x0002_0200 {
            if let Some(team) = team_id {
                let off = header_size + strings.len() as u32;
                strings.extend_from_slice(team.as_bytes());
                strings.push(0);
                off
            } else {
                0 // CD has the field but team-id absent (ad-hoc-style)
            }
        } else {
            0 // CD too old to carry teamOffset
        };

        let total_len = header_size + strings.len() as u32;

        let mut blob: Vec<u8> = Vec::with_capacity(total_len as usize);
        blob.extend_from_slice(&CSMAGIC_CODEDIRECTORY.to_be_bytes()); // 0
        blob.extend_from_slice(&total_len.to_be_bytes()); // 4
        blob.extend_from_slice(&version.to_be_bytes()); // 8
        blob.extend_from_slice(&flags.to_be_bytes()); // 12
        blob.extend_from_slice(&0u32.to_be_bytes()); // 16 hashOffset
        blob.extend_from_slice(&ident_offset.to_be_bytes()); // 20 identOffset
        blob.extend_from_slice(&0u32.to_be_bytes()); // 24 nSpecialSlots
        blob.extend_from_slice(&0u32.to_be_bytes()); // 28 nCodeSlots
        blob.extend_from_slice(&0u32.to_be_bytes()); // 32 codeLimit
        blob.push(0); // 36 hashSize
        blob.push(0); // 37 hashType
        blob.push(0); // 38 platform
        blob.push(0); // 39 pageSize
        blob.extend_from_slice(&0u32.to_be_bytes()); // 40 spare2
        if version >= 0x0002_0100 {
            blob.extend_from_slice(&0u32.to_be_bytes()); // 44 scatterOffset
        }
        if version >= 0x0002_0200 {
            blob.extend_from_slice(&team_offset_value.to_be_bytes()); // 48
        }
        if version >= 0x0002_0300 {
            blob.extend_from_slice(&0u32.to_be_bytes()); // 52 spare3
            blob.extend_from_slice(&0u64.to_be_bytes()); // 56 codeLimit64
        }
        if version >= 0x0002_0400 {
            blob.extend_from_slice(&0u64.to_be_bytes()); // 64 execSegBase
            blob.extend_from_slice(&0u64.to_be_bytes()); // 72 execSegLimit
            blob.extend_from_slice(&0u64.to_be_bytes()); // 80 execSegFlags
        }
        blob.extend_from_slice(&strings);

        debug_assert_eq!(blob.len() as u32, total_len);
        blob
    }

    /// Build a SuperBlob containing a single CodeDirectory blob.
    /// The 8-byte SuperBlob preamble + 8-byte index entry put the
    /// CD blob at offset 20 within the SuperBlob.
    fn build_codesign_superblob(cd_blob: &[u8]) -> Vec<u8> {
        // SuperBlob layout (all BE):
        //   0  magic   u32  (0xfade0cc0)
        //   4  length  u32  (total SB size)
        //   8  count   u32  (= 1 here)
        //  12  type    u32  (per-index entry; we use 0 = CD)
        //  16  offset  u32  (offset within SB to the blob)
        //  20  <CD blob bytes>
        let blob_offset: u32 = 20;
        let total_len: u32 = blob_offset + cd_blob.len() as u32;
        let mut sb: Vec<u8> = Vec::with_capacity(total_len as usize);
        sb.extend_from_slice(&CSMAGIC_EMBEDDED_SIGNATURE.to_be_bytes());
        sb.extend_from_slice(&total_len.to_be_bytes());
        sb.extend_from_slice(&1u32.to_be_bytes()); // count
        sb.extend_from_slice(&0u32.to_be_bytes()); // type 0 = CodeDirectory
        sb.extend_from_slice(&blob_offset.to_be_bytes());
        sb.extend_from_slice(cd_blob);
        sb
    }

    /// Assemble a Mach-O 64-LE binary with an LC_CODE_SIGNATURE
    /// load command pointing at a SuperBlob appended after the
    /// load commands. Returns the full byte image.
    fn build_macho_with_codesign(superblob: &[u8]) -> Vec<u8> {
        // dataoff = mach_header (32) + LC_CODE_SIGNATURE (16) = 48.
        let dataoff: u32 = 32 + 16;
        let datasize: u32 = superblob.len() as u32;

        // LinkeditDataCommand (16 bytes) — endianness matches the
        // Mach-O header, so LE here.
        let mut lc = Vec::with_capacity(16);
        lc.extend_from_slice(&LC_CODE_SIGNATURE.to_le_bytes());
        lc.extend_from_slice(&16u32.to_le_bytes()); // cmdsize
        lc.extend_from_slice(&dataoff.to_le_bytes());
        lc.extend_from_slice(&datasize.to_le_bytes());

        let mut macho = build_macho_64_le(&[lc]);
        macho.extend_from_slice(superblob);
        macho
    }

    #[test]
    fn parse_codesign_identifier_from_synthetic_superblob() {
        let cd = build_codedirectory_blob(
            0x0002_0400,
            0x0001_0000, // hardened-runtime
            "com.example.myapp",
            Some("EQHXZ8M8AV"),
        );
        let sb = build_codesign_superblob(&cd);
        let macho = build_macho_with_codesign(&sb);
        assert_eq!(
            parse_codesign_identifier(&macho).as_deref(),
            Some("com.example.myapp"),
        );
    }

    #[test]
    fn parse_codesign_flags_decodes_hardened_runtime() {
        let cd = build_codedirectory_blob(
            0x0002_0400,
            0x0001_0000,
            "x",
            None,
        );
        let macho = build_macho_with_codesign(&build_codesign_superblob(&cd));
        assert_eq!(
            parse_codesign_flags(&macho),
            vec!["hardened-runtime".to_string()],
        );
    }

    #[test]
    fn parse_codesign_flags_handles_multi_flag_bitfield() {
        let cd = build_codedirectory_blob(
            0x0002_0400,
            0x0001_0200, // hardened-runtime | library-validation
            "x",
            None,
        );
        let macho = build_macho_with_codesign(&build_codesign_superblob(&cd));
        assert_eq!(
            parse_codesign_flags(&macho),
            vec!["hardened-runtime".to_string(), "library-validation".to_string()],
        );
    }

    #[test]
    fn parse_codesign_flags_emits_unknown_for_unrecognized_bits() {
        let cd = build_codedirectory_blob(
            0x0002_0400,
            0x0040_0000, // unrecognized bit
            "x",
            None,
        );
        let macho = build_macho_with_codesign(&build_codesign_superblob(&cd));
        let flags = parse_codesign_flags(&macho);
        assert_eq!(flags, vec!["unknown-0x400000".to_string()]);
    }

    #[test]
    fn parse_codesign_team_id_skips_when_cd_version_too_old() {
        // CD v0x20100 doesn't carry teamOffset → returns None.
        let cd = build_codedirectory_blob(
            0x0002_0100,
            0x0000_0002, // adhoc
            "ad.hoc",
            None,
        );
        let macho = build_macho_with_codesign(&build_codesign_superblob(&cd));
        // Identifier still parses; team_id does not.
        assert_eq!(
            parse_codesign_identifier(&macho).as_deref(),
            Some("ad.hoc"),
        );
        assert_eq!(parse_codesign_team_id(&macho), None);
        assert_eq!(parse_codesign_flags(&macho), vec!["adhoc".to_string()]);
    }

    #[test]
    fn parse_codesign_team_id_extracts_when_cd_version_supports_it() {
        let cd = build_codedirectory_blob(
            0x0002_0400,
            0x0001_0000,
            "com.example",
            Some("ABC1234XYZ"),
        );
        let macho = build_macho_with_codesign(&build_codesign_superblob(&cd));
        assert_eq!(
            parse_codesign_team_id(&macho).as_deref(),
            Some("ABC1234XYZ"),
        );
    }

    #[test]
    fn parse_codesign_returns_none_for_no_lc_code_signature() {
        // Plain Mach-O with only LC_UUID — no LC_CODE_SIGNATURE.
        let bytes = build_macho_64_le(&[build_lc_uuid([0u8; 16])]);
        assert_eq!(parse_codesign_identifier(&bytes), None);
        assert!(parse_codesign_flags(&bytes).is_empty());
        assert_eq!(parse_codesign_team_id(&bytes), None);
    }

    #[test]
    fn parse_codesign_returns_none_for_malformed_superblob_magic() {
        // Build a Mach-O whose LC_CODE_SIGNATURE points at bytes
        // that don't begin with CSMAGIC_EMBEDDED_SIGNATURE.
        let dataoff: u32 = 32 + 16;
        let bogus_payload = [0xDE, 0xAD, 0xBE, 0xEF, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8];
        let datasize: u32 = bogus_payload.len() as u32;

        let mut lc = Vec::with_capacity(16);
        lc.extend_from_slice(&LC_CODE_SIGNATURE.to_le_bytes());
        lc.extend_from_slice(&16u32.to_le_bytes());
        lc.extend_from_slice(&dataoff.to_le_bytes());
        lc.extend_from_slice(&datasize.to_le_bytes());

        let mut macho = build_macho_64_le(&[lc]);
        macho.extend_from_slice(&bogus_payload);

        assert_eq!(parse_codesign_identifier(&macho), None);
        assert!(parse_codesign_flags(&macho).is_empty());
        assert_eq!(parse_codesign_team_id(&macho), None);
    }
}
